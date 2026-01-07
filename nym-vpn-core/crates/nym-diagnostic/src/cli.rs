// Copyright 2025 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: GPL-3.0-only

use clap::Parser;
use std::{path::PathBuf, sync::OnceLock};

// Helper for passing LONG_VERSION to clap
fn pretty_build_info_static() -> &'static str {
    static PRETTY_BUILD_INFORMATION: OnceLock<String> = OnceLock::new();
    PRETTY_BUILD_INFORMATION.get_or_init(|| nym_bin_common::bin_info_local_vergen!().pretty_print())
}

#[derive(Clone, Debug, Parser)]
#[clap(author = "Nymtech", version, long_version = pretty_build_info_static(), about)]
pub(crate) struct CliArgs {
    /// Logging verbosity.
    #[arg(long, short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,

    // SW will wait a bit
    /// custom network
    // #[arg(short, long, hide = true)]
    // pub network: String,

    /// Override the default user agent string.
    // #[arg(long, value_parser = parse_user_agent)]
    // pub user_agent: Option<UserAgent>,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Command,
}

impl CliArgs {
    #[allow(dead_code)] // false positive, it's used in the binary
    pub fn verbosity_level(&self) -> tracing::Level {
        match self.verbose {
            0 => tracing::Level::INFO,
            1 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE,
        }
    }
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Command {
    /// Run diagnostic
    Run(RunParams),

    /// Register to a gateway for diagnostic. SUCCESSFUL RUNS ARE WASTING AN ENTRY TICKET
    Register(RegisterParams),
}

#[derive(Debug, Clone, clap::Args)]
pub struct RunParams {
    /// Id of the gateway we are going to connect to.
    #[arg(long)]
    pub gateway: Option<String>,

    /// Skip DNS diagnostic
    #[clap(long, action = clap::ArgAction::SetTrue)]
    pub skip_dns: bool,

    /// Skip HTTP diagnostic
    #[clap(long, action = clap::ArgAction::SetTrue)]
    pub skip_http: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct RegisterParams {
    /// Id of the gateway we are going to connect to.
    #[arg(long)]
    pub gateway: String,

    /// Path to the storage dir
    #[arg(long)]
    pub storage_path: PathBuf,
}

// fn parse_user_agent(user_agent: &str) -> Result<UserAgent, String> {
//     UserAgent::from_str(user_agent).map_err(|e| e.to_string())
// }
