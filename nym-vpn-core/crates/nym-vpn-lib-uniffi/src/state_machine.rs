// Copyright 2024 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::sync::Arc;

use nym_statistics::StatisticsSender;
use nym_vpn_account_controller::{AccountCommandSender, AccountStateReceiver};
use nym_vpn_lib::{
    VpnTopologyProvider,
    tunnel_state_machine::{
        DnsOptions, GatewayPerformanceOptions, MixnetTunnelOptions, NymConfig, TunnelCommand,
        TunnelConstants, TunnelSettings, TunnelStateMachine, WireguardMultihopMode,
        WireguardTunnelOptions,
    },
};
use nym_vpn_lib_types::TunnelType;
use nym_vpn_network_config::Network;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::gateway_cache;

use super::{STATE_MACHINE_HANDLE, VPNConfig, error::VpnError};

pub(super) async fn init_state_machine(
    config: Box<VPNConfig>,
    network_env: Network,
    account_controller_tx: AccountCommandSender,
    account_controller_state: AccountStateReceiver,
    statistics_event_sender: StatisticsSender,
) -> Result<(), VpnError> {
    let mut guard = STATE_MACHINE_HANDLE.lock().await;

    if guard.is_none() {
        statistics_event_sender.report(nym_statistics::events::StatisticsEvent::new_connecting(
            config.enable_two_hop,
        )); // mobile "Connect" event
        let state_machine_handle = start_state_machine(
            config,
            network_env,
            account_controller_tx,
            account_controller_state,
            statistics_event_sender,
        )
        .await?;
        state_machine_handle.send_command(TunnelCommand::Connect);
        *guard = Some(state_machine_handle);
        Ok(())
    } else {
        Err(VpnError::InvalidStateError {
            details: "State machine is already running.".to_owned(),
        })
    }
}

