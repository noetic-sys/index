//! CLI argument definitions.

use clap::{Parser, Subcommand};

use crate::commands::{
    CleanCmd, ConfigCmd, IndexCmd, InitCmd, ListCmd, McpCmd, PruneCmd, RemoveCmd, SearchCmd,
    StatsCmd, StatusCmd, UpdateCmd, WatchCmd,
};

#[derive(Parser)]
#[command(name = "idx")]
#[command(about = "Index - semantic code search for your dependencies")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize index and scan dependencies
    Init(InitCmd),

    /// Update index with changed/new dependencies
    Update(UpdateCmd),

    /// Watch manifests and auto-reindex on changes
    Watch(WatchCmd),

    /// Index a specific package
    Index(IndexCmd),

    /// Search for code in indexed packages
    Search(SearchCmd),

    /// List all indexed packages
    List(ListCmd),

    /// Show index statistics
    Stats(StatsCmd),

    /// Show index status vs manifest dependencies
    Status(StatusCmd),

    /// Remove a package from the index
    Remove(RemoveCmd),

    /// Remove packages no longer in manifests
    Prune(PruneCmd),

    /// Delete the entire .index directory
    Clean(CleanCmd),

    /// Run as MCP server (for AI tools)
    Mcp(McpCmd),

    /// Manage configuration (API keys, etc.)
    Config(ConfigCmd),
}

impl Command {
    pub async fn execute(&self) -> anyhow::Result<()> {
        match self {
            Command::Init(cmd) => cmd.run().await,
            Command::Update(cmd) => cmd.run().await,
            Command::Watch(cmd) => cmd.run().await,
            Command::Index(cmd) => cmd.run().await,
            Command::Search(cmd) => cmd.run().await,
            Command::List(cmd) => cmd.run().await,
            Command::Stats(cmd) => cmd.run().await,
            Command::Status(cmd) => cmd.run().await,
            Command::Remove(cmd) => cmd.run().await,
            Command::Prune(cmd) => cmd.run().await,
            Command::Clean(cmd) => cmd.run().await,
            Command::Mcp(cmd) => cmd.run().await,
            Command::Config(cmd) => cmd.run().await,
        }
    }
}
