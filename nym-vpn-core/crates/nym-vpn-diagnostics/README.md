# VPN Diagnostics Tool

A testing sting utility to identify firewall, DNS, and tunnel connectivity at various stages of the the nym-vpn connection lifecycle.

## Purpose

This tool identifies areas when VPN connections fail:
- Firewall blocking traffic
- DNS resolution failures (IPv4/IPv6)
- Tunnel interface misconfiguration
- Peer address unreachable
- UDP/TCP connection issues

## Building

```bash
cd nym-vpn-client/nym-vpn-core
cargo build --release -p nym-vpn-diagnostics --bin vpn-diagnostics
```

Binary location: `target/release/vpn-diagnostics`

## Quick Start

```bash
# Auto-detect everything (daemon state + tunnel interface)
sudo ./vpn-diagnostics

# List available tunnel interfaces
sudo ./vpn-diagnostics --list-interfaces

# Monitor during network transitions
sudo ./vpn-diagnostics --continuous 5

# Save results as JSON
sudo ./vpn-diagnostics --format json --output diagnostics.json
```

## Command Options

```
vpn-diagnostics [OPTIONS]

OPTIONS:
  -s, --state <STATE>              Tunnel state [default: auto-detect via daemon]
  -i, --interface <INTERFACE>      Tunnel interface name [default: auto-detect]
      --list-interfaces            List detected VPN tunnel interfaces
      --peer-v4 <PEER_V4>          IPv4 peer address to test
      --peer-v6 <PEER_V6>          IPv6 peer address to test
  -f, --format <FORMAT>            Output format: text or json [default: text]
  -c, --continuous <SECONDS>       Run continuously every N seconds
  -o, --output <FILE>              Output file for JSON results
  -h, --help                       Print help
```

## Output Format

### Text Output

```
VPN DIAGNOSTICS REPORT
Timestamp: 2025-11-05T12:00:00Z
Tunnel State: Connected
Tunnel Interface: utun5

--- FIREWALL TESTS ---
  [PASS] DNS Port IPv4             DNS port 53 accessible via IPv4
  [SKIP] DNS Port IPv6             IPv6 not available
  [PASS] TCP Outbound IPv4         TCP IPv4 outbound allowed (tested 1.1.1.1:443)
  [SKIP] TCP Outbound IPv6         IPv6 not available
  [PASS] UDP Outbound IPv4         UDP IPv4 outbound allowed
  [SKIP] UDP Outbound IPv6         IPv6 not available

--- DNS RESOLUTION TESTS ---
  [PASS] IPv4 Resolution           Resolved 4 IPv4 addresses
  [SKIP] IPv6 Resolution           No IPv6 addresses resolved
  Response Time: 245ms
  System DNS: 127.0.0.53 (using VPN local DNS forwarder)

--- TUNNEL CONNECTIVITY TESTS ---
  [PASS] Interface Exists          Tunnel interface utun5 exists
  [PASS] Ping Tunnel IPv4          Can ping through utun5 (IPv4)
  [SKIP] Ping Tunnel IPv6          IPv6 ping not working
  [PASS] Reach Peer IPv4           Can reach peer 10.0.0.1 via IPv4
  [SKIP] Reach Peer IPv6           No IPv6 peer address specified
  [PASS] UDP Socket IPv4           Can open UDP socket (IPv4)
  [SKIP] UDP Socket IPv6           IPv6 not available
  [PASS] Routing IPv4              IPv4 routing configured for utun5
  [SKIP] Routing IPv6              No IPv6 routes configured for utun5

--- NETWORK TESTS ---
  [PASS] Cloudflare DNS IPv4       Can reach DNS server 1.1.1.1
  [SKIP] Cloudflare DNS IPv6       IPv6 not available
  [PASS] Quad9 DNS IPv4            Can reach DNS server 9.9.9.9
  [SKIP] Quad9 DNS IPv6            IPv6 not available
  [PASS] DuckDuckGo IPv4           TCP connection to 52.250.42.157:443 successful
  [SKIP] DuckDuckGo IPv6           IPv6 not available
  [PASS] TCP Connect IPv4          TCP connection to 1.1.1.1:443 successful
  [SKIP] TCP Connect IPv6          IPv6 not available

--- OVERALL STATUS ---
All tests passed - VPN functioning correctly

NOTE: During transition states (connecting/disconnecting), some failures are expected.
```

