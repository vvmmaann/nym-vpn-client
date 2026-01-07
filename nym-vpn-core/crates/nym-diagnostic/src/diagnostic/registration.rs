// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use nym_authenticator_client::{AuthClientMixnetListener, AuthenticatorClient, RegistrationError};
use nym_bandwidth_controller::{BandwidthController, BandwidthTicketProvider};
use nym_client_core::client::topology_control::nym_api_provider::Config;
use nym_credentials_interface::TicketType;
use nym_registration_common::GatewayData;
use nym_sdk::mixnet::{DisconnectedMixnetClient, Ephemeral, MixnetClient, x25519};
use nym_sdk::{DebugConfig, NymApiTopologyProvider, mixnet::MixnetClientBuilder};
use nym_sdk::{NymNetworkDetails, TopologyProvider};
use nym_topology::HardcodedTopologyProvider;
use nym_validator_client::client::NymApiClientExt;
use nym_validator_client::nyxd::{Config as NyxdClientConfig, NyxdClient};
use nym_vpn_api_client::{api_urls_to_urls, fronted_http_client};
use nym_vpn_lib::{Recipient, StoragePaths, new_user_agent};
use nym_vpn_network_config::Network;

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::diagnostic::helpers::DiagnosticResult;

struct WgRegistrationConfig {
    gateway_auth_address: Recipient,
    gateway_version: String,
    gateway_keypair: Arc<x25519::KeyPair>,
    gateway_ip: IpAddr,
    bandwidth_provider: Box<dyn BandwidthTicketProvider>,
}

pub struct RegistrationDiagnostic;

impl RegistrationDiagnostic {
    pub async fn run_diagnostic(
        network: &Network,
        gateway_id: &str,
        storage_path: Option<&PathBuf>,
    ) -> anyhow::Result<RegistrationReport> {
        tracing::info!("Registering diagnostic on gateway {}", gateway_id);
        let topology_provider = setup_topology(network).await?;

        let mut registration_report = RegistrationReport {
            mixnet_client_build: DiagnosticResult::from_value(()),
            mixnet_client_start: None,
            wireguard_registration: None,
        };
        tracing::info!("Starting mixnet client");

        let disconnected_mixnet_client = match Self::build_mixnet_client(
            network.nym_network_details().clone(),
            gateway_id,
            Box::new(topology_provider),
        ) {
            Ok(client) => {
                registration_report.mixnet_client_build = DiagnosticResult::<()>::SUCCESS;
                client
            }
            Err(e) => {
                registration_report.mixnet_client_build = DiagnosticResult::from_err(e);
                return Ok(registration_report);
            }
        };

        let mixnet_client = match Box::pin(tokio::time::timeout(
            Duration::from_secs(10),
            disconnected_mixnet_client.connect_to_mixnet(),
        ))
        .await
        {
            Ok(Ok(client)) => {
                registration_report.mixnet_client_start = Some(DiagnosticResult::<()>::SUCCESS);
                client
            }
            Ok(Err(e)) => {
                registration_report.mixnet_client_start = Some(DiagnosticResult::from_err(e));
                return Ok(registration_report);
            }
            Err(e) => {
                registration_report.mixnet_client_start = Some(DiagnosticResult::from_err(e));
                return Ok(registration_report);
            }
        };

        tracing::info!("Mixnet client started");

        let registration_config = match setup_registration(network, gateway_id, storage_path).await
        {
            Ok(config) => config,
            Err(e) => {
                registration_report.wireguard_registration = Some(DiagnosticResult::from_err(
                    format!("Registration not possible: {}", e),
                ));
                mixnet_client.disconnect().await;
                return Ok(registration_report);
            }
        };

        tracing::info!("Registering...");

        match Self::wireguard_registration(mixnet_client, registration_config).await {
            Ok(response) => {
                registration_report.wireguard_registration =
                    Some(DiagnosticResult::from_value(response))
            }
            Err(e) => {
                registration_report.wireguard_registration = Some(DiagnosticResult::from_err(e));
            }
        };
        tracing::info!("Registration diagnostic complete");
        Ok(registration_report)
    }

    fn build_mixnet_client(
        network: NymNetworkDetails,
        gateway_id: &str,
        topology_provider: Box<dyn TopologyProvider + Send + Sync>,
    ) -> Result<DisconnectedMixnetClient<Ephemeral>, Box<nym_sdk::Error>> {
        let builder = MixnetClientBuilder::new_ephemeral()
            .with_user_agent(new_user_agent!())
            .request_gateway(gateway_id.into())
            .network_details(network)
            .debug_config(debug_config())
            .credentials_mode(false)
            .no_hostname(true)
            .custom_topology_provider(topology_provider);

        builder.build().map_err(Box::new)
    }

