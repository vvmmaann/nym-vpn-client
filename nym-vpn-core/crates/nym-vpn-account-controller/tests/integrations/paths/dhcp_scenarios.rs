// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use crate::common::{TestBench, account_summary::*, endpoints};
use nym_vpn_lib_types::{AccountControllerErrorStateReason, AccountControllerState};

// DHCP scenario tests for account controller behavior during network configuration changes.
// These validate proper handling of DHCP lease events, network interface switches,
// and configuration updates that users commonly encounter.

#[tokio::test]
async fn dhcp_lease_expiration_during_account_sync() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // DHCP lease expires - network interface temporarily loses connectivity
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // DHCP renews lease and network comes back
    test_bench.simulate_dhcp_renewal().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    test_bench.simulate_tunnel_ready().await?;

    // Verify the transition was clean with no failed attempts
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(
        failed_attempts, 0,
        "AC should not have failed network attempts during DHCP transition"
    );

    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}

#[tokio::test]
async fn network_interface_change_wifi_to_ethernet() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate network interface change (wifi -> ethernet)
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // Simulate new interface coming online
    test_bench.simulate_dhcp_renewal().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    test_bench.simulate_tunnel_ready().await?;

    // AC should adapt to new interface without failed attempts
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(
        failed_attempts, 0,
        "AC should not have failed network attempts during interface change"
    );

    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}

#[tokio::test]
async fn partial_connectivity_dhcp_server_unavailable() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate partial connectivity - DHCP server unavailable but some network up
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // AC should not attempt networking with partial connectivity
    let result = test_bench
        .command_sender
        .background_refresh_account_state()
        .await;
    assert!(
        result.is_err(),
        "AC should not attempt networking with partial connectivity"
    );

    Ok(())
}

#[tokio::test]
async fn rapid_dhcp_changes_stability_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

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
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}

#[tokio::test]
async fn dhcp_timeout_recovery_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate DHCP timeout (common in enterprise networks)
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // Wait for DHCP timeout recovery
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Simulate DHCP recovery
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should recover properly without failed attempts
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(
        failed_attempts, 0,
        "AC should not have failed network attempts during timeout recovery"
    );

    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}

#[tokio::test]
async fn dhcp_lease_renewal_during_connection() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate tunnel in connecting state (firewall UP)
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate DHCP lease renewal during connection
    test_bench.simulate_dhcp_renewal().await?;

    // AC should stay in ReadyToConnect (network still available)
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Verify no failed network attempts during renewal
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(
        failed_attempts, 0,
        "AC should not have failed network attempts during DHCP renewal"
    );

    // Only when tunnel signals ready should AC proceed with networking
    test_bench.simulate_tunnel_ready().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}

#[tokio::test]
async fn network_configuration_change_handling() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate network configuration change (new IP, gateway, DNS)
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // Simulate new network configuration
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should handle new configuration without failed attempts
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(
        failed_attempts, 0,
        "AC should not have failed network attempts during config change"
    );

    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}

#[tokio::test]
async fn dhcp_server_restart_scenario() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Simulate DHCP server restart
    test_bench.simulate_dhcp_lease_expiration().await?;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // Simulate DHCP server coming back online
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.simulate_tunnel_ready().await?;

    // AC should recover from DHCP server restart without failed attempts
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(
        failed_attempts, 0,
        "AC should not have failed network attempts during DHCP server restart"
    );

    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}
