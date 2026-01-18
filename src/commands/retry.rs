//! Retry command - mark a failed/skipped package for retry.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::models::VersionStatus;
use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct RetryCmd {
    /// Package to retry (format: registry:name@version or name@version)
    /// Use --all to retry all failed packages
    pub package: Option<String>,

    /// Retry all failed packages
    #[arg(long)]
    pub all: bool,
}

impl RetryCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;

        if self.all {
            // Retry all failed packages
            let failed = indexer
                .db()
                .list_versions_by_status(VersionStatus::Failed)
                .await?;

            if failed.is_empty() {
                println!("No failed packages to retry.");
                return Ok(());
            }

            for ver in &failed {
                indexer.db().mark_version_pending(&ver.version_id).await?;
            }

            println!(
                "Marked {} packages for retry. Run `idx update` to reindex.",
                failed.len()
            );
        } else if let Some(ref spec) = self.package {
            // Retry specific package
            let (registry, name, version) = parse_package_spec(spec)?;

            let ver = indexer
                .db()
                .find_version(&registry, &name, &version)
                .await?
                .context(format!(
                    "Package not found: {}:{}@{}",
                    registry, name, version
                ))?;

            indexer.db().mark_version_pending(&ver.version_id).await?;

            println!(
                "Marked {}:{}@{} for retry. Run `idx update` to reindex.",
                registry, name, version
            );
        } else {
            anyhow::bail!("Specify a package to retry or use --all to retry all failed packages.");
        }

        Ok(())
    }
}

/// Parse package spec: "crates:tokio@1.0.0" or "tokio@1.0.0" (assumes crates)
fn parse_package_spec(spec: &str) -> Result<(String, String, String)> {
    let (registry, rest) = if spec.contains(':') {
        let parts: Vec<_> = spec.splitn(2, ':').collect();
        (parts[0].to_string(), parts[1])
    } else {
        ("crates".to_string(), spec)
    };

    let (name, version) = rest
        .rsplit_once('@')
        .context("Invalid package spec. Use: name@version or registry:name@version")?;

    Ok((registry, name.to_string(), version.to_string()))
}
