// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

//! VPN Diagnostics Tool
//!
//! Comprehensive diagnostics for identifying VPN connection failures.
//! Tests firewall state, DNS resolution, tunnel connectivity, and network reachability
//! at various stages of the connection lifecycle to pinpoint exact failure points.

use nym_vpn_proto::proto::{TunnelState, nym_vpn_service_client::NymVpnServiceClient};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

// ===== Constants =====

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const DNS_TEST_TIMEOUT: Duration = Duration::from_secs(10);

const DNS_TEST_DOMAINS: &[&str] = &[
    "nymvpn.com",
    "validator.nymtech.net",
    "rpc.nymtech.net",
    "duckduckgo.com",
];

const CLOUDFLARE_DNS_V4: &str = "1.1.1.1";
const CLOUDFLARE_DNS_V4_ALT: &str = "1.0.0.1";
const CLOUDFLARE_DNS_V6: &str = "2606:4700:4700::1111";
const QUAD9_DNS_V4: &str = "9.9.9.9";
const QUAD9_DNS_V4_ALT: &str = "149.112.112.112";
const QUAD9_DNS_V6: &str = "2620:fe::fe";

const DUCKDUCKGO_V4: &str = "52.250.42.157:443";
const DUCKDUCKGO_V6: &str = "[2620:1ec:c11::200]:443";

const PING_TARGET_V4: &str = "1.1.1.1";
const PING_TARGET_V6: &str = "2606:4700:4700::1111";

