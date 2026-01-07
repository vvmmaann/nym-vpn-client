// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use nym_validator_client::nym_api::NymApiClientExt;
use nym_vpn_api_client::{
    api_urls_to_urls, fronted_http_client, response::NymVpnHealthResponse, types::VpnApiTime,
};
use nym_vpn_lib::new_user_agent;
use nym_vpn_network_config::Network;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::diagnostic::helpers::DiagnosticResult;

pub struct HttpDiagnostic;

impl HttpDiagnostic {
    pub async fn run_diagnostic(network: &Network) -> anyhow::Result<HttpReport> {
        tracing::info!("Running http diagnostic");
        let nym_vpn_api_client = nym_vpn_api_client::VpnApiClient::from_network(
            network.nym_network_details(),
            new_user_agent!(),
            None,
        )
        .await?;

        let nym_urls = api_urls_to_urls(
            &network
                .nym_api_urls()
                .ok_or(anyhow::anyhow!("No API URLs in the given network"))?,
        )?;
        let api_client = fronted_http_client(nym_urls, None, None, None).await?;

        // Setup is done, we return a report from now on

        let health_response = nym_vpn_api_client.get_health().await;
        let remote_time = nym_vpn_api_client
            .get_remote_time()
            .await
            .map(ApiTimeSkew::from);

        let nb_nodes = api_client
            .get_all_described_nodes()
            .await
            .map(|list| list.len());

        Ok(HttpReport {
            remote_time: remote_time.into(),
            health_response: health_response.into(),
            nb_nymnodes: nb_nodes.into(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpReport {
    remote_time: DiagnosticResult<ApiTimeSkew>,
    health_response: DiagnosticResult<NymVpnHealthResponse>,
    nb_nymnodes: DiagnosticResult<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTimeSkew {
    // The local time on the client.
    pub local_time: OffsetDateTime,

    // The estimated time on the remote server. Based on RTT, it's not guaranteed to be accurate.
    pub estimated_remote_time: OffsetDateTime,

    pub accetably_synced: bool,
}

impl From<VpnApiTime> for ApiTimeSkew {
    fn from(value: VpnApiTime) -> Self {
        Self {
            local_time: value.local_time,
            estimated_remote_time: value.estimated_remote_time,
            accetably_synced: value.is_acceptable_synced(),
        }
    }
}