    async fn wireguard_registration(
        mixnet_client: MixnetClient,
        wg_registration_config: WgRegistrationConfig,
    ) -> Result<GatewayData, RegistrationError> {
        let address = *mixnet_client.nym_address();

        let mixnet_listener =
            AuthClientMixnetListener::new(mixnet_client, CancellationToken::new()).start();
        let mut auth_client = AuthenticatorClient::new(
            mixnet_listener.subscribe(),
            mixnet_listener.mixnet_sender(),
            address,
            wg_registration_config.gateway_auth_address,
            wg_registration_config.gateway_version.into(),
            wg_registration_config.gateway_keypair,
            wg_registration_config.gateway_ip,
        );

        // Embedded timeout
        let auth_res = auth_client
            .register_wireguard(
                &*wg_registration_config.bandwidth_provider,
                TicketType::V1WireguardEntry,
            )
            .await;

        // Stopping mixnet client
        mixnet_listener.stop().await;

        auth_res
    }
}

fn debug_config() -> DebugConfig {
    let mut debug_config = DebugConfig::default();

    debug_config.traffic.average_packet_delay = Duration::ZERO;
    debug_config.traffic.disable_mix_hops = true;
    debug_config
        .traffic
        .disable_main_poisson_packet_distribution = true;
    debug_config.cover_traffic.disable_loop_cover_traffic_stream = true;
    debug_config.topology.minimum_mixnode_performance = 0;
    debug_config.topology.minimum_gateway_performance = 0;
    debug_config
}

async fn setup_topology(network: &Network) -> anyhow::Result<HardcodedTopologyProvider> {
    let nym_urls = api_urls_to_urls(
        &network
            .nym_api_urls()
            .ok_or(anyhow::anyhow!("No API URLs in the given network"))?,
    )?;
    let api_client = fronted_http_client(nym_urls.clone(), None, None, None).await?;
    const DEFAULT_CONFIG: Config = Config {
        min_mixnode_performance: 0,
        min_gateway_performance: 0,
        use_extended_topology: true,
        ignore_egress_epoch_role: true,
    };

    let mut topology_provider = NymApiTopologyProvider::new(
        DEFAULT_CONFIG,
        nym_urls.into_iter().map(Into::into).collect(),
        api_client,
    );

    let topology = topology_provider
        .get_new_topology()
        .await
        .ok_or(anyhow::anyhow!("Failed to get topology"))?;

    Ok(HardcodedTopologyProvider::new(topology))
}

async fn setup_registration(
    network: &Network,
    gateway_id: &str,
    storage_path: Option<&PathBuf>,
) -> anyhow::Result<WgRegistrationConfig> {
    let storage_path = storage_path.ok_or(anyhow::anyhow!("No storage path provided"))?;

    let nym_urls = api_urls_to_urls(
        &network
            .nym_api_urls()
            .ok_or(anyhow::anyhow!("No API URLs in the given network"))?,
    )?;
    let api_client = fronted_http_client(nym_urls.clone(), None, None, None).await?;

    let described_nodes = api_client.get_all_described_nodes().await?;
    let gateway = described_nodes
        .iter()
        .find(|g| g.ed25519_identity_key().to_base58_string() == gateway_id)
        .ok_or(anyhow::anyhow!("Gateway not found"))?
        .clone();

    let mut rng = rand::rngs::OsRng;
    let gateway_keypair = Arc::new(x25519::KeyPair::new(&mut rng));

    let gateway_version = gateway.version().to_string();
    let authenticator_address = gateway
        .description
        .authenticator
        .and_then(|a| Recipient::try_from_base58_string(&a.address).ok())
        .ok_or(anyhow::anyhow!(
            "Failed to get authenticator address for chosen gateway"
        ))?;
    let gateway_ip = *gateway
        .description
        .host_information
        .ip_address
        .first()
        .ok_or(anyhow::anyhow!(
            "Chosen gateway does not have announced IP addresses"
        ))?;

    let bandwidth_provider = setup_bandwidth_provider(network, storage_path).await?;

    Ok(WgRegistrationConfig {
        gateway_auth_address: authenticator_address,
        gateway_version,
        gateway_keypair,
        gateway_ip,
        bandwidth_provider,
    })
}

async fn setup_bandwidth_provider(
    network: &Network,
    storage_path: &PathBuf,
) -> anyhow::Result<Box<dyn BandwidthTicketProvider>> {
    let config = NyxdClientConfig::try_from_nym_network_details(network.nym_network_details())?;
    let nyxd_url = network
        .nym_network_details()
        .endpoints
        .first()
        .map(|ep| ep.nyxd_url())
        .ok_or(anyhow::anyhow!("Invalid Nyxd URl"))?;

    let storage_paths = StoragePaths::new_from_dir(storage_path)?;
    if !storage_paths.credential_database_path.exists() {
        return Err(anyhow::anyhow!("Credential database doesn't exist"));
    }
    let credential_storage = storage_paths.persistent_credential_storage().await?;
    let nyxd_client = NyxdClient::connect(config, nyxd_url.as_str())?;

    Ok(Box::new(BandwidthController::new(
        credential_storage,
        nyxd_client,
    )))
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationReport {
    mixnet_client_build: DiagnosticResult<()>,

    #[serde(skip_serializing_if = "Option::is_none")]
    mixnet_client_start: Option<DiagnosticResult<()>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    wireguard_registration: Option<DiagnosticResult<GatewayData>>,
}