### JSON Output

```json
{
  "timestamp": "2025-11-05T12:00:00Z",
  "tunnel_state": "Connected",
  "firewall_tests": {
    "can_resolve_dns_v4": {
      "passed": true,
      "error": null,
      "details": "DNS port 53 accessible via IPv4"
    }
  },
  "dns_tests": {
    "can_resolve_ipv4_addresses": {
      "passed": true,
      "error": null,
      "details": "Resolved 4 IPv4 addresses"
    },
    "dns_response_time_ms": 245
  },
  "overall_status": "AllTestsPassed"
}
```

## Usage Scenarios

### 1. Baseline Check (Before Connecting)

```bash
sudo ./vpn-diagnostics --state disconnected
```

**Expected Results:**
- Network and DNS tests: PASS
- Tunnel tests: SKIP (no tunnel interface)
- Firewall: PASS (allows outbound traffic)

### 2. During Connection

```bash
sudo ./vpn-diagnostics
```

Auto-detects daemon state (connecting) and tunnel interface.

**Check for:**
- Tunnel interface created
- Firewall not blocking DNS
- UDP/TCP connections allowed

### 3. Connected State Validation

```bash
sudo ./vpn-diagnostics --peer-v4 10.0.0.1
```

**Expected Results:**
- All tests: PASS
- Can ping through tunnel
- Routing configured
- Can reach peer address

### 4. Network Transition Monitoring

```bash
# Terminal 1: Monitor diagnostics
sudo ./vpn-diagnostics --continuous 5

# Terminal 2: Test actual traffic
curl -v https://duckduckgo.com
curl -v https://api.ipify.org
ping -c 3 1.1.1.1
```

Watch for recovery after:
- WiFi network switch
- System sleep/wake
- Network interface changes

### 5. Firewall Issue Detection

```bash
RUST_LOG=debug sudo ./vpn-diagnostics \
  --format json \
  --output firewall-debug.json
```

## Interpreting Results

### FIREWALL LOCKOUT

**Symptoms:**
```
[FAIL] TCP Outbound IPv4 - ERROR: Connection refused
[FAIL] UDP Outbound IPv4 - ERROR: Permission denied
[PASS] DNS Port IPv4
```

**Root Cause:** Firewall rules blocking critical traffic before tunnel established

**Solution:** Check firewall policy in `nym-firewall` crate, ensure DNS and API endpoints are allowed

---

## What Gets Tested

### Firewall Layer
- DNS port (UDP 53) accessible
- Outbound TCP allowed (IPv4/IPv6)
- Outbound UDP allowed (IPv4/IPv6)
- Critical traffic not blocked

### DNS Layer
- Domain name resolution (IPv4/IPv6)
- DNS response times (<10s timeout)
- Multiple domains resolve successfully
- System DNS configuration (detects VPN local forwarder)

### Tunnel Layer
- Tunnel interface exists
- Ping through tunnel (IPv4/IPv6)
- Peer address reachable (IPv4/IPv6)
- UDP sockets functional (IPv4/IPv6)
- Routing configured correctly (IPv4/IPv6)

### Network Layer
- Public DNS servers reachable (Cloudflare, Quad9)
- DuckDuckGo reachable (real-world connectivity test)
- TCP connections work (IPv4/IPv6)
- General internet connectivity

## Exit Codes

- **0**: All tests passed
- **1**: One or more tests failed
- **2**: Critical error running diagnostics

## Real-World Debugging Examples

### Example 1: "Connection Cancelled" Errors

**Problem:** Repeated "connection cancelled" during registration

**Test:**
```bash
sudo ./vpn-diagnostics --continuous 3 --output connection-debug
```

