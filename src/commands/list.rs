//! List command - list all indexed packages.

use anyhow::Result;
use clap::Args;

use crate::local::models::VersionStatus;
use crate::local::{self, LocalIndexer};

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

    /// List the entire global index instead of scoping to this project's dependencies
    #[arg(long)]
    pub global: bool,
}

impl ListCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir();
        let indexer = LocalIndexer::new(&index_dir).await?;

        let project_id = if self.global {
            None
        } else {
            let cwd = std::env::current_dir()?;
            Some(local::project_id(&cwd))
        };

        // Get versions (optionally filtered by status)
        let versions = if let Some(ref status_str) = self.status {
            let status: VersionStatus = status_str.parse().map_err(|_| {
                anyhow::anyhow!(
                    "Invalid status: {}. Use: indexed, failed, skipped, pending",
                    status_str
                )
            })?;
            indexer.db().list_versions_by_status(status).await?
        } else {
            indexer.db().list_versions().await?
        };

        // Scope to this project's registered packages unless --global
        let versions = if let Some(ref pid) = project_id {
            let project_deps = indexer.db().list_project_packages(pid).await?;
            if project_deps.is_empty() {
                anyhow::bail!(
                    "No packages indexed for this project. Run `idx init` first, or use --global."
                );
            }
            versions
                .into_iter()
                .filter(|v| {
                    project_deps
                        .iter()
                        .any(|(r, n, _)| r == &v.registry && n == &v.name)
                })
                .collect()
        } else {
            versions
        };

        if versions.is_empty() {
            if let Some(status) = &self.status {
                println!("No packages with status '{}'.", status);
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
                println!(
                    "{}:{}@{}{}",
                    ver.registry, ver.name, ver.version, status_str
                );

                // Show error message for failed packages
                if status == VersionStatus::Failed
                    && let Some(ref err) = ver.error_message
                {
                    println!("  └─ {}", err);
                }
            }
        }

        if !self.names_only {
            println!("\n{} packages", filtered.len());
        }

        Ok(())
    }
}
