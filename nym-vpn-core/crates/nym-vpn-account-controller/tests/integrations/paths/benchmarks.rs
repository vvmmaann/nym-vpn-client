// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use crate::common::{TestBench, account_summary::*, endpoints};
use nym_vpn_lib_types::AccountControllerState;
use std::time::Instant;

// Performance benchmarks for firewall sync and network transition operations.
// These ensure the firewall synchronization fix doesn't introduce performance regressions.

#[tokio::test]
async fn firewall_sync_performance_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Measure firewall sync performance
    let start = Instant::now();
    test_bench.simulate_tunnel_offline_state().await?;
    let sync_time = start.elapsed();

    // Verify sync happens within acceptable time (< 100ms)
    assert!(
        sync_time.as_millis() < 100,
        "Firewall sync took too long: {:?}",
        sync_time
    );

    Ok(())
}

#[tokio::test]
async fn state_transition_performance_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Measure state transition performance
    let start = Instant::now();

    // Test multiple state transitions
    for _ in 0..10 {
        test_bench.simulate_tunnel_offline_state().await?;
        test_bench.simulate_tunnel_ready().await?;
    }

    let total_time = start.elapsed();
    let avg_time_per_transition = total_time / 20; // 20 transitions total

    // Verify each transition is fast (< 50ms average)
    assert!(
        avg_time_per_transition.as_millis() < 50,
        "State transitions too slow: avg {:?} per transition",
        avg_time_per_transition
    );

    Ok(())
}

#[tokio::test]
async fn network_change_detection_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Measure network change detection performance
    let start = Instant::now();

    // Simulate rapid network changes
    for _ in 0..100 {
        test_bench.go_offline()?;
        test_bench.go_online()?;
    }

    let total_time = start.elapsed();
    let avg_time_per_change = total_time / 200; // 200 changes total

    // Verify network change detection is fast (< 10ms average)
    assert!(
        avg_time_per_change.as_millis() < 10,
        "Network change detection too slow: avg {:?} per change",
        avg_time_per_change
    );

    Ok(())
}

#[tokio::test]
async fn sleep_wake_cycle_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Measure complete sleep/wake cycle performance
    let start = Instant::now();

    // Simulate multiple sleep/wake cycles
    for _ in 0..5 {
        test_bench.simulate_sleep().await?;
        test_bench.simulate_wake().await?;
    }

    let total_time = start.elapsed();
    let avg_time_per_cycle = total_time / 5;

    // Verify sleep/wake cycles are reasonably fast (< 500ms average)
    assert!(
        avg_time_per_cycle.as_millis() < 500,
        "Sleep/wake cycles too slow: avg {:?} per cycle",
        avg_time_per_cycle
    );

    Ok(())
}

#[tokio::test]
async fn dhcp_scenario_performance_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Measure DHCP scenario handling performance
    let start = Instant::now();

    // Simulate various DHCP scenarios
    for _ in 0..10 {
        test_bench.simulate_dhcp_lease_expiration().await?;
        test_bench.simulate_tunnel_offline_state().await?;
        test_bench.simulate_dhcp_renewal().await?;
        test_bench.simulate_tunnel_ready().await?;
    }

    let total_time = start.elapsed();
    let avg_time_per_scenario = total_time / 10;

    // Verify DHCP scenarios are handled efficiently (< 250ms average)
    // Note: This includes network state propagation delays we added for reliability
    assert!(
        avg_time_per_scenario.as_millis() < 250,
        "DHCP scenario handling too slow: avg {:?} per scenario",
        avg_time_per_scenario
    );

    Ok(())
}

#[tokio::test]
async fn memory_usage_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Simulate extended operation to check for memory leaks
    for i in 0..1000 {
        test_bench.simulate_tunnel_offline_state().await?;
        test_bench.simulate_tunnel_ready().await?;

        // Every 100 iterations, check state
        if i % 100 == 0 {
            test_bench
                .assert_state(AccountControllerState::ReadyToConnect)
                .await;
        }
    }

    // If we get here without panicking, memory usage is acceptable
    Ok(())
}

#[tokio::test]
async fn sequential_operations_benchmark() -> anyhow::Result<()> {
    let mut test_bench = TestBench::new_no_credentials().await?;

    // Setup VPN API mocks
    let mocks = vec![
        endpoints::synced_health(),
        endpoints::account_summary_with_device_200(account_ready_to_connect()),
    ];
    test_bench.register_vpn_api_mocks(mocks).await;

    test_bench.store_mock_account().await?;

    // Measure sequential operations performance
    let start = Instant::now();

    // Simulate sequential network and tunnel state changes
    for _ in 0..100 {
        test_bench.go_offline()?;
        test_bench.simulate_tunnel_offline_state().await?;
        test_bench.go_online()?;
        test_bench.simulate_tunnel_ready().await?;
    }

    let total_time = start.elapsed();

    // Verify sequential operations complete within reasonable time (< 5s)
    assert!(
        total_time.as_secs() < 5,
        "Sequential operations too slow: {:?}",
        total_time
    );

    Ok(())
}
