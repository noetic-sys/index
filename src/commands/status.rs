//! Status command - show indexed vs manifest dependencies.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::local::{self, LocalIndexer};
use crate::manifests::{parse_cargo_deps, parse_npm_deps, parse_python_deps};

#[derive(Args)]
pub struct StatusCmd {
    /// Directory to scan for manifests (default: current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

impl StatusCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir();

        if index_dir.is_none() {
            println!("No .index directory found. Run `idx init` first.");
            return Ok(());
        }

        let index_dir = index_dir.unwrap();
        let indexer = LocalIndexer::new(&index_dir).await?;

        // Get indexed packages
        let indexed_packages = indexer.db().list_packages().await?;
        let indexed_set: HashSet<(String, String, String)> = indexed_packages
            .iter()
            .map(|p| (p.registry.clone(), p.name.clone(), p.version.clone()))
            .collect();

        // Get manifest dependencies
        let mut manifest_deps = Vec::new();
        if let Ok(deps) = parse_npm_deps(&self.path) {
            manifest_deps.extend(deps);
        }
        if let Ok(deps) = parse_cargo_deps(&self.path) {
            manifest_deps.extend(deps);
        }
        if let Ok(deps) = parse_python_deps(&self.path) {
            manifest_deps.extend(deps);
        }

        let manifest_set: HashSet<(String, String, String)> = manifest_deps
            .iter()
            .map(|d| (d.registry.clone(), d.name.clone(), d.version.clone()))
            .collect();

        // Find gaps
        let missing: Vec<_> = manifest_set.difference(&indexed_set).collect();

        let extra: Vec<_> = indexed_set.difference(&manifest_set).collect();

        // Print status
        println!("Index: {}", index_dir.display());
        println!();
        println!("Indexed:  {} packages", indexed_set.len());
        println!("Manifest: {} dependencies", manifest_set.len());
        println!();

        if missing.is_empty() {
            println!("All dependencies are indexed.");
        } else {
            println!("Missing ({}):", missing.len());
            for (registry, name, version) in &missing {
                println!("  {}:{}@{}", registry, name, version);
            }
        }

        if !extra.is_empty() {
            println!();
            println!("Extra (indexed but not in manifest) ({}):", extra.len());
            for (registry, name, version) in &extra {
                println!("  {}:{}@{}", registry, name, version);
            }
        }

        Ok(())
    }
}
