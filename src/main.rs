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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.command.execute().await
}
