//! Status command - show indexed vs manifest dependencies.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::local::models::VersionStatus;
use crate::local::{self, LocalIndexer};
use crate::manifests::{
    discover_manifest_dirs, parse_cargo_deps, parse_go_deps, parse_maven_deps, parse_npm_deps,
    parse_python_deps,
};

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

        // Get indexed versions
        let indexed_versions = indexer.db().list_versions().await?;
        let indexed_set: HashSet<(String, String, String)> = indexed_versions
            .iter()
            .filter(|v| v.status() == VersionStatus::Indexed)
            .map(|v| (v.registry.clone(), v.name.clone(), v.version.clone()))
            .collect();

        let failed_count = indexed_versions
            .iter()
            .filter(|v| v.status() == VersionStatus::Failed)
            .count();

        let skipped_count = indexed_versions
            .iter()
            .filter(|v| v.status() == VersionStatus::Skipped)
            .count();

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
        if failed_count > 0 {
            println!(
                "Failed:   {} packages (run `idx list -s failed` to see errors)",
                failed_count
            );
        }
        if skipped_count > 0 {
            println!("Skipped:  {} packages", skipped_count);
        }
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
