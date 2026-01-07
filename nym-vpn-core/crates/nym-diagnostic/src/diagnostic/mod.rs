// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use nym_vpn_network_config::Network;
use serde::{Deserialize, Serialize};

use crate::{
    cli::{RegisterParams, RunParams},
    diagnostic::{
        dns::{CompleteDnsReport, DnsDiagnostic},
        gateway::{GatewayDiagnostic, GatewayReport},
        helpers::DiagnosticResult,
        http::{HttpDiagnostic, HttpReport},
        registration::{RegistrationDiagnostic, RegistrationReport},
    },
};

mod dns;
mod gateway;
mod helpers;
mod http;
mod registration;

pub struct DiagnosticHandler;

impl DiagnosticHandler {
    pub async fn run(network: Network, parameters: RunParams) -> DiagnosticReport {
        let dns_report = if !parameters.skip_dns {
            Some(
                DnsDiagnostic::run_diagnostic(&network)
                    .await
                    .inspect_err(|e| tracing::error!("Dns diagnostic error : {}", e.to_string())),
            )
        } else {
            None
        };

        let http_report = if !parameters.skip_http {
            Some(
                HttpDiagnostic::run_diagnostic(&network)
                    .await
                    .inspect_err(|e| tracing::error!("Http diagnostic error : {}", e.to_string())),
            )
        } else {
            None
        };

        let gateway_report = match parameters.gateway {
            Some(id) => Some(
                GatewayDiagnostic::run_diagnostic(&network, &id)
                    .await
                    .inspect_err(|e| tracing::error!("Gateway diagnostic error : {}", e)),
            ),
            None => None,
        };

        DiagnosticReport {
            dns: dns_report.map(Into::into),
            http: http_report.map(Into::into),
            gateway: gateway_report.map(Into::into),
        }
    }

    pub async fn register(
        network: Network,
        parameters: RegisterParams,
    ) -> DiagnosticResult<RegistrationReport> {
        RegistrationDiagnostic::run_diagnostic(
            &network,
            &parameters.gateway,
            &parameters.storage_path,
        )
        .await
        .inspect_err(|e| tracing::error!("Registration diagnostic error : {}", e))
        .into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    dns: Option<DiagnosticResult<CompleteDnsReport>>,
    http: Option<DiagnosticResult<HttpReport>>,
    gateway: Option<DiagnosticResult<GatewayReport>>,
}
