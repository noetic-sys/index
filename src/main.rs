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
    // MCP mode inits its own subscriber (writes to stderr), so skip here
    let is_mcp = std::env::args().nth(1).is_some_and(|arg| arg == "mcp");

    if !is_mcp {
        // Initialize tracing for non-MCP commands (controlled by RUST_LOG env var)
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_target(false)
            .init();
    }

    let cli = Cli::parse();
    cli.command.execute().await
}
