// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

//! VPN Diagnostics CLI Tool
//!
//! Standalone utility to test VPN connectivity, firewall rules, and DNS resolution
//! at various stages of the connection lifecycle.

use clap::Parser;
use nym_vpn_diagnostics::{OverallStatus, VpnDiagnostics};
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Parser, Debug)]
#[command(name = "vpn-diagnostics")]
#[command(about = "VPN diagnostics tool to identify firewall, DNS, and tunnel connectivity")]
struct Args {
    /// Tunnel state to test (e.g., "connecting", "connected", "disconnected")
    #[arg(short, long, default_value = "unknown")]
    state: String,

    /// Tunnel interface name (e.g., "tun0", "utun4"). If not specified, will auto-detect
    #[arg(short = 'i', long)]
    interface: Option<String>,

    /// IPv4 peer address to test
    #[arg(long)]
    peer_v4: Option<Ipv4Addr>,

    /// IPv6 peer address to test
    #[arg(long)]
    peer_v6: Option<Ipv6Addr>,

    /// Output format: text or json
    #[arg(short = 'f', long, default_value = "text")]
    format: String,

    /// Continuous monitoring mode (run every N seconds)
    #[arg(short = 'c', long)]
    continuous: Option<u64>,

    /// Output file for JSON results
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// List detected VPN tunnel interfaces and exit
    #[arg(short = 'l', long)]
    list_interfaces: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    // Handle list-interfaces command
    if args.list_interfaces {
        list_vpn_interfaces();
        return Ok(());
    }

    // Auto-detect daemon state if not explicitly provided
    let state = if args.state == "unknown" {
        match VpnDiagnostics::get_daemon_state().await {
            Ok(daemon_state) => {
                tracing::info!("Detected daemon state: {}", daemon_state);
                daemon_state
            }
            Err(e) => {
                tracing::warn!("Could not query daemon state: {}. Using 'unknown'", e);
                args.state.clone()
            }
        }
    } else {
        args.state.clone()
    };

    // Build diagnostics runner
    let mut diag = VpnDiagnostics::new();

    // Auto-detect tunnel interface if not provided
    let interface = if let Some(iface) = args.interface {
        iface
    } else {
        match detect_vpn_tunnel_interface() {
            Some(iface) => {
                println!("Auto-detected tunnel interface: {}\n", iface);
                iface
            }
            None => {
                eprintln!("Warning: Could not auto-detect tunnel interface.");
                eprintln!("VPN may not be connected, or use -i to specify interface manually.");
                eprintln!("Run with --list-interfaces to see available interfaces.\n");
                String::new()
            }
        }
    };

    if !interface.is_empty() {
        diag = diag.with_tunnel_interface(interface);
    }

    if let Some(peer) = args.peer_v4 {
        diag = diag.with_peer_v4(peer);
    }

    if let Some(peer) = args.peer_v6 {
        diag = diag.with_peer_v6(peer);
    }

    // Continuous mode or single run
    if let Some(interval) = args.continuous {
        run_continuous(
            &diag,
            &state,
            interval,
            &args.format,
            args.output.as_deref(),
        )
        .await?;
    } else {
        run_single(&diag, &state, &args.format, args.output.as_deref()).await?;
    }

    Ok(())
}

async fn run_single(
    diag: &VpnDiagnostics,
    state: &str,
    format: &str,
    output_file: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = diag.run_full_diagnostics(state).await;

    match format {
        "json" => {
            let json = VpnDiagnostics::to_json(&report)?;
            if let Some(file) = output_file {
                std::fs::write(file, &json)?;
                println!("Report written to: {}", file);
            } else {
                println!("{}", json);
            }
        }
        _ => {
            VpnDiagnostics::print_report(&report);
            if let Some(file) = output_file {
                let json = VpnDiagnostics::to_json(&report)?;
                std::fs::write(file, &json)?;
                println!("\nJSON report also written to: {}", file);
            }
        }
    }

    // Exit with error code if tests failed
    match report.overall_status {
        OverallStatus::AllTestsPassed => {
            std::process::exit(0);
        }
        _ => {
            std::process::exit(1);
        }
    }
}

