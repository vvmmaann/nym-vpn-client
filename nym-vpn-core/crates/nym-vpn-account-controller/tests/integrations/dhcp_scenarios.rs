// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use crate::common::{TestBench, account_summary::*, endpoints};
use nym_vpn_lib_types::{AccountControllerErrorStateReason, AccountControllerState};

/// Tests for DHCP-related scenarios that affect account controller behavior.
/// These tests verify that the AC properly handles network configuration changes,
/// DHCP lease expiration/renewal, and network interface transitions.

#[tokio::test]
async fn dhcp_lease_expiration_during_account_sync() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Start account sync
    test_bench.command_sender.background_refresh_account_state().await?;

    // Simulate DHCP lease expiration during sync
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;

    // AC should handle the network change gracefully
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate DHCP renewal and network restoration
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should resume normal operation
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn network_interface_change_wifi_to_ethernet() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate network interface change (wifi -> ethernet)
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate new interface coming online
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should adapt to new interface
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn partial_connectivity_dhcp_server_unavailable() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate partial connectivity - DHCP server unavailable but some network up
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // AC should not attempt networking with partial connectivity
    let result = test_bench.command_sender.background_refresh_account_state().await;
    assert!(result.is_err(), "AC should not attempt networking with partial connectivity");

    Ok(())
}

#[tokio::test]
async fn rapid_dhcp_changes_stability_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Simulate rapid DHCP changes (common in unstable networks)
    for _ in 0..5 {
        test_bench.simulate_dhcp_lease_expiration().await?;
        test_bench.simulate_tunnel_offline_state().await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        test_bench.simulate_dhcp_renewal().await?;
        test_bench.simulate_tunnel_ready().await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // AC should be stable despite rapid changes
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    Ok(())
}

#[tokio::test]
async fn dhcp_timeout_recovery_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate DHCP timeout (common in enterprise networks)
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Wait for DHCP timeout recovery
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Simulate DHCP recovery
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should recover properly
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn dhcp_lease_renewal_during_connection() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Start connection process
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate DHCP lease renewal during connection (should not affect AC)
    test_bench.simulate_dhcp_renewal().await?;
    
    // AC should still be offline until tunnel is ready
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Only when tunnel signals ready should AC proceed
    test_bench.simulate_tunnel_ready().await?;
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn network_configuration_change_handling() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate network configuration change (new IP, gateway, DNS)
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate new network configuration
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should handle new configuration
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn dhcp_server_restart_scenario() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate DHCP server restart
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate DHCP server coming back online
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should recover from DHCP server restart
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}
