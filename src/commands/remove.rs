//! Remove command - remove a specific package from the index.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct RemoveCmd {
    /// Package to remove (format: registry:name@version or name@version)
    pub package: String,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

impl RemoveCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;

        // Parse package spec
        let (registry, name, version) = parse_package_spec(&self.package)?;

        // Find the package
        let package = indexer
            .db()
            .find_package(&registry, &name, &version)
            .await?
            .context(format!(
                "Package not found: {}:{}@{}",
                registry, name, version
            ))?;

        if !self.yes {
            println!("Remove {}:{}@{}?", registry, name, version);
            print!("[y/N] ");
            std::io::Write::flush(&mut std::io::stdout())?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        }

        // Delete from db (returns namespaces)
        let namespaces = indexer.db().delete_package(&package.id).await?;

        // Delete from vector store
        for ns in &namespaces {
            indexer.vectors().delete_namespace(ns).await?;
        }

        // Delete from blob storage
        indexer
            .storage()
            .delete_package(&registry, &name, &version)
            .await?;

        println!("Removed {}:{}@{}", registry, name, version);

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
