//! CLI command implementations.

mod clean;
mod config;
mod index;
mod init;
mod list;
mod mcp;
mod prune;
mod remove;
mod search;
mod stats;
mod status;
mod update;
mod watch;

pub use clean::CleanCmd;
pub use config::ConfigCmd;
pub use index::IndexCmd;
pub use init::InitCmd;
pub use list::ListCmd;
pub use mcp::McpCmd;
pub use prune::PruneCmd;
pub use remove::RemoveCmd;
pub use search::SearchCmd;
pub use stats::StatsCmd;
pub use status::StatusCmd;
pub use update::UpdateCmd;
pub use watch::WatchCmd;
