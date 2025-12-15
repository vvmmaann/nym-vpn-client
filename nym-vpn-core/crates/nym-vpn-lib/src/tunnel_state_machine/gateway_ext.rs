// Copyright 2023 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use std::net::SocketAddr;

pub trait GatewayExt {
    /// Returns a list of all endpoints with WSS port if available, otherwise WS port.
    fn endpoints(&self) -> Vec<SocketAddr>;
}

impl GatewayExt for nym_gateway_directory::Gateway {
    /// Always returns the socket addresses for ws. If a WSS port is defined those
    /// will be returned as well so both are usable.
    fn endpoints(&self) -> Vec<SocketAddr> { 
        let mut ports: Vec<u16> = vec![self.entry_info.ws_port];
        if let Some(wss_port) = self.entry_info.wss_port {
            ports.push(wss_port);
        }

        itertools::iproduct!(self.ips.clone(), ports)
            .map(|(ip, port)| SocketAddr::new(ip, port))
            .collect::<Vec<_>>()
    }
}
