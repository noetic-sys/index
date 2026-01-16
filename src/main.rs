//! Index CLI - semantic code search for your dependencies.

mod cli;
mod commands;
mod indexer;
mod local;
mod manifests;
mod registry;
mod types;

use anyhow::Result;
use cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.command.execute().await
}
