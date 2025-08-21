use nym_connection_monitor::ConnectionStatusEvent;
use serde::{Deserialize, Serialize};

pub use nym_client_core::gateway_probe::{Entry, Exit, WgProbeResults};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub node: String,
    pub used_entry: String,
    pub outcome: ProbeOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOutcome {
    pub as_entry: Entry,
    pub as_exit: Option<Exit>,
    pub wg: Option<WgProbeResults>,
}

#[derive(Debug, Clone, Default)]
pub struct IpPingReplies {
    pub ipr_tun_ip_v4: bool,
    pub ipr_tun_ip_v6: bool,
    pub external_ip_v4: bool,
    pub external_ip_v6: bool,
}

impl From<IpPingReplies> for Exit {
    fn from(value: IpPingReplies) -> Self {
        Self {
            can_connect: true,
            can_route_ip_v4: value.ipr_tun_ip_v4,
            can_route_ip_external_v4: value.external_ip_v4,
            can_route_ip_v6: value.ipr_tun_ip_v6,
            can_route_ip_external_v6: value.external_ip_v6,
        }
    }
}

impl IpPingReplies {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_event(&mut self, event: &ConnectionStatusEvent) {
        match event {
            ConnectionStatusEvent::MixnetSelfPing => {}
            ConnectionStatusEvent::Icmpv4IprTunDevicePingReply => self.ipr_tun_ip_v4 = true,
            ConnectionStatusEvent::Icmpv6IprTunDevicePingReply => self.ipr_tun_ip_v6 = true,
            ConnectionStatusEvent::Icmpv4IprExternalPingReply => self.external_ip_v4 = true,
            ConnectionStatusEvent::Icmpv6IprExternalPingReply => self.external_ip_v6 = true,
        }
    }
}
