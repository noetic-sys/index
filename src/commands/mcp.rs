//! MCP command - run as an MCP server.

use anyhow::{Context, Result};
use clap::Args;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::local;

#[derive(Args)]
pub struct McpCmd;

impl McpCmd {
    pub async fn run(&self) -> Result<()> {
        // Logging to stderr (stdout is for MCP protocol)
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(false),
            )
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .init();

        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        local::mcp::run_local(&index_dir).await
    }
}