pub(super) async fn start_state_machine(
    config: Box<VPNConfig>,
    network_env: Network,
    account_controller_tx: AccountCommandSender,
    account_controller_state: AccountStateReceiver,
    statistics_event_sender: StatisticsSender,
) -> Result<StateMachineHandle, VpnError> {
    let tunnel_type = if config.enable_two_hop {
        TunnelType::Wireguard
    } else {
        TunnelType::Mixnet
    };

    let entry_point = config.entry_gateway;
    let exit_point = config.exit_router;

    // Bootstrap the state machines gateway client with the static gateway client, so that we can
    // use the existing cached directory data.
    let gateway_cache_handle = gateway_cache::get_gateway_cache_handle().await?;
    let gateway_config = gateway_cache::get_gateway_config().await?;

    let nym_config = NymConfig {
        config_path: config.config_path,
        data_path: config.credential_data_path,
        gateway_config,
        network_env: network_env.clone(),
    };

    let user_agent = nym_sdk::UserAgent::from(config.user_agent.clone());

    let tunnel_settings = TunnelSettings {
        enable_ipv6: true,
        // ios: not used because vpn configuration is configured separately
        // todo: consider guarding with target_os
        allow_lan: true,
        residential_exit: config.residential_exit,
        tunnel_type,
        mixnet_tunnel_options: MixnetTunnelOptions::default(),
        wireguard_tunnel_options: WireguardTunnelOptions {
            multihop_mode: WireguardMultihopMode::Netstack,
            enable_bridges: config.enable_bridges,
        },
        gateway_performance_options: GatewayPerformanceOptions::default(),
        mixnet_client_config: None,
        entry_point: Box::new(entry_point),
        exit_point: Box::new(exit_point),
        dns: DnsOptions::default(),
        user_agent: Some(user_agent.clone()),
    };
    let tunnel_constants = TunnelConstants::default();

    let (command_sender, command_receiver) = mpsc::unbounded_channel();
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel();

    let state_listener = config.tun_status_listener;
    let event_broadcaster_handler = tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            if let Some(ref state_listener) = state_listener {
                (*state_listener).on_event(event);
            }
        }
    });

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let route_handler = crate::offline_monitor::get_route_handler().await?;

    let connectivity_handle = crate::offline_monitor::get_connectivity_handle().await?;

    let shutdown_token = CancellationToken::new();

    // Build validator client with ALL nym-api URLs for domain fronting fallback
    let validator_client = {
        let mut network_details_for_client = network_env.nym_network_details().clone();
        
        // Reorder URLs: frontdoors first, then direct URLs
        // This ensures frontdoors are tried first, but direct URLs are available as fallback
        if let Some(api_urls) = network_env.nym_api_urls() {
            let mut fronted_urls: Vec<_> = vec![];
            let mut direct_urls: Vec<_> = vec![];
            
            for url in api_urls {
                if url.front_hosts.is_some() && !url.front_hosts.as_ref().unwrap().is_empty() {
                    fronted_urls.push(url);
                } else {
                    direct_urls.push(url);
                }
            }
            
            // Frontdoors first, then direct URLs for fallback
            fronted_urls.extend(direct_urls);
            
            if !fronted_urls.is_empty() {
                network_details_for_client.nym_api_urls = Some(fronted_urls);
            }
        }
        
        let mut builder = nym_http_api_client::ClientBuilder::from_network(&network_details_for_client)
            .map_err(|e| VpnError::HttpClient(format!("Failed to build client from network: {e}")))?;
        
        // Add resolver overrides for front domains (required for domain fronting to work)
        if let Some(api_urls) = &network_details_for_client.nym_api_urls {
            for api_url in api_urls {
                if let Some(fronts) = &api_url.front_hosts {
                    let domain = if let Ok(url) = url::Url::parse(&api_url.url) {
                        url.host_str().unwrap_or(&api_url.url).to_string()
                    } else {
                        api_url.url.clone()
                    };
                    
                    for front in fronts {
                        if let Ok(addrs) = nym_vpn_api_client::str_to_socket_addr(front).await {
                            builder = builder.resolve_to_addrs(&domain, &addrs);
                            tracing::info!(
                                "Enabling Resolver override for {domain}: {}",
                                addrs.iter()
                                    .map(|addr| addr.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                    }
                }
            }
        }
        
        builder = builder.with_retries(3);  // Enable URL rotation and domain fronting fallback
        
        builder.build().map_err(|e| VpnError::HttpClient(format!("Failed to build HTTP client: {e}")))?
    };

    // Extract just the URL strings for topology provider rotation
    let nym_api_urls: Vec<url::Url> = network_env
        .nym_api_urls()
        .unwrap_or_else(|| vec![nym_network_defaults::ApiUrl {
            url: network_env.nym_api_url().to_string(),
            front_hosts: None,
        }])
        .into_iter()
        .filter_map(|api_url| url::Url::parse(&api_url.url).ok())
        .collect();

    let topology_provider = VpnTopologyProvider::new(
        nym_api_urls,
        validator_client,
        false,
        shutdown_token.child_token(),
    );
    topology_provider.fetch().await;

    let state_machine_handle = TunnelStateMachine::spawn(
        command_receiver,
        event_sender,
        nym_config,
        tunnel_settings,
        tunnel_constants,
        account_controller_tx,
        account_controller_state,
        statistics_event_sender,
        gateway_cache_handle,
        topology_provider,
        connectivity_handle,
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        route_handler,
        #[cfg(target_os = "ios")]
        Arc::new(crate::tunnel_provider::ios::OSTunProviderImpl::new(
            config.tun_provider,
        )),
        #[cfg(target_os = "android")]
        Arc::new(crate::tunnel_provider::android::AndroidTunProviderImpl::new(config.tun_provider)),
        shutdown_token.child_token(),
    )
    .await?;

    Ok(StateMachineHandle {
        state_machine_handle,
        event_broadcaster_handler,
        command_sender,
        shutdown_token,
    })
}

pub(super) struct StateMachineHandle {
    state_machine_handle: JoinHandle<()>,
    event_broadcaster_handler: JoinHandle<()>,
    command_sender: mpsc::UnboundedSender<TunnelCommand>,
    shutdown_token: CancellationToken,
}

impl StateMachineHandle {
    fn send_command(&self, command: TunnelCommand) {
        if let Err(e) = self.command_sender.send(command) {
            tracing::error!("Failed to send tunnel command: {}", e);
        }
    }

    pub(super) async fn shutdown_and_wait(self) {
        self.shutdown_token.cancel();

        if let Err(e) = self.state_machine_handle.await {
            tracing::error!("Failed to join on state machine handle: {}", e);
        }

        if let Err(e) = self.event_broadcaster_handler.await {
            tracing::error!("Failed to join on event broadcaster handle: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    // Test helper struct to mimic ApiUrl
    #[derive(Clone)]
    struct TestApiUrl {
        url: String,
        front_hosts: Option<Vec<String>>,
    }

    #[test]
    fn test_url_ordering_frontdoors_first() {
        // Test that URLs are reordered: frontdoors first, then direct
        let mixed_urls = vec![
            TestApiUrl {
                url: "https://direct.nym.com/api/".to_string(),
                front_hosts: None,
            },
            TestApiUrl {
                url: "https://frontdoor.vercel.app/nym-api/".to_string(),
                front_hosts: Some(vec!["vercel.app".to_string(), "vercel.com".to_string()]),
            },
        ];

        let mut fronted_urls: Vec<_> = vec![];
        let mut direct_urls: Vec<_> = vec![];
        
        for url in mixed_urls {
            if url.front_hosts.is_some() && !url.front_hosts.as_ref().unwrap().is_empty() {
                fronted_urls.push(url);
            } else {
                direct_urls.push(url);
            }
        }
        
        fronted_urls.extend(direct_urls);

        assert_eq!(fronted_urls.len(), 2, "Should keep all URLs");
        assert!(fronted_urls[0].url.contains("frontdoor"), "Frontdoor should be first");
        assert!(fronted_urls[1].url.contains("direct"), "Direct should be second");
    }

    #[test]
    fn test_multiple_frontdoors_preserved() {
        // Test that multiple frontdoor URLs are all kept and come first
        let all_fronted = vec![
            TestApiUrl {
                url: "https://direct.nym.com/api/".to_string(),
                front_hosts: None,
            },
            TestApiUrl {
                url: "https://frontdoor1.vercel.app/api/".to_string(),
                front_hosts: Some(vec!["vercel.app".to_string()]),
            },
            TestApiUrl {
                url: "https://frontdoor2.vercel.app/api/".to_string(),
                front_hosts: Some(vec!["vercel.com".to_string()]),
            },
        ];

        let mut fronted_urls: Vec<_> = vec![];
        let mut direct_urls: Vec<_> = vec![];
        
        for url in all_fronted {
            if url.front_hosts.is_some() && !url.front_hosts.as_ref().unwrap().is_empty() {
                fronted_urls.push(url);
            } else {
                direct_urls.push(url);
            }
        }
        
        fronted_urls.extend(direct_urls);

        assert_eq!(fronted_urls.len(), 3, "All URLs should be kept");
        assert!(fronted_urls[0].url.contains("frontdoor1"), "First frontdoor first");
        assert!(fronted_urls[1].url.contains("frontdoor2"), "Second frontdoor second");  
        assert!(fronted_urls[2].url.contains("direct"), "Direct URL last");
    }

    #[test]
    fn test_empty_fronts_treated_as_direct() {
        // URLs with empty front_hosts should be treated as direct
        let urls = vec![
            TestApiUrl {
                url: "https://api.nym.com/".to_string(),
                front_hosts: Some(vec![]),
            },
            TestApiUrl {
                url: "https://frontdoor.nym.com/".to_string(),
                front_hosts: Some(vec!["cdn.nym.com".to_string()]),
            },
        ];

        let mut fronted_urls: Vec<_> = vec![];
        let mut direct_urls: Vec<_> = vec![];
        
        for url in urls {
            if url.front_hosts.is_some() && !url.front_hosts.as_ref().unwrap().is_empty() {
                fronted_urls.push(url);
            } else {
                direct_urls.push(url);
            }
        }
        
        fronted_urls.extend(direct_urls);

        assert_eq!(fronted_urls.len(), 2);
        assert!(fronted_urls[0].url.contains("frontdoor"), "Frontdoor first");
        assert!(fronted_urls[1].url.contains("api.nym.com"), "Empty fronts treated as direct");
    }
}
