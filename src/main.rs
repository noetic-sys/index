//! Index CLI - semantic code search for your dependencies.

mod cli;
mod commands;
mod indexer;
mod local;
mod manifests;
mod registry;
mod types;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing (controlled by RUST_LOG env var)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    cli.command.execute().await
}
