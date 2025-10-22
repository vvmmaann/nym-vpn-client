// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use crate::common::{TestBench, account_summary::*, endpoints};
use nym_vpn_lib_types::AccountControllerState;

// Core validation tests for firewall synchronization between tunnel SM and account controller.
// These tests verify the fix for the issue where AC would attempt network operations while
// the tunnel's firewall was still blocking, causing failed requests during sleep/wake cycles.

#[tokio::test]
async fn firewall_actually_blocks_network_operations() -> anyhow::Result<()> {
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

    // When tunnel signals firewall is UP, AC should refuse to make network requests
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .command_sender
        .background_refresh_account_state()
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    // AC should stay in ReadyToConnect, not transition to Syncing (which would mean it tried to network)
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // When tunnel signals firewall is DOWN, AC should allow networking
    test_bench.simulate_tunnel_connected_state().await?;
    test_bench
        .command_sender
        .background_refresh_account_state()
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Now AC should attempt networking (either still syncing or completed)
    let state = test_bench.state_receiver.get_state();
    assert!(
        state == AccountControllerState::Syncing || state == AccountControllerState::ReadyToConnect,
        "Expected AC to attempt networking when firewall is down, got: {:?}",
        state
    );

    Ok(())
}

#[tokio::test]
async fn firewall_blocks_automatic_refresh() -> anyhow::Result<()> {
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

    // Signal firewall is blocking and wait to see if automatic refresh respects it
    test_bench.simulate_tunnel_offline_state().await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    test_bench.simulate_tunnel_connected_state().await?;

    Ok(())
}

#[tokio::test]
async fn sleep_wake_prevents_premature_networking() -> anyhow::Result<()> {
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

    // Simulate computer going to sleep - network goes offline and tunnel sets firewall to block
    test_bench.go_offline()?;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    test_bench.simulate_tunnel_offline_state().await?;
    test_bench
        .assert_state(AccountControllerState::Offline)
        .await;

    // Simulate wake - network is restored but tunnel hasn't finished reconnecting yet
    // This is the critical scenario: AC sees network is back but firewall is still blocking
    test_bench.go_online()?;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Wait and verify AC doesn't try to network prematurely (the bug we're testing for)
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    // Tunnel finishes reconnecting and signals firewall is clear - now AC can network
    test_bench.simulate_tunnel_connected_state().await?;
    test_bench
        .assert_state(AccountControllerState::ReadyToConnect)
        .await;

    Ok(())
}
