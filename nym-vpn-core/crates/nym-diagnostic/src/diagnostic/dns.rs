// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use nym_http_api_client::{HickoryDnsResolver, ResolveError};
use nym_vpn_network_config::Network;

use hickory_resolver::{
    Resolver, ResolverBuilder,
    config::{ResolverConfig, ResolverOpts},
    name_server::TokioConnectionProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    iter,
    net::IpAddr,
    time::{Duration, Instant},
};

use crate::diagnostic::helpers::DiagnosticResult;

pub struct DnsDiagnostic;

impl DnsDiagnostic {
    fn system() -> anyhow::Result<Resolver<TokioConnectionProvider>> {
        Ok(Self::build_resolver(Resolver::builder_tokio()?))
    }

    fn from_config(config: ResolverConfig) -> Resolver<TokioConnectionProvider> {
        Self::build_resolver(Resolver::builder_with_config(
            config,
            TokioConnectionProvider::default(),
        ))
    }

    fn build_resolver(
        base: ResolverBuilder<TokioConnectionProvider>,
    ) -> Resolver<TokioConnectionProvider> {
        let mut options = ResolverOpts::default();
        options.attempts = 0;
        options.cache_size = 0;
        options.ip_strategy = hickory_resolver::config::LookupIpStrategy::Ipv4AndIpv6;
        options.timeout = Duration::from_secs(2);
        base.with_options(options).build()
    }

    pub async fn run_diagnostic(network: &Network) -> anyhow::Result<CompleteDnsReport> {
        tracing::info!("Running DNS diagnostic");

        let hostnames = hostnames(network);

        tracing::debug!("Running DNS diagnostic on: {:?}", hostnames);
        tracing::debug!("System DNS diagnostic");
        let system_resolver = DnsDiagnostic::system()?;
        let system = DnsReport::new(&hostnames, &system_resolver).await;

        tracing::debug!("Quad9 DNS diagnostic");
        let quad9_resolver = DnsDiagnostic::from_config(ResolverConfig::quad9());
        let quad9 = DnsReport::new(&hostnames, &quad9_resolver).await;

        tracing::debug!("Quad9 DoH diagnostic");
        let quad9_doh_resolver = DnsDiagnostic::from_config(ResolverConfig::quad9());
        let quad9_doh = DnsReport::new(&hostnames, &quad9_doh_resolver).await;

        tracing::debug!("Quad9 DoT diagnostic");
        let quad9_dot_resolver = DnsDiagnostic::from_config(ResolverConfig::quad9());
        let quad9_dot = DnsReport::new(&hostnames, &quad9_dot_resolver).await;

        tracing::debug!("CloudFlare DNS diagnostic");
        let cloudflare_resolver = DnsDiagnostic::from_config(ResolverConfig::cloudflare());
        let cloudflare = DnsReport::new(&hostnames, &cloudflare_resolver).await;

        tracing::debug!("CloudFlare DoH diagnostic");
        let cloudflare_doh_resolver = DnsDiagnostic::from_config(ResolverConfig::cloudflare());
        let cloudflare_doh = DnsReport::new(&hostnames, &cloudflare_doh_resolver).await;

        tracing::debug!("CloudFlare DoT diagnostic");
        let cloudflare_dot_resolver = DnsDiagnostic::from_config(ResolverConfig::cloudflare());
        let cloudflare_dot = DnsReport::new(&hostnames, &cloudflare_dot_resolver).await;

        tracing::debug!("Nym custom DNS diagnostic");
        let mut nym_resolver = HickoryDnsResolver::default();
        nym_resolver.disable_system_fallback();
        nym_resolver.set_static_fallbacks(Default::default());
        let nym = DnsReport::new(&hostnames, &nym_resolver).await;

        Ok(CompleteDnsReport {
            system,
            quad9,
            quad9_doh,
            quad9_dot,
            cloudflare,
            cloudflare_doh,
            cloudflare_dot,
            nym,
        })
    }
}

pub fn hostnames(network: &Network) -> Vec<String> {
    let api_urls = network
        .nym_api_urls_as_urls()
        .into_iter()
        .chain(network.nym_vpn_api_urls_as_urls())
        .flatten()
        .chain(iter::once(network.nyxd_url.clone()));

    // Convert str urls to hostnames
    api_urls
        .filter_map(|url| match url.host_str() {
            Some(host) => Some(host.to_string()),
            None => {
                tracing::warn!("URL has no host component: {}", url);
                None
            }
        })
        .collect()
}

// QoL trait to accommodate both our custom resolver and hickory ones
#[async_trait::async_trait]
trait DnsResolver {
    async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>, ResolveError>;
}

#[async_trait::async_trait]
impl DnsResolver for HickoryDnsResolver {
    async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>, ResolveError> {
        Ok(self.resolve_str(hostname).await?.collect())
    }
}

#[async_trait::async_trait]
impl DnsResolver for Resolver<TokioConnectionProvider> {
    async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>, ResolveError> {
        Ok(self.lookup_ip(hostname).await?.iter().collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompleteDnsReport {
    system: DnsReport,
    quad9: DnsReport,
    quad9_doh: DnsReport,
    quad9_dot: DnsReport,
    cloudflare: DnsReport,
    cloudflare_doh: DnsReport,
    cloudflare_dot: DnsReport,
    nym: DnsReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DnsReport(Vec<DnsResolution>);

impl DnsReport {
    pub async fn new(hostnames: &[String], dns_resolver: &impl DnsResolver) -> Self {
        Self(
            futures::future::join_all(
                hostnames
                    .iter()
                    .map(|h| DnsResolution::new(h, dns_resolver)),
            )
            .await,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DnsResolution {
    hostname: String,
    resolution: DiagnosticResult<Vec<IpAddr>>,
    resolution_duration_ms: u128,
}

impl DnsResolution {
    pub async fn new(hostname: &str, dns_resolver: &impl DnsResolver) -> Self {
        let now = Instant::now();
        let resolution = dns_resolver.resolve(hostname).await;
        let resolution_duration_ms = now.elapsed().as_millis();

        Self {
            hostname: hostname.into(),
            resolution: resolution.into(),
            resolution_duration_ms,
        }
    }
}
