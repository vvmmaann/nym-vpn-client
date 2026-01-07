// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{conversions::ConversionError, proto};

impl TryFrom<nym_diagnostic::DiagnosticReport> for proto::DiagnosticReport {
    type Error = ConversionError;
    fn try_from(value: nym_diagnostic::DiagnosticReport) -> Result<Self, Self::Error> {
        Ok(Self {
            json: serde_json::to_string(&value).map_err(|e| {
                ConversionError::generic(format!("failed to convert Diagnostic Report : {e}"))
            })?,
        })
    }
}

impl TryFrom<proto::DiagnosticReport> for nym_diagnostic::DiagnosticReport {
    type Error = ConversionError;
    fn try_from(value: proto::DiagnosticReport) -> Result<Self, Self::Error> {
        serde_json::from_str(&value.json).map_err(|e| {
            ConversionError::generic(format!("failed to convert Diagnostic Report : {e}"))
        })
    }
}

impl From<proto::DiagnosticRunParams> for nym_diagnostic::cli::RunParams {
    fn from(value: proto::DiagnosticRunParams) -> Self {
        Self {
            gateway: value.gateway.map(|g| g.id),
            skip_dns: value.skip_dns,
            skip_http: value.skip_http,
        }
    }
}

impl From<nym_diagnostic::cli::RunParams> for proto::DiagnosticRunParams {
    fn from(value: nym_diagnostic::cli::RunParams) -> Self {
        Self {
            gateway: value.gateway.map(|id| proto::GatewayId { id }),
            skip_dns: value.skip_dns,
            skip_http: value.skip_http,
        }
    }
}
