// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use nym_vpn_network_config::Network;
use serde::{Deserialize, Serialize};
use std::iter;

pub fn hostnames(network: &Network) -> Vec<String> {
    let api_urls = network
        .nym_api_urls_as_urls()
        .into_iter()
        .chain(network.nym_vpn_api_urls_as_urls())
        .flatten()
        .chain(iter::once(network.nyxd_url.clone()));

    // Convert str urls to hostnames
    api_urls
        .filter_map(|url| match url.host_str() {
            Some(host) => Some(host.to_string()),
            None => {
                tracing::warn!("URL has no host component: {}", url);
                None
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticResult<T> {
    pub ok: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> DiagnosticResult<T> {
    pub const SUCCESS: DiagnosticResult<()> = DiagnosticResult {
        ok: true,
        value: None,
        error: None,
    };

    pub fn from_err(error: impl ToString) -> DiagnosticResult<T> {
        Self {
            ok: false,
            value: None,
            error: Some(error.to_string()),
        }
    }

    pub fn from_value(value: T) -> DiagnosticResult<T> {
        Self {
            ok: true,
            value: Some(value),
            error: None,
        }
    }
}

impl<T, E> From<Result<T, E>> for DiagnosticResult<T>
where
    E: ToString,
{
    fn from(value: Result<T, E>) -> Self {
        let result = value.map_err(|e| e.to_string());
        DiagnosticResult {
            ok: result.is_ok(),
            error: result.as_ref().err().cloned(),
            value: result.ok(),
        }
    }
}