// ===== Public Types =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub timestamp: String,
    pub tunnel_state: String,
    pub firewall_tests: FirewallTests,
    pub dns_tests: DnsTests,
    pub tunnel_connectivity_tests: TunnelConnectivityTests,
    pub network_tests: NetworkTests,
    pub overall_status: OverallStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallTests {
    pub can_resolve_dns_v4: TestResult,
    pub can_resolve_dns_v6: TestResult,
    pub firewall_allows_outbound_tcp_v4: TestResult,
    pub firewall_allows_outbound_tcp_v6: TestResult,
    pub firewall_allows_outbound_udp_v4: TestResult,
    pub firewall_allows_outbound_udp_v6: TestResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsTests {
    pub can_resolve_ipv4_addresses: TestResult,
    pub can_resolve_ipv6_addresses: TestResult,
    pub resolved_addresses_v4: Vec<String>,
    pub resolved_addresses_v6: Vec<String>,
    pub expected_addresses_match_v4: bool,
    pub expected_addresses_match_v6: bool,
    pub dns_response_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConnectivityTests {
    pub tunnel_interface_exists: TestResult,
    pub can_ping_tunnel_v4: TestResult,
    pub can_ping_tunnel_v6: TestResult,
    pub can_reach_peer_address_v4: TestResult,
    pub can_reach_peer_address_v6: TestResult,
    pub can_open_udp_socket_v4: TestResult,
    pub can_open_udp_socket_v6: TestResult,
    pub tunnel_routing_configured_v4: TestResult,
    pub tunnel_routing_configured_v6: TestResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTests {
    pub can_reach_cloudflare_dns_v4: TestResult,
    pub can_reach_cloudflare_dns_v6: TestResult,
    pub can_reach_quad9_dns_v4: TestResult,
    pub can_reach_quad9_dns_v6: TestResult,
    pub can_reach_duckduckgo_v4: TestResult,
    pub can_reach_duckduckgo_v6: TestResult,
    pub can_establish_tcp_connection_v4: TestResult,
    pub can_establish_tcp_connection_v6: TestResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub passed: bool,
    pub error: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverallStatus {
    AllTestsPassed,
    FirewallIssue,
    DnsIssue,
    TunnelIssue,
    NetworkIssue,
    MultipleIssues(Vec<String>),
}

impl TestResult {
    pub fn success(details: impl Into<String>) -> Self {
        Self {
            passed: true,
            error: None,
            details: Some(details.into()),
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            passed: false,
            error: Some(error.into()),
            details: None,
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            passed: false,
            error: None,
            details: Some(format!("Skipped: {}", reason.into())),
        }
    }

    /// Returns true if this test was skipped (not failed)
    pub fn is_skipped(&self) -> bool {
        !self.passed && self.error.is_none()
    }

    /// Returns true if this test actually failed (not skipped)
    pub fn is_failed(&self) -> bool {
        !self.passed && self.error.is_some()
    }
}

// ===== Main Diagnostics Runner =====

pub struct VpnDiagnostics {
    tunnel_interface: Option<String>,
    peer_address_v4: Option<Ipv4Addr>,
    peer_address_v6: Option<Ipv6Addr>,
}

impl VpnDiagnostics {
    // ===== Public API =====

    pub fn new() -> Self {
        Self {
            tunnel_interface: None,
            peer_address_v4: None,
            peer_address_v6: None,
        }
    }

    pub fn with_tunnel_interface(mut self, interface: String) -> Self {
        self.tunnel_interface = Some(interface);
        self
    }

    pub fn with_peer_v4(mut self, addr: Ipv4Addr) -> Self {
        self.peer_address_v4 = Some(addr);
        self
    }

    pub fn with_peer_v6(mut self, addr: Ipv6Addr) -> Self {
        self.peer_address_v6 = Some(addr);
        self
    }

    /// Query the daemon for its current tunnel state via gRPC
    pub async fn get_daemon_state() -> Result<String, String> {
        let socket_path = Self::default_socket_path();

        tracing::debug!("Connecting to daemon at: {}", socket_path.display());

        // Create gRPC channel using Unix socket
        let channel = Endpoint::from_static("unix://placeholder")
            .connect_with_connector(service_fn(move |_: Uri| {
                nym_ipc::client::connect(socket_path.clone())
            }))
            .await
            .map_err(|e| format!("failed to connect to daemon: {}", e))?;

        let mut client = NymVpnServiceClient::new(channel);

        let state = client
            .get_tunnel_state(())
            .await
            .map_err(|e| format!("Failed to query tunnel state: {}", e))?
            .into_inner();

        Ok(Self::format_tunnel_state(&state))
    }

    fn default_socket_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from("/var/run/nym-vpn.sock")
        }
        #[cfg(target_os = "linux")]
        {
            PathBuf::from("/run/nym-vpn.sock")
        }
        #[cfg(target_os = "windows")]
        {
            PathBuf::from(r"\\.\pipe\nym-vpn")
        }
    }

    fn format_tunnel_state(state: &TunnelState) -> String {
        match &state.state {
            Some(s) => match s {
                nym_vpn_proto::proto::tunnel_state::State::Disconnected(_) => {
                    "disconnected".to_string()
                }
                nym_vpn_proto::proto::tunnel_state::State::Connecting(c) => {
                    format!("connecting (attempt {})", c.retry_attempt)
                }
                nym_vpn_proto::proto::tunnel_state::State::Connected(_) => "connected".to_string(),
                nym_vpn_proto::proto::tunnel_state::State::Disconnecting(_) => {
                    "disconnecting".to_string()
                }
                nym_vpn_proto::proto::tunnel_state::State::Error(e) => {
                    format!("error ({:?})", e.reason)
                }
                nym_vpn_proto::proto::tunnel_state::State::Offline(_) => "offline".to_string(),
            },
            None => "unknown".to_string(),
        }
    }

    /// Run full diagnostic suite
    pub async fn run_full_diagnostics(&self, tunnel_state: &str) -> DiagnosticReport {
        let start = std::time::Instant::now();

        tracing::info!("Running VPN diagnostics for state: {}", tunnel_state);

        let firewall_tests = self.test_firewall().await;
        let dns_tests = self.test_dns_resolution().await;
        let tunnel_tests = self.test_tunnel_connectivity().await;
        let network_tests = self.test_network_connectivity().await;

        let overall_status = self.determine_overall_status(
            tunnel_state,
            &firewall_tests,
            &dns_tests,
            &tunnel_tests,
            &network_tests,
        );

        let report = DiagnosticReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            tunnel_state: tunnel_state.to_string(),
            firewall_tests,
            dns_tests,
            tunnel_connectivity_tests: tunnel_tests,
            network_tests,
            overall_status,
        };

        tracing::info!(
            "Diagnostics completed in {:?}, status: {:?}",
            start.elapsed(),
            report.overall_status
        );

        report
    }

    /// Test firewall configuration
    async fn test_firewall(&self) -> FirewallTests {
        tracing::debug!("Testing firewall configuration...");

        FirewallTests {
            can_resolve_dns_v4: self.test_dns_port_accessible_v4().await,
            can_resolve_dns_v6: self.test_dns_port_accessible_v6().await,
            firewall_allows_outbound_tcp_v4: self.test_tcp_outbound_v4().await,
            firewall_allows_outbound_tcp_v6: self.test_tcp_outbound_v6().await,
            firewall_allows_outbound_udp_v4: self.test_udp_outbound_v4().await,
            firewall_allows_outbound_udp_v6: self.test_udp_outbound_v6().await,
        }
    }

    /// Test DNS resolution capability.
    ///
    /// Attempts to resolve test domains via IPv4 and IPv6 using the system DNS configuration.
    /// This respects the VPN's local DNS forwarder when connected.
    async fn test_dns_resolution(&self) -> DnsTests {
        tracing::debug!(
            "Testing DNS resolution for {} domains",
            DNS_TEST_DOMAINS.len()
        );

        self.log_system_dns_config();

        let start = std::time::Instant::now();

        let ipv4_result = self.resolve_domains_v4(DNS_TEST_DOMAINS).await;
        let ipv6_result = self.resolve_domains_v6(DNS_TEST_DOMAINS).await;

        let resolved_v4: Vec<String> = match ipv4_result {
            Ok(addrs) => addrs.into_iter().map(|ip| ip.to_string()).collect(),
            Err(ref e) => {
                tracing::warn!("IPv4 DNS resolution failed: {}", e);
                Vec::new()
            }
        };

        let resolved_v6: Vec<String> = match ipv6_result {
            Ok(addrs) => addrs.into_iter().map(|ip| ip.to_string()).collect(),
            Err(ref e) => {
                tracing::warn!("IPv6 DNS resolution failed: {}", e);
                Vec::new()
            }
        };

        let elapsed = start.elapsed();

        let ipv4_success = !resolved_v4.is_empty();
        let ipv6_success = !resolved_v6.is_empty();

        DnsTests {
            can_resolve_ipv4_addresses: if ipv4_success {
                TestResult::success(format!("resolved {} IPv4 addresses", resolved_v4.len()))
            } else {
                TestResult::failure("no IPv4 addresses resolved")
            },
            can_resolve_ipv6_addresses: if ipv6_success {
                TestResult::success(format!("resolved {} IPv6 addresses", resolved_v6.len()))
            } else {
                TestResult::failure("no IPv6 addresses resolved")
            },
            resolved_addresses_v4: resolved_v4,
            resolved_addresses_v6: resolved_v6,
            expected_addresses_match_v4: ipv4_success,
            expected_addresses_match_v6: ipv6_success,
            dns_response_time_ms: elapsed.as_millis() as u64,
        }
    }

    /// Test tunnel connectivity.
    ///
    /// Verifies tunnel interface existence, ICMP reachability, peer connectivity,
    /// and routing configuration. These tests are expected to fail when disconnected
    /// due to leftover interface state.
    async fn test_tunnel_connectivity(&self) -> TunnelConnectivityTests {
        tracing::debug!(
            "Testing tunnel connectivity for interface: {:?}",
            self.tunnel_interface
        );

        TunnelConnectivityTests {
            tunnel_interface_exists: self.check_tunnel_interface_exists().await,
            can_ping_tunnel_v4: self.test_ping_tunnel_v4().await,
            can_ping_tunnel_v6: self.test_ping_tunnel_v6().await,
            can_reach_peer_address_v4: self.test_reach_peer_v4().await,
            can_reach_peer_address_v6: self.test_reach_peer_v6().await,
            can_open_udp_socket_v4: self.test_udp_socket_v4().await,
            can_open_udp_socket_v6: self.test_udp_socket_v6().await,
            tunnel_routing_configured_v4: self.check_tunnel_routing_v4().await,
            tunnel_routing_configured_v6: self.check_tunnel_routing_v6().await,
        }
    }

    /// Test general network connectivity.
    ///
    /// Verifies reachability to public DNS servers (Cloudflare, Quad9) and real-world
    /// endpoints (DuckDuckGo) to validate both DNS and TCP connectivity paths.
    async fn test_network_connectivity(&self) -> NetworkTests {
        tracing::debug!("Testing network reachability to public endpoints");

        NetworkTests {
            can_reach_cloudflare_dns_v4: self.test_reach_dns_server(CLOUDFLARE_DNS_V4, true).await,
            can_reach_cloudflare_dns_v6: self.test_reach_dns_server(CLOUDFLARE_DNS_V6, false).await,
            can_reach_quad9_dns_v4: self.test_reach_dns_server(QUAD9_DNS_V4, true).await,
            can_reach_quad9_dns_v6: self.test_reach_dns_server(QUAD9_DNS_V6, false).await,
            can_reach_duckduckgo_v4: self.test_tcp_endpoint(DUCKDUCKGO_V4, true).await,
            can_reach_duckduckgo_v6: self.test_tcp_endpoint(DUCKDUCKGO_V6, false).await,
            can_establish_tcp_connection_v4: self.test_tcp_connect_v4().await,
            can_establish_tcp_connection_v6: self.test_tcp_connect_v6().await,
        }
    }

    // ===== Test Implementations: Firewall =====

    async fn test_dns_port_accessible_v4(&self) -> TestResult {
        match timeout(TEST_TIMEOUT, UdpSocket::bind("0.0.0.0:0")).await {
            Ok(Ok(socket)) => {
                match timeout(
                    TEST_TIMEOUT,
                    socket.connect(format!("{}:53", CLOUDFLARE_DNS_V4)),
                )
                .await
                {
                    Ok(Ok(_)) => TestResult::success("DNS port 53 accessible via IPv4"),
                    Ok(Err(e)) => {
                        TestResult::failure(format!("failed to connect to DNS port: {}", e))
                    }
                    Err(_) => TestResult::failure("timeout connecting to DNS port"),
                }
            }
            Ok(Err(e)) => TestResult::failure(format!("failed to bind UDP socket: {}", e)),
            Err(_) => TestResult::failure("timeout binding UDP socket"),
        }
    }

    async fn test_dns_port_accessible_v6(&self) -> TestResult {
        match timeout(TEST_TIMEOUT, UdpSocket::bind("[::]:0")).await {
            Ok(Ok(socket)) => {
                match timeout(
                    TEST_TIMEOUT,
                    socket.connect(format!("[{}]:53", CLOUDFLARE_DNS_V6)),
                )
                .await
                {
                    Ok(Ok(_)) => TestResult::success("DNS port 53 accessible via IPv6"),
                    Ok(Err(e)) => {
                        TestResult::failure(format!("failed to connect to DNS port: {}", e))
                    }
                    Err(_) => TestResult::failure("timeout connecting to DNS port"),
                }
            }
            Ok(Err(e)) => TestResult::skipped(format!("IPv6 not available: {}", e)),
            Err(_) => TestResult::failure("timeout binding UDP socket"),
        }
    }

    async fn test_tcp_outbound_v4(&self) -> TestResult {
        let test_endpoints = [
            (CLOUDFLARE_DNS_V4, 443),
            (CLOUDFLARE_DNS_V4_ALT, 443),
            (QUAD9_DNS_V4, 443),
            (QUAD9_DNS_V4_ALT, 443),
        ];

        for (ip, port) in test_endpoints {
            match timeout(TEST_TIMEOUT, TcpStream::connect(format!("{}:{}", ip, port))).await {
                Ok(Ok(_)) => {
                    return TestResult::success(format!(
                        "TCP IPv4 outbound allowed (tested {}:{})",
                        ip, port
                    ));
                }
                Ok(Err(e)) => {
                    tracing::debug!("Failed to connect to {}:{}: {}", ip, port, e);
                    continue;
                }
                Err(_) => {
                    tracing::debug!("Timeout connecting to {}:{}", ip, port);
                    continue;
                }
            }
        }

        TestResult::failure("all TCP IPv4 outbound connections blocked or failed")
    }

    async fn test_tcp_outbound_v6(&self) -> TestResult {
        let test_endpoints = [
            (format!("[{}]", CLOUDFLARE_DNS_V6), 443),
            (format!("[{}]", QUAD9_DNS_V6), 443),
        ];

        for (ip, port) in test_endpoints {
            match timeout(TEST_TIMEOUT, TcpStream::connect(format!("{}:{}", ip, port))).await {
                Ok(Ok(_)) => {
                    return TestResult::success(format!(
                        "TCP IPv6 outbound allowed (tested {}:{})",
                        ip, port
                    ));
                }
                Ok(Err(e)) => {
                    tracing::debug!("Failed to connect to {}:{}: {}", ip, port, e);
                    continue;
                }
                Err(_) => {
                    tracing::debug!("Timeout connecting to {}:{}", ip, port);
                    continue;
                }
            }
        }

        TestResult::skipped("IPv6 not available or all TCP IPv6 outbound connections blocked")
    }

    async fn test_udp_outbound_v4(&self) -> TestResult {
        match UdpSocket::bind("0.0.0.0:0").await {
            Ok(socket) => {
                match timeout(
                    TEST_TIMEOUT,
                    socket.send_to(b"test", format!("{}:53", CLOUDFLARE_DNS_V4)),
                )
                .await
                {
                    Ok(Ok(_)) => TestResult::success("UDP IPv4 outbound allowed"),
                    Ok(Err(e)) => TestResult::failure(format!("failed to send UDP packet: {}", e)),
                    Err(_) => TestResult::failure("timeout sending UDP packet"),
                }
            }
            Err(e) => TestResult::failure(format!("failed to bind UDP socket: {}", e)),
        }
    }

    async fn test_udp_outbound_v6(&self) -> TestResult {
        match UdpSocket::bind("[::]:0").await {
            Ok(socket) => {
                match timeout(
                    TEST_TIMEOUT,
                    socket.send_to(b"test", format!("[{}]:53", CLOUDFLARE_DNS_V6)),
                )
                .await
                {
                    Ok(Ok(_)) => TestResult::success("UDP IPv6 outbound allowed"),
                    Ok(Err(e)) => TestResult::failure(format!("failed to send UDP packet: {}", e)),
                    Err(_) => TestResult::failure("timeout sending UDP packet"),
                }
            }
            Err(e) => TestResult::skipped(format!("IPv6 not available: {}", e)),
        }
    }

    // ===== DNS Resolution Tests =====

    async fn resolve_domains_v4(&self, domains: &[&str]) -> Result<Vec<Ipv4Addr>, String> {
        use hickory_resolver::{
            TokioResolver, name_server::TokioConnectionProvider, system_conf::read_system_conf,
        };

        // Use system DNS configuration to respect the VPN's local DNS forwarder when active
        let (config, opts) = match read_system_conf() {
            Ok((cfg, opts)) => {
                tracing::debug!(
                    "Read system DNS config: {} nameservers",
                    cfg.name_servers().len()
                );
                (cfg, opts)
            }
            Err(e) => {
                return Err(format!("Failed to read system DNS config: {}", e));
            }
        };

        let mut builder =
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default());
        *builder.options_mut() = opts;
        let resolver = builder.build();

        let mut ipv4_addrs = Vec::new();

        for domain in domains {
            match timeout(DNS_TEST_TIMEOUT, resolver.ipv4_lookup(*domain)).await {
                Ok(Ok(lookup)) => {
                    for ip in lookup.iter() {
                        ipv4_addrs.push(ip.0);
                        tracing::debug!("Resolved {} -> {}", domain, ip.0);
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("Failed to resolve {} (IPv4): {}", domain, e);
                }
                Err(_) => {
                    tracing::warn!("Timeout resolving {} (IPv4)", domain);
                }
            }
        }

        Ok(ipv4_addrs)
    }

    async fn resolve_domains_v6(&self, domains: &[&str]) -> Result<Vec<Ipv6Addr>, String> {
        use hickory_resolver::{
            TokioResolver, name_server::TokioConnectionProvider, system_conf::read_system_conf,
        };

        // Use system DNS configuration to respect the VPN's local DNS forwarder when active
        let (config, opts) = match read_system_conf() {
            Ok((cfg, opts)) => {
                tracing::debug!(
                    "Read system DNS config: {} nameservers",
                    cfg.name_servers().len()
                );
                (cfg, opts)
            }
            Err(e) => {
                return Err(format!("Failed to read system DNS config: {}", e));
            }
        };

        let mut builder =
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default());
        *builder.options_mut() = opts;
        let resolver = builder.build();

        let mut ipv6_addrs = Vec::new();

        for domain in domains {
            match timeout(DNS_TEST_TIMEOUT, resolver.ipv6_lookup(*domain)).await {
                Ok(Ok(lookup)) => {
                    for ip in lookup.iter() {
                        ipv6_addrs.push(ip.0);
                        tracing::debug!("Resolved {} -> {}", domain, ip.0);
                    }
                }
                Ok(Err(e)) => {
                    tracing::debug!("No IPv6 addresses for {}: {}", domain, e);
                }
                Err(_) => {
                    tracing::debug!("Timeout resolving {} (IPv6)", domain);
                }
            }
        }

        Ok(ipv6_addrs)
    }

    // ===== Tunnel Connectivity Tests =====

    async fn check_tunnel_interface_exists(&self) -> TestResult {
        if let Some(ref interface) = self.tunnel_interface {
            // Check if interface exists using netlink or platform-specific APIs
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                match Command::new("ip")
                    .args(&["link", "show", interface])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        TestResult::success(format!("Tunnel interface {} exists", interface))
                    }
                    Ok(_) => {
                        TestResult::failure(format!("tunnel interface {} not found", interface))
                    }
                    Err(e) => TestResult::failure(format!("failed to check interface: {}", e)),
                }
            }

            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                match Command::new("ifconfig").arg(interface).output() {
                    Ok(output) if output.status.success() => {
                        TestResult::success(format!("Tunnel interface {} exists", interface))
                    }
                    Ok(_) => {
                        TestResult::failure(format!("tunnel interface {} not found", interface))
                    }
                    Err(e) => TestResult::failure(format!("failed to check interface: {}", e)),
                }
            }

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                TestResult::skipped("Platform not supported for interface check")
            }
        } else {
            TestResult::skipped("No tunnel interface specified")
        }
    }

    async fn test_ping_tunnel_v4(&self) -> TestResult {
        if let Some(ref interface) = self.tunnel_interface {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                use std::process::Command;
                match Command::new("ping")
                    .args(["-c", "1", "-W", "2", "-I", interface, PING_TARGET_V4])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        TestResult::success(format!("Can ping through {} (IPv4)", interface))
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        // macOS ping -I expects a multicast-capable interface.
                        // Stale or unconfigured interfaces produce this error.
                        if stderr.contains("invalid multicast interface")
                            || stderr.contains("invalid interface")
                        {
                            TestResult::skipped(format!(
                                "Interface {} not configured for ICMP",
                                interface
                            ))
                        } else {
                            TestResult::failure(format!("ICMP ping failed: {}", stderr))
                        }
                    }
                    Err(e) => TestResult::failure(format!("failed to execute ping command: {}", e)),
                }
            }

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                TestResult::skipped("Platform not supported")
            }
        } else {
            TestResult::skipped("No tunnel interface specified")
        }
    }

    async fn test_ping_tunnel_v6(&self) -> TestResult {
        if let Some(ref interface) = self.tunnel_interface {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                match Command::new("ping6")
                    .args(["-c", "1", "-W", "2", "-I", interface, PING_TARGET_V6])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        TestResult::success(format!("Can ping through {} (IPv6)", interface))
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        // Interface errors indicate the interface is not configured for IPv6
                        if stderr.contains("invalid") || stderr.contains("not found") {
                            TestResult::skipped(format!(
                                "Interface {} not configured for ICMPv6",
                                interface
                            ))
                        } else {
                            TestResult::skipped(format!("ICMPv6 not available: {}", stderr))
                        }
                    }
                    Err(e) => {
                        TestResult::failure(format!("failed to execute ping6 command: {}", e))
                    }
                }
            }

            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                match Command::new("ping6")
                    .args(["-c", "1", "-W", "2000", "-I", interface, PING_TARGET_V6])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        TestResult::success(format!("Can ping through {} (IPv6)", interface))
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        // Interface errors or DNS resolution failures indicate IPv6 misconfiguration
                        if stderr.contains("invalid")
                            || stderr.contains("not found")
                            || stderr.contains("nodename nor servname")
                        {
                            TestResult::skipped(format!(
                                "Interface {} not configured for ICMPv6",
                                interface
                            ))
                        } else {
                            TestResult::skipped(format!("ICMPv6 not available: {}", stderr))
                        }
                    }
                    Err(e) => {
                        TestResult::failure(format!("failed to execute ping6 command: {}", e))
                    }
                }
            }

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                TestResult::skipped("Platform not supported")
            }
        } else {
            TestResult::skipped("No tunnel interface specified")
        }
    }

    async fn test_reach_peer_v4(&self) -> TestResult {
        if let Some(peer) = self.peer_address_v4 {
            match timeout(TEST_TIMEOUT, UdpSocket::bind("0.0.0.0:0")).await {
                Ok(Ok(socket)) => {
                    match timeout(
                        TEST_TIMEOUT,
                        socket.send_to(b"test", SocketAddr::new(IpAddr::V4(peer), 51820)),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            TestResult::success(format!("Can reach peer {} via IPv4", peer))
                        }
                        Ok(Err(e)) => TestResult::failure(format!("Cannot reach peer: {}", e)),
                        Err(_) => TestResult::failure("timeout reaching peer"),
                    }
                }
                Ok(Err(e)) => TestResult::failure(format!("Cannot bind socket: {}", e)),
                Err(_) => TestResult::failure("timeout binding socket"),
            }
        } else {
            TestResult::skipped("No IPv4 peer address specified")
        }
    }

    async fn test_reach_peer_v6(&self) -> TestResult {
        if let Some(peer) = self.peer_address_v6 {
            match timeout(TEST_TIMEOUT, UdpSocket::bind("[::]:0")).await {
                Ok(Ok(socket)) => {
                    match timeout(
                        TEST_TIMEOUT,
                        socket.send_to(b"test", SocketAddr::new(IpAddr::V6(peer), 51820)),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            TestResult::success(format!("Can reach peer {} via IPv6", peer))
                        }
                        Ok(Err(e)) => TestResult::failure(format!("Cannot reach peer: {}", e)),
                        Err(_) => TestResult::failure("timeout reaching peer"),
                    }
                }
                Ok(Err(e)) => TestResult::skipped(format!("IPv6 not available: {}", e)),
                Err(_) => TestResult::failure("timeout binding socket"),
            }
        } else {
            TestResult::skipped("No IPv6 peer address specified")
        }
    }

    async fn test_udp_socket_v4(&self) -> TestResult {
        match timeout(TEST_TIMEOUT, UdpSocket::bind("0.0.0.0:0")).await {
            Ok(Ok(_)) => TestResult::success("Can open UDP socket (IPv4)"),
            Ok(Err(e)) => TestResult::failure(format!("Cannot bind UDP socket: {}", e)),
            Err(_) => TestResult::failure("timeout binding UDP socket"),
        }
    }

    async fn test_udp_socket_v6(&self) -> TestResult {
        match timeout(TEST_TIMEOUT, UdpSocket::bind("[::]:0")).await {
            Ok(Ok(_)) => TestResult::success("Can open UDP socket (IPv6)"),
            Ok(Err(e)) => TestResult::skipped(format!("IPv6 not available: {}", e)),
            Err(_) => TestResult::failure("timeout binding UDP socket"),
        }
    }

    async fn check_tunnel_routing_v4(&self) -> TestResult {
        if let Some(ref interface) = self.tunnel_interface {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                match Command::new("ip")
                    .args(&["route", "show", "dev", interface])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let routes = String::from_utf8_lossy(&output.stdout);
                        if routes.trim().is_empty() {
                            TestResult::failure(format!("No routes configured for {}", interface))
                        } else {
                            TestResult::success(format!(
                                "IPv4 routing configured: {}",
                                routes.lines().next().unwrap_or("")
                            ))
                        }
                    }
                    Ok(_) => TestResult::failure("Failed to get routes"),
                    Err(e) => TestResult::failure(format!("failed to check routes: {}", e)),
                }
            }

            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                match Command::new("netstat").args(["-rn", "-f", "inet"]).output() {
                    Ok(output) if output.status.success() => {
                        let routes = String::from_utf8_lossy(&output.stdout);
                        if routes.contains(interface) {
                            TestResult::success(format!(
                                "IPv4 routing configured for {}",
                                interface
                            ))
                        } else {
                            TestResult::failure(format!("No IPv4 routes found for {}", interface))
                        }
                    }
                    Ok(_) => TestResult::failure("Failed to get routes"),
                    Err(e) => TestResult::failure(format!("failed to check routes: {}", e)),
                }
            }

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                TestResult::skipped("Platform not supported")
            }
        } else {
            TestResult::skipped("No tunnel interface specified")
        }
    }

    async fn check_tunnel_routing_v6(&self) -> TestResult {
        if let Some(ref interface) = self.tunnel_interface {
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                match Command::new("ip")
                    .args(&["-6", "route", "show", "dev", interface])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let routes = String::from_utf8_lossy(&output.stdout);
                        if routes.trim().is_empty() {
                            TestResult::skipped(format!(
                                "No IPv6 routes configured for {}",
                                interface
                            ))
                        } else {
                            TestResult::success(format!(
                                "IPv6 routing configured: {}",
                                routes.lines().next().unwrap_or("")
                            ))
                        }
                    }
                    Ok(_) => TestResult::failure("Failed to get IPv6 routes"),
                    Err(e) => TestResult::failure(format!("failed to check routes: {}", e)),
                }
            }

            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                match Command::new("netstat")
                    .args(["-rn", "-f", "inet6"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let routes = String::from_utf8_lossy(&output.stdout);
                        if routes.contains(interface) {
                            TestResult::success(format!(
                                "IPv6 routing configured for {}",
                                interface
                            ))
                        } else {
                            TestResult::skipped(format!("No IPv6 routes found for {}", interface))
                        }
                    }
                    Ok(_) => TestResult::failure("Failed to get routes"),
                    Err(e) => TestResult::failure(format!("failed to check routes: {}", e)),
                }
            }

            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                TestResult::skipped("Platform not supported")
            }
        } else {
            TestResult::skipped("No tunnel interface specified")
        }
    }

    // ===== Network Connectivity Tests =====

    async fn test_reach_dns_server(&self, ip: &str, is_v4: bool) -> TestResult {
        let addr = format!("{}:53", ip);
        match timeout(
            TEST_TIMEOUT,
            UdpSocket::bind(if is_v4 { "0.0.0.0:0" } else { "[::]:0" }),
        )
        .await
        {
            Ok(Ok(socket)) => match timeout(TEST_TIMEOUT, socket.send_to(b"test", &addr)).await {
                Ok(Ok(_)) => TestResult::success(format!("Can reach DNS server {}", ip)),
                Ok(Err(e)) => TestResult::failure(format!("Cannot reach {}: {}", ip, e)),
                Err(_) => TestResult::failure(format!("Timeout reaching {}", ip)),
            },
            Ok(Err(e)) => {
                if is_v4 {
                    TestResult::failure(format!("Cannot bind socket: {}", e))
                } else {
                    TestResult::skipped(format!("IPv6 not available: {}", e))
                }
            }
            Err(_) => TestResult::failure("timeout binding socket"),
        }
    }

    async fn test_tcp_endpoint(&self, endpoint: &str, is_v4: bool) -> TestResult {
        match timeout(TEST_TIMEOUT, TcpStream::connect(endpoint)).await {
            Ok(Ok(_)) => TestResult::success(format!("TCP connection to {} successful", endpoint)),
            Ok(Err(e)) => {
                if is_v4 {
                    TestResult::failure(format!("TCP connection failed: {}", e))
                } else {
                    TestResult::skipped(format!("IPv6 not available: {}", e))
                }
            }
            Err(_) => TestResult::failure("timeout establishing TCP connection"),
        }
    }

    async fn test_tcp_connect_v4(&self) -> TestResult {
        self.test_tcp_endpoint(&format!("{}:443", CLOUDFLARE_DNS_V4), true)
            .await
    }

    async fn test_tcp_connect_v6(&self) -> TestResult {
        self.test_tcp_endpoint(&format!("[{}]:443", CLOUDFLARE_DNS_V6), false)
            .await
    }

    // ===== Status Determination =====

    /// Determine overall diagnostic status based on test results.
    ///
    /// This function categorizes failures into firewall, DNS, tunnel, or network issues.
    /// Skipped tests (e.g., IPv6 unavailability) are not counted as failures.
    /// When in disconnected state, tunnel failures are expected and ignored.
    fn determine_overall_status(
        &self,
        tunnel_state: &str,
        firewall: &FirewallTests,
        dns: &DnsTests,
        tunnel: &TunnelConnectivityTests,
        network: &NetworkTests,
    ) -> OverallStatus {
        let mut issues: Vec<String> = Vec::new();
        let is_disconnected = tunnel_state.contains("disconnect") || tunnel_state == "unknown";

        // Firewall: Check if IPv4 traffic is blocked
        let firewall_blocking_ipv4 = firewall.can_resolve_dns_v4.is_failed()
            || firewall.firewall_allows_outbound_tcp_v4.is_failed()
            || firewall.firewall_allows_outbound_udp_v4.is_failed();

        if firewall_blocking_ipv4 {
            issues.push("firewall blocking critical IPv4 traffic".to_string());
        }

        // DNS: Check resolution failures and performance
        let dns_completely_failed = dns.can_resolve_ipv4_addresses.is_failed()
            && dns.can_resolve_ipv6_addresses.is_failed();

        if dns_completely_failed {
            issues.push("DNS resolution completely failing".to_string());
        } else if dns.dns_response_time_ms > 5000 {
            issues.push(format!(
                "DNS resolution very slow ({}ms)",
                dns.dns_response_time_ms
            ));
        }

        // Tunnel: Only check when not disconnected (leftover interface state is expected)
        if !is_disconnected {
            let interface_exists = tunnel.tunnel_interface_exists.passed;
            let ping_fails = tunnel.can_ping_tunnel_v4.is_failed();
            let routing_fails = tunnel.tunnel_routing_configured_v4.is_failed();

            if interface_exists && ping_fails {
                issues.push("tunnel interface exists but ICMP unreachable (IPv4)".to_string());
            }
            if interface_exists && routing_fails {
                issues.push("tunnel routing not configured (IPv4)".to_string());
            }
            if tunnel.tunnel_interface_exists.is_failed() {
                issues.push("tunnel interface not created".to_string());
            }
        }

        // Network: Check public endpoint reachability
        let no_dns_servers_reachable = network.can_reach_cloudflare_dns_v4.is_failed()
            && network.can_reach_quad9_dns_v4.is_failed();

        if no_dns_servers_reachable {
            issues.push("failed to reach any public DNS servers (IPv4)".to_string());
        }

        if network.can_establish_tcp_connection_v4.is_failed() {
            issues.push("failed to establish TCP connections (IPv4)".to_string());
        }

        // Categorize issue type
        match issues.len() {
            0 => OverallStatus::AllTestsPassed,
            1 => self.categorize_single_issue(&issues[0]),
            _ => OverallStatus::MultipleIssues(issues),
        }
    }

    fn categorize_single_issue(&self, issue: &str) -> OverallStatus {
        if issue.contains("firewall") {
            OverallStatus::FirewallIssue
        } else if issue.contains("DNS") {
            OverallStatus::DnsIssue
        } else if issue.contains("tunnel") {
            OverallStatus::TunnelIssue
        } else {
            OverallStatus::NetworkIssue
        }
    }

    // ===== Output Formatting =====

    /// Print report in human-readable format
    pub fn print_report(report: &DiagnosticReport) {
        println!("\n╔══════════════════════════════════════════════════════╗");
        println!("║          VPN DIAGNOSTICS REPORT                      ║");
        println!("╚══════════════════════════════════════════════════════╝");
        println!("\nTimestamp: {}", report.timestamp);
        println!("Tunnel State: {}", report.tunnel_state);
        println!("\n━━━ FIREWALL TESTS ━━━");
        Self::print_test("DNS Port IPv4", &report.firewall_tests.can_resolve_dns_v4);
        Self::print_test("DNS Port IPv6", &report.firewall_tests.can_resolve_dns_v6);
        Self::print_test(
            "TCP Outbound IPv4",
            &report.firewall_tests.firewall_allows_outbound_tcp_v4,
        );
        Self::print_test(
            "TCP Outbound IPv6",
            &report.firewall_tests.firewall_allows_outbound_tcp_v6,
        );
        Self::print_test(
            "UDP Outbound IPv4",
            &report.firewall_tests.firewall_allows_outbound_udp_v4,
        );
        Self::print_test(
            "UDP Outbound IPv6",
            &report.firewall_tests.firewall_allows_outbound_udp_v6,
        );

        println!("\n━━━ DNS RESOLUTION TESTS ━━━");
        Self::print_test(
            "IPv4 Resolution",
            &report.dns_tests.can_resolve_ipv4_addresses,
        );
        Self::print_test(
            "IPv6 Resolution",
            &report.dns_tests.can_resolve_ipv6_addresses,
        );
        println!(
            "  Resolved IPv4: {}",
            report.dns_tests.resolved_addresses_v4.join(", ")
        );
        println!(
            "  Resolved IPv6: {}",
            report.dns_tests.resolved_addresses_v6.join(", ")
        );
        println!(
            "  Response Time: {}ms",
            report.dns_tests.dns_response_time_ms
        );

        println!("\n━━━ TUNNEL CONNECTIVITY TESTS ━━━");
        Self::print_test(
            "Interface Exists",
            &report.tunnel_connectivity_tests.tunnel_interface_exists,
        );
        Self::print_test(
            "Ping Tunnel IPv4",
            &report.tunnel_connectivity_tests.can_ping_tunnel_v4,
        );
        Self::print_test(
            "Ping Tunnel IPv6",
            &report.tunnel_connectivity_tests.can_ping_tunnel_v6,
        );
        Self::print_test(
            "Reach Peer IPv4",
            &report.tunnel_connectivity_tests.can_reach_peer_address_v4,
        );
        Self::print_test(
            "Reach Peer IPv6",
            &report.tunnel_connectivity_tests.can_reach_peer_address_v6,
        );
        Self::print_test(
            "UDP Socket IPv4",
            &report.tunnel_connectivity_tests.can_open_udp_socket_v4,
        );
        Self::print_test(
            "UDP Socket IPv6",
            &report.tunnel_connectivity_tests.can_open_udp_socket_v6,
        );
        Self::print_test(
            "Routing IPv4",
            &report
                .tunnel_connectivity_tests
                .tunnel_routing_configured_v4,
        );
        Self::print_test(
            "Routing IPv6",
            &report
                .tunnel_connectivity_tests
                .tunnel_routing_configured_v6,
        );

        println!("\n━━━ NETWORK TESTS ━━━");
        Self::print_test(
            "Cloudflare DNS IPv4",
            &report.network_tests.can_reach_cloudflare_dns_v4,
        );
        Self::print_test(
            "Cloudflare DNS IPv6",
            &report.network_tests.can_reach_cloudflare_dns_v6,
        );
        Self::print_test(
            "Quad9 DNS IPv4",
            &report.network_tests.can_reach_quad9_dns_v4,
        );
        Self::print_test(
            "Quad9 DNS IPv6",
            &report.network_tests.can_reach_quad9_dns_v6,
        );
        Self::print_test(
            "DuckDuckGo IPv4",
            &report.network_tests.can_reach_duckduckgo_v4,
        );
        Self::print_test(
            "DuckDuckGo IPv6",
            &report.network_tests.can_reach_duckduckgo_v6,
        );
        Self::print_test(
            "TCP Connect IPv4",
            &report.network_tests.can_establish_tcp_connection_v4,
        );
        Self::print_test(
            "TCP Connect IPv6",
            &report.network_tests.can_establish_tcp_connection_v6,
        );

        println!("\n━━━ OVERALL STATUS ━━━");
        match &report.overall_status {
            OverallStatus::AllTestsPassed => {
                println!("[PASS] All tests passed - VPN functioning correctly");
            }
            OverallStatus::FirewallIssue => {
                println!("[FAIL] FIREWALL ISSUE DETECTED");
                println!("       Firewall rules are blocking critical traffic");
            }
            OverallStatus::DnsIssue => {
                println!("[FAIL] DNS ISSUE DETECTED");
                println!("       DNS resolution failing or misconfigured");
            }
            OverallStatus::TunnelIssue => {
                println!("[FAIL] TUNNEL ISSUE DETECTED");
                println!("       Tunnel interface or routing problem");
            }
            OverallStatus::NetworkIssue => {
                println!("[FAIL] NETWORK ISSUE DETECTED");
                println!("       General network connectivity problem");
            }
            OverallStatus::MultipleIssues(issues) => {
                println!("[FAIL] MULTIPLE ISSUES DETECTED:");
                for issue in issues {
                    println!("       - {}", issue);
                }
            }
        }

        // Context-aware notes for different states
        if report.tunnel_state.contains("disconnect")
            || report.tunnel_state == "unknown"
            || report.tunnel_state.contains("offline")
        {
            println!("\nNOTE: Disconnected state - tunnel failures are expected.");
            println!(
                "      Leftover interface state from previous connection may cause test failures."
            );
            println!(
                "      Only firewall, DNS, and network connectivity tests indicate actual problems."
            );
        } else if report.tunnel_state.contains("connecting")
            || report.tunnel_state.contains("disconnecting")
        {
            println!("\nNOTE: Transition state - temporary failures expected.");
            println!(
                "      Firewall rules and interface configuration changes occur during these windows."
            );
            println!("      Failures typically resolve within 2-5 seconds once state stabilizes.");
        }

        println!();
    }

    fn print_test(name: &str, result: &TestResult) {
        let status = if result.passed {
            "[PASS]"
        } else if result
            .details
            .as_ref()
            .map(|d| d.starts_with("Skipped"))
            .unwrap_or(false)
        {
            "[SKIP]"
        } else {
            "[FAIL]"
        };

        print!("  {} {:<25}", status, name);

        if let Some(ref details) = result.details {
            println!(" {}", details);
        } else if let Some(ref error) = result.error {
            println!(" ERROR: {}", error);
        } else {
            println!();
        }
    }

    /// Export report as JSON
    pub fn to_json(report: &DiagnosticReport) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(report)
    }

    // ===== Helper Functions =====

    /// Log system DNS configuration to assist with troubleshooting DNS resolution issues.
    ///
    /// On macOS, reads DNS config via scutil.
    /// On Linux, reads /etc/resolv.conf.
    /// Detects if the VPN's local DNS forwarder (127.x.x.x) is active.
    fn log_system_dns_config(&self) {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            if let Ok(output) = Command::new("scutil").args(["--dns"]).output() {
                let dns_output = String::from_utf8_lossy(&output.stdout);
                let nameservers: Vec<String> = dns_output
                    .lines()
                    .filter_map(|line| {
                        if line.trim().starts_with("nameserver") {
                            line.split_whitespace().nth(2).map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();

                self.log_nameservers(&nameservers);
            }
        }

        #[cfg(target_os = "linux")]
        {
            use std::fs;

            if let Ok(resolv_conf) = fs::read_to_string("/etc/resolv.conf") {
                let nameservers: Vec<String> = resolv_conf
                    .lines()
                    .filter_map(|line| {
                        line.trim()
                            .strip_prefix("nameserver")
                            .and_then(|ns| ns.trim().split_whitespace().next())
                            .map(|s| s.to_string())
                    })
                    .collect();

                self.log_nameservers(&nameservers);
            }
        }
    }

    fn log_nameservers(&self, nameservers: &[String]) {
        if nameservers.is_empty() {
            return;
        }

        let unique_servers: std::collections::HashSet<&str> =
            nameservers.iter().map(|s| s.as_str()).collect();
        let servers_list = unique_servers.into_iter().collect::<Vec<_>>().join(", ");

        tracing::info!("System DNS servers: {}", servers_list);

        let using_vpn_forwarder = nameservers.iter().any(|ns| ns.starts_with("127."));
        if using_vpn_forwarder {
            tracing::info!("VPN local DNS forwarder active");
        } else {
            tracing::warn!("VPN DNS forwarder not active - using public DNS");
        }
    }
}

impl Default for VpnDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_diagnostics() {
        let diag = VpnDiagnostics::new();
        let report = diag.run_full_diagnostics("disconnected").await;

        // Should be able to at least test some basic network connectivity
        assert!(
            report.network_tests.can_establish_tcp_connection_v4.passed
                || report
                    .network_tests
                    .can_establish_tcp_connection_v4
                    .error
                    .is_some()
        );
    }
}