**Look for:**
- Firewall blocking UDP to peer address
- DNS working but TCP connections blocked
- Tunnel interface exists but no routing

---

### Example 2: DNS Timeouts

**Problem:** DNS lookups timing out intermittently

**Test:**
```bash
sudo ./vpn-diagnostics --continuous 10 --format json --output dns-monitoring
```

**Look for:**
- `dns_response_time_ms` > 5000 (slow)
- `can_resolve_ipv4_addresses.passed: false`
- `firewall_allows_outbound_udp_v4.passed: false`

---

### Example 3: Network Change Recovery

**Problem:** VPN doesn't reconnect after WiFi switch

**Test:**
```bash
# Before switch
sudo ./vpn-diagnostics --output before.json

# After switch
sudo ./vpn-diagnostics --output after.json

# Compare
diff <(jq '.firewall_tests' before.json) <(jq '.firewall_tests' after.json)
```

**Look for:**
- Firewall state changed
- DNS servers changed
- Routing disappeared

## Integration with VPN Daemon

### Auto-Detection Features

The tool queries the VPN daemon via gRPC to automatically detect:
- Current tunnel state (Disconnected, Connecting, Connected, etc.)
- Active tunnel interface (utun4, utun5, tun0, etc.)

**No manual state/interface specification needed** when daemon is running.


## Platform Support

### Fully Supported
- **macOS** (ARM/Intel): All tests work
- **Linux**: All tests work

### Partially Supported
- **Windows**: Firewall, DNS, and network tests work. Tunnel tests (ping, routing, interface detection) will be skipped.

### What Works on All Platforms
- Firewall tests (TCP/UDP connectivity)
- DNS resolution tests
- Network connectivity tests (Cloudflare, Quad9, DuckDuckGo)
- Daemon state detection (via gRPC)
- JSON output

### Platform-Specific Features
These tests are **SKIPPED** on Windows and other platforms:
- Tunnel interface auto-detection (requires `ifconfig` or `ip link`)
- Ping tests (requires `ping`/`ping6` commands)
- Routing checks (requires `netstat` or `ip route`)
- `--list-interfaces` flag

**For Windows users:** You can still use the tool to test firewall and DNS issues, but you'll need to manually verify tunnel connectivity using Windows networking tools.

## FAQ

**Q: Why do I need sudo/admin privileges?**  
A: Network tests require raw socket access and interface inspection.


**Q: What's the difference between FAIL and SKIP?**  
A:
- FAIL: Test ran and failed - indicates a problem
- SKIP: Test couldn't run (expected behavior, e.g., no IPv6 support, or Windows platform)

**Q: How often should I run continuous monitoring?**  
A:
- Normal debugging: Every 10-30 seconds
- Network change testing: Every 5 seconds
- Production monitoring: Every 60 seconds

## Expected Results by State

### Disconnected
- Network connectivity: PASS
- Firewall: PASS (allows outbound)
- DNS resolution: PASS
- Tunnel tests: SKIP

### Connecting
- Firewall: PASS (allows VPN traffic)
- DNS: PASS (resolves API endpoints)
- Tunnel interface: PASS (created)
- Tunnel connectivity: May FAIL during transition (expected)

### Connected
- All firewall tests: PASS
- All DNS tests: PASS
- All tunnel tests: PASS
- Can reach internet through tunnel: PASS

### Connection Failed
Check which category fails:
- Firewall: Rules blocking traffic
- DNS: Cannot resolve hostnames
- Tunnel: Interface/routing broken
- Network: No internet connectivity

## Catching the Firewall Lockout Bug

```bash
# Before login (no account)
sudo ./vpn-diagnostics --state disconnected
# Expected: All network tests PASS

# After daemon starts (account check pending)
sudo ./vpn-diagnostics
# If firewall applied BEFORE account check:
[FAIL] TCP Outbound IPv4 - ERROR: Connection refused
[FAIL] DNS Port IPv4 - ERROR: Permission denied
[PASS] Interface Exists - Tunnel interface utun5 exists

# This immediately shows the firewall applied too early
```