async fn run_continuous(
    diag: &VpnDiagnostics,
    initial_state: &str,
    interval_secs: u64,
    format: &str,
    output_file: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Running diagnostics every {} seconds (Ctrl+C to stop)...\n",
        interval_secs
    );

    let mut iteration = 0;
    loop {
        iteration += 1;
        println!("─── Iteration {} ───", iteration);

        // Query daemon state on each iteration to detect transitions
        let state = match VpnDiagnostics::get_daemon_state().await {
            Ok(daemon_state) => daemon_state,
            Err(_) => {
                tracing::warn!("Failed to query daemon state, using fallback");
                initial_state.to_string()
            }
        };

        // Skip diagnostics if daemon is not in a meaningful state
        if state == "unknown" || state == "disconnected" {
            println!(
                "Daemon state: {} - skipping diagnostics (waiting for connection)",
                state
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
            continue;
        }

        let report = diag.run_full_diagnostics(&state).await;

        match format {
            "json" => {
                let json = VpnDiagnostics::to_json(&report)?;
                println!("{}", json);

                if let Some(file) = output_file {
                    let timestamp_file = format!("{}.{}", file, iteration);
                    std::fs::write(&timestamp_file, &json)?;
                    println!("Report written to: {}", timestamp_file);
                }
            }
            _ => {
                VpnDiagnostics::print_report(&report);

                if let Some(file) = output_file {
                    let json = VpnDiagnostics::to_json(&report)?;
                    let timestamp_file = format!("{}.{}.json", file, iteration);
                    std::fs::write(&timestamp_file, &json)?;
                    println!("JSON report written to: {}", timestamp_file);
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
    }
}

/// Detect VPN tunnel interface automatically
fn detect_vpn_tunnel_interface() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // On macOS, VPN uses utun interfaces (utun4, utun5, etc)
        detect_utun_interface()
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, VPN uses tun interfaces (tun0, tun1, etc)
        detect_tun_interface()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn detect_utun_interface() -> Option<String> {
    use std::process::Command;

    // Run ifconfig and look for utun interfaces with active state
    let output = Command::new("ifconfig").output().ok()?;

    let ifconfig_output = String::from_utf8_lossy(&output.stdout);

    // Look for utun interfaces (typically utun4, utun5 for VPN)
    // Skip utun0-3 which are usually system interfaces
    for line in ifconfig_output.lines() {
        if line.starts_with("utun")
            && !line.starts_with("utun0")
            && !line.starts_with("utun1")
            && !line.starts_with("utun2")
            && !line.starts_with("utun3")
            && let Some(iface_name) = line.split(':').next()
        {
            // Prefer higher numbered utun interfaces (more likely to be VPN)
            return Some(iface_name.trim().to_string());
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn detect_tun_interface() -> Option<String> {
    use std::process::Command;

    // Run ip link show and look for tun interfaces
    let output = Command::new("ip").args(&["link", "show"]).output().ok()?;

    let ip_output = String::from_utf8_lossy(&output.stdout);

    // Look for tun0, tun1, etc
    for line in ip_output.lines() {
        if line.contains("tun") && line.contains("state UP") {
            // Extract interface name
            if let Some(parts) = line.split(':').nth(1) {
                return Some(parts.trim().split_whitespace().next()?.to_string());
            }
        }
    }

    None
}

/// List all detected VPN tunnel interfaces
fn list_vpn_interfaces() {
    println!("Scanning for VPN tunnel interfaces...\n");

    #[cfg(target_os = "macos")]
    {
        list_utun_interfaces();
    }

    #[cfg(target_os = "linux")]
    {
        list_tun_interfaces();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!("Platform not supported for auto-detection.");
    }
}

#[cfg(target_os = "macos")]
fn list_utun_interfaces() {
    use std::process::Command;

    let output = match Command::new("ifconfig").output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("Failed to run ifconfig: {}", e);
            return;
        }
    };

    let ifconfig_output = String::from_utf8_lossy(&output.stdout);
    let mut found_any = false;

    for line in ifconfig_output.lines() {
        if line.starts_with("utun")
            && let Some(iface_name) = line.split(':').next()
        {
            let iface = iface_name.trim();
            // Check if it's likely a VPN interface (skip utun0-3 which are system)
            let is_vpn_likely = !matches!(iface, "utun0" | "utun1" | "utun2" | "utun3");
            let marker = if is_vpn_likely {
                " (likely VPN)"
            } else {
                " (system)"
            };
            println!("  {} {}", iface, marker);
            found_any = true;
        }
    }

    if !found_any {
        println!("  No utun interfaces found. VPN may not be connected.");
    }
}

#[cfg(target_os = "linux")]
fn list_tun_interfaces() {
    use std::process::Command;

    let output = match Command::new("ip").args(&["link", "show"]).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("Failed to run ip command: {}", e);
            return;
        }
    };

    let ip_output = String::from_utf8_lossy(&output.stdout);
    let mut found_any = false;

    for line in ip_output.lines() {
        if line.contains("tun") {
            if let Some(parts) = line.split(':').nth(1) {
                if let Some(iface) = parts.trim().split_whitespace().next() {
                    let state = if line.contains("state UP") {
                        "UP"
                    } else {
                        "DOWN"
                    };
                    println!("  {} (state: {})", iface, state);
                    found_any = true;
                }
            }
        }
    }

    if !found_any {
        println!("  No tun interfaces found. VPN may not be connected.");
    }
}
