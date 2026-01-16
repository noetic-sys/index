//! Update command - re-index packages with changed versions.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::types::Registry;
use anyhow::{Context, Result};
use clap::Args;
use futures::stream::{self, StreamExt};

use crate::local::{self, LocalIndexer};
use crate::manifests::{
    Dependency, parse_cargo_deps, parse_go_deps, parse_maven_deps, parse_npm_deps,
    parse_python_deps,
};

#[derive(Args)]
pub struct UpdateCmd {
    /// Directory to scan for manifests (default: current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Number of packages to index concurrently
    #[arg(long, short = 'j', default_value = "4")]
    pub concurrency: usize,

    /// Show detailed output
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

impl UpdateCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let indexer = Arc::new(LocalIndexer::new(&index_dir).await?);

        // Get indexed packages: (registry, name) -> version
        let indexed_packages = indexer.db().list_packages().await?;
        let indexed_versions: HashMap<(String, String), String> = indexed_packages
            .iter()
            .map(|p| ((p.registry.clone(), p.name.clone()), p.version.clone()))
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
        if let Ok(deps) = parse_maven_deps(&self.path) {
            manifest_deps.extend(deps);
        }
        if let Ok(deps) = parse_go_deps(&self.path) {
            manifest_deps.extend(deps);
        }

        // Find packages that need updating (version changed or new)
        let mut to_update: Vec<Dependency> = Vec::new();
        for dep in manifest_deps {
            let key = (dep.registry.clone(), dep.name.clone());
            match indexed_versions.get(&key) {
                Some(indexed_version) if indexed_version == &dep.version => {
                    // Already indexed at correct version
                }
                Some(indexed_version) => {
                    // Version changed
                    if self.verbose {
                        println!(
                            "  {}@{} -> {} (version changed)",
                            dep.name, indexed_version, dep.version
                        );
                    }
                    to_update.push(dep);
                }
                None => {
                    // New package
                    if self.verbose {
                        println!("  {}@{} (new)", dep.name, dep.version);
                    }
                    to_update.push(dep);
                }
            }
        }

        if to_update.is_empty() {
            println!("All packages are up to date.");
            return Ok(());
        }

        println!("Found {} packages to update", to_update.len());

        let indexed = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let total = to_update.len();
        let verbose = self.verbose;
        let concurrency = self.concurrency.max(1);

        stream::iter(to_update.into_iter().map(|dep| {
            let indexer = Arc::clone(&indexer);
            let indexed = Arc::clone(&indexed);
            let failed = Arc::clone(&failed);
            let completed = Arc::clone(&completed);

            async move {
                let registry = match Registry::from_str(&dep.registry) {
                    Ok(r) => r,
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        if verbose {
                            eprintln!("  {} -> error: {}", dep.name, e);
                        }
                        return;
                    }
                };

                match indexer
                    .index_package(registry, &dep.name, &dep.version)
                    .await
                {
                    Ok(result) => {
                        indexed.fetch_add(1, Ordering::Relaxed);
                        if verbose {
                            eprintln!(
                                "  {}@{} -> indexed ({} chunks)",
                                dep.name, dep.version, result.chunks_indexed
                            );
                        }
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        if verbose {
                            eprintln!("  {}@{} -> failed: {}", dep.name, dep.version, e);
                        }
                    }
                }

                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if !verbose {
                    print!("\r[{}/{}] completed", done, total);
                    std::io::stdout().flush().ok();
                }
            }
        }))
        .buffer_unordered(concurrency)
        .collect::<Vec<()>>()
        .await;

        if !verbose {
            print!("\r{}\r", " ".repeat(40));
        }

        let indexed = indexed.load(Ordering::Relaxed);
        let failed = failed.load(Ordering::Relaxed);

        println!("Results:");
        println!("  {} indexed", indexed);
        if failed > 0 {
            println!("  {} failed", failed);
        }

        Ok(())
    }
}
