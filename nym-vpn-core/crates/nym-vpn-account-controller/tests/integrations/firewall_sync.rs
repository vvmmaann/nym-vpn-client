// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use crate::common::{TestBench, account_summary::*, endpoints};
use nym_vpn_lib_types::{AccountControllerErrorStateReason, AccountControllerState};

/// Tests for firewall state synchronization between tunnel state machine and account controller.
/// These tests verify that the AC properly handles firewall state changes and doesn't attempt
/// networking when the tunnel SM indicates the firewall is blocking.

#[tokio::test]
async fn offline_state_firewall_sync_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks for normal operation
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    // Start with account stored and ready
    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate tunnel state machine entering offline state
    // This should call set_vpn_api_firewall_up() to notify AC that firewall is blocking
    test_bench.simulate_tunnel_offline_state().await?;
    
    // AC should transition to offline state and know firewall is blocking
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate wake-up: network comes back but AC should not immediately try networking
    // because firewall is still blocking
    test_bench.go_online()?;
    
    // AC should stay in offline state until SM tells it firewall is down
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Verify no failed network attempts during this period
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(failed_attempts, 0, "AC should not attempt networking when firewall is blocking");

    // Only when SM transitions to connected should AC start networking
    test_bench.simulate_tunnel_connected_state().await?;
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn sleep_wake_firewall_sync_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    // Start with account ready to connect
    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate sleep: tunnel goes offline and firewall blocks everything
    test_bench.simulate_sleep().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate wake: network comes back but firewall still blocking
    test_bench.simulate_wake().await?;
    test_bench.go_online()?;

    // AC should NOT immediately start networking (firewall still blocking)
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Verify no failed network attempts during this period
    let failed_attempts = test_bench.get_failed_network_attempts().await;
    assert_eq!(failed_attempts, 0, "AC should not attempt networking when firewall is blocking after wake");

    // Only when SM signals firewall down should AC start networking
    test_bench.simulate_tunnel_ready().await?;
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn firewall_state_transition_matrix() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Test all state transitions and verify firewall sync
    let state_transitions = vec![
        (TunnelState::Connecting, AccountControllerState::Offline),     // firewall UP
        (TunnelState::Connected, AccountControllerState::Syncing),      // firewall DOWN  
        (TunnelState::Offline, AccountControllerState::Offline),        // firewall UP
        (TunnelState::Disconnected, AccountControllerState::ReadyToConnect), // firewall DOWN
    ];

    for (tunnel_state, expected_account_state) in state_transitions {
        test_bench.simulate_tunnel_state(tunnel_state).await?;
        test_bench.assert_state(expected_account_state).await;
    }

    Ok(())
}

#[tokio::test]
async fn network_timing_edge_cases() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Test rapid state changes to ensure AC doesn't get confused
    for _ in 0..10 {
        test_bench.go_offline()?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        test_bench.go_online()?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Verify AC doesn't get confused by rapid transitions
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    Ok(())
}

#[tokio::test]
async fn graceful_wakeup_pattern_test() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Simulate sleep
    test_bench.simulate_sleep().await?;

    // Simulate wake with network stabilization delay (as mentioned by Tommy)
    test_bench.simulate_wake_with_delay(std::time::Duration::from_secs(3)).await?;

    // Verify AC waits for proper signal before networking
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Only after SM signals firewall down should AC start
    test_bench.simulate_tunnel_ready().await?;
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

#[tokio::test]
async fn firewall_sync_performance_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    let start = std::time::Instant::now();

    // Measure time for state synchronization
    test_bench.simulate_tunnel_offline_state().await?;
    let sync_time = start.elapsed();

    // Verify sync happens within acceptable time (< 100ms)
    assert!(sync_time.as_millis() < 100, "Firewall sync took too long: {:?}", sync_time);

    Ok(())
}

/// Test that AC properly handles firewall blocking during account operations
#[tokio::test]
async fn firewall_blocking_during_operations() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate tunnel going offline during account operations
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Try to perform account operations while firewall is blocking
    let result = test_bench.command_sender.background_refresh_account_state().await;
    assert!(result.is_err(), "Account operations should fail when firewall is blocking");
    
    // Verify the error is the expected offline error
    match result.unwrap_err() {
        nym_vpn_lib_types::AccountCommandError::Offline => {
            // Expected error
        }
        other => panic!("Expected Offline error, got: {:?}", other),
    }

    Ok(())
}

/// Test DHCP lease expiration scenarios with firewall sync
#[tokio::test]
async fn dhcp_lease_expiration_firewall_sync() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;
    test_bench.assert_state(AccountControllerState::ReadyToConnect).await;

    // Simulate DHCP lease expiration causing network interface change
    test_bench.simulate_dhcp_lease_expiration().await?;
    
    // Tunnel should go offline due to network change
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Simulate DHCP renewal and network restoration
    test_bench.simulate_dhcp_renewal().await?;
    test_bench.go_online()?;

    // AC should still be offline until tunnel signals ready
    test_bench.assert_state(AccountControllerState::Offline).await;

    // Only when tunnel is ready should AC resume operations
    test_bench.simulate_tunnel_ready().await?;
    test_bench.assert_state(AccountControllerState::Syncing).await;

    Ok(())
}

// Helper types for test simulation
#[derive(Debug, Clone, Copy, PartialEq)]
enum TunnelState {
    Connecting,
    Connected,
    Offline,
    Disconnected,
}
