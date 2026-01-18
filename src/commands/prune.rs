//! Prune command - remove packages no longer in manifests.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};
use crate::manifests::{
    discover_manifest_dirs, parse_cargo_deps, parse_go_deps, parse_maven_deps, parse_npm_deps,
    parse_python_deps,
};

#[derive(Args)]
pub struct PruneCmd {
    /// Directory to scan for manifests (default: current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Only show what would be removed (dry run)
    #[arg(long)]
    pub dry_run: bool,
}

impl PruneCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;

        // Get indexed versions
        let indexed_versions = indexer.db().list_versions().await?;

        // Get manifest dependencies from all discovered roots
        let manifest_dirs = discover_manifest_dirs(&self.path)?;
        let mut manifest_deps = Vec::new();

        for dir in &manifest_dirs {
            if let Ok(deps) = parse_npm_deps(dir) {
                manifest_deps.extend(deps);
            }
            if let Ok(deps) = parse_cargo_deps(dir) {
                manifest_deps.extend(deps);
            }
            if let Ok(deps) = parse_python_deps(dir) {
                manifest_deps.extend(deps);
            }
            if let Ok(deps) = parse_maven_deps(dir) {
                manifest_deps.extend(deps);
            }
            if let Ok(deps) = parse_go_deps(dir) {
                manifest_deps.extend(deps);
            }
        }

        let manifest_set: HashSet<(String, String)> = manifest_deps
            .iter()
            .map(|d| (d.registry.clone(), d.name.clone()))
            .collect();

        // Find versions to prune (indexed but not in manifest)
        let to_prune: Vec<_> = indexed_versions
            .iter()
            .filter(|v| !manifest_set.contains(&(v.registry.clone(), v.name.clone())))
            .collect();

        if to_prune.is_empty() {
            println!("Nothing to prune. All indexed packages are in manifests.");
            return Ok(());
        }

        println!("Packages to remove ({}):", to_prune.len());
        for ver in &to_prune {
            println!("  {}:{}@{}", ver.registry, ver.name, ver.version);
        }

        if self.dry_run {
            println!("\n(dry run - nothing removed)");
            return Ok(());
        }

        if !self.yes {
            print!("\nRemove these packages? [y/N] ");
            std::io::Write::flush(&mut std::io::stdout())?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        }

        // Remove versions
        let mut removed = 0;
        for ver in &to_prune {
            // Delete from db
            let namespaces = indexer.db().delete_version(&ver.version_id).await?;

            // Delete from vector store
            for ns in &namespaces {
                indexer.vectors().delete_namespace(ns).await?;
            }

            // Delete from blob storage
            indexer
                .storage()
                .delete_package(&ver.registry, &ver.name, &ver.version)
                .await?;

            removed += 1;
        }

        println!("\nRemoved {} packages.", removed);

        Ok(())
    }
}
