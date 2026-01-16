//! List command - list all indexed packages.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};
use crate::local::models::VersionStatus;

#[derive(Args)]
pub struct ListCmd {
    /// Filter by registry (npm, pypi, crates)
    #[arg(long, short = 'r')]
    pub registry: Option<String>,

    /// Filter by status (indexed, failed, skipped, pending)
    #[arg(long, short = 's')]
    pub status: Option<String>,

    /// Show only package names (no versions)
    #[arg(long)]
    pub names_only: bool,
}

impl ListCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;

        // Get versions (optionally filtered by status)
        let versions = if let Some(ref status_str) = self.status {
            let status: VersionStatus = status_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid status: {}. Use: indexed, failed, skipped, pending", status_str))?;
            indexer.db().list_versions_by_status(status).await?
        } else {
            indexer.db().list_versions().await?
        };

        if versions.is_empty() {
            if self.status.is_some() {
                println!("No packages with status '{}'.", self.status.as_ref().unwrap());
            } else {
                println!("No packages indexed yet. Run `idx init` to index your dependencies.");
            }
            return Ok(());
        }

        let filtered: Vec<_> = if let Some(ref reg) = self.registry {
            versions
                .into_iter()
                .filter(|v| &v.registry == reg)
                .collect()
        } else {
            versions
        };

        if filtered.is_empty() {
            println!(
                "No packages found for registry '{}'.",
                self.registry.as_ref().unwrap()
            );
            return Ok(());
        }

        for ver in &filtered {
            if self.names_only {
                println!("{}", ver.name);
            } else {
                let status = ver.status();
                let status_str = match status {
                    VersionStatus::Indexed => "",
                    VersionStatus::Failed => " [failed]",
                    VersionStatus::Skipped => " [skipped]",
                    VersionStatus::Pending => " [pending]",
                };
                println!("{}:{}@{}{}", ver.registry, ver.name, ver.version, status_str);
            }
        }

        if !self.names_only {
            println!("\n{} packages", filtered.len());
        }

        Ok(())
    }
}
