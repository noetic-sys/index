//! List command - list all indexed packages.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct ListCmd {
    /// Filter by registry (npm, pypi, crates)
    #[arg(long, short = 'r')]
    pub registry: Option<String>,

    /// Show only package names (no versions)
    #[arg(long)]
    pub names_only: bool,
}

impl ListCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;
        let packages = indexer.db().list_packages().await?;

        if packages.is_empty() {
            println!("No packages indexed yet. Run `idx init` to index your dependencies.");
            return Ok(());
        }

        let filtered: Vec<_> = if let Some(ref reg) = self.registry {
            packages
                .into_iter()
                .filter(|p| &p.registry == reg)
                .collect()
        } else {
            packages
        };

        if filtered.is_empty() {
            println!(
                "No packages found for registry '{}'.",
                self.registry.as_ref().unwrap()
            );
            return Ok(());
        }

        for pkg in &filtered {
            if self.names_only {
                println!("{}", pkg.name);
            } else {
                println!("{}:{}@{}", pkg.registry, pkg.name, pkg.version);
            }
        }

        if !self.names_only {
            println!("\n{} packages", filtered.len());
        }

        Ok(())
    }
}
