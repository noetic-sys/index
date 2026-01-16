//! Skip command - mark a package version as skipped.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct SkipCmd {
    /// Package to skip (format: registry:name@version or name@version)
    pub package: String,
}

impl SkipCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;

        // Parse package spec
        let (registry, name, version) = parse_package_spec(&self.package)?;

        // Find the version
        let ver = indexer
            .db()
            .find_version(&registry, &name, &version)
            .await?
            .context(format!(
                "Package not found: {}:{}@{}",
                registry, name, version
            ))?;

        // Mark as skipped
        indexer.db().mark_version_skipped(&ver.version_id).await?;

        println!("Skipped {}:{}@{}", registry, name, version);

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
