// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

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

impl TryFrom<nym_diagnostic::RegistrationDiagnosticReport> for proto::RegistrationDiagnosticReport {
    type Error = ConversionError;
    fn try_from(value: nym_diagnostic::RegistrationDiagnosticReport) -> Result<Self, Self::Error> {
        Ok(Self {
            json: serde_json::to_string(&value).map_err(|e| {
                ConversionError::generic(format!(
                    "failed to convert Registration Diagnostic Report : {e}"
                ))
            })?,
        })
    }
}

impl TryFrom<proto::RegistrationDiagnosticReport> for nym_diagnostic::RegistrationDiagnosticReport {
    type Error = ConversionError;
    fn try_from(value: proto::RegistrationDiagnosticReport) -> Result<Self, Self::Error> {
        serde_json::from_str(&value.json).map_err(|e| {
            ConversionError::generic(format!(
                "failed to convert Registration Diagnostic Report : {e}"
            ))
        })
    }
}

impl TryFrom<proto::DiagnosticRegisterParams> for nym_diagnostic::cli::RegisterParams {
    type Error = ConversionError;
    fn try_from(value: proto::DiagnosticRegisterParams) -> Result<Self, Self::Error> {
        Ok(Self {
            gateway: value
                .gateway
                .map(|g| g.id)
                .ok_or_else(|| ConversionError::generic("missing gateway id"))?,
            storage_path: value.storage_path.map(PathBuf::from),
        })
    }
}

impl From<nym_diagnostic::cli::RegisterParams> for proto::DiagnosticRegisterParams {
    fn from(value: nym_diagnostic::cli::RegisterParams) -> Self {
        Self {
            gateway: Some(proto::GatewayId { id: value.gateway }),
            storage_path: value
                .storage_path
                .and_then(|p| p.to_str().map(str::to_string)),
        }
    }
}
