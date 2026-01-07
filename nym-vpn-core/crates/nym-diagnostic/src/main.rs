// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use clap::Parser;
use nym_vpn_network_config::Network;

use crate::{
    cli::{CliArgs, Command},
    diagnostic::DiagnosticHandler,
};

mod cli;
mod diagnostic;
mod logging;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    logging::setup_tracing_logger(&args)?;
    let network = Network::mainnet_default().ok_or(anyhow::anyhow!("Missing network config"))?;

    match args.command {
        Command::Run {
            gateway,
            skip_dns,
            skip_http,
        } => {
            let report = DiagnosticHandler::run(network, gateway, skip_dns, skip_http).await;
            tracing::info!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
    }
}
