//! Init command - index all dependencies from project manifests.

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
    Dependency, discover_manifest_dirs, parse_cargo_deps, parse_go_deps, parse_maven_deps,
    parse_npm_deps, parse_python_deps,
};

#[derive(Args)]
pub struct InitCmd {
    /// Directory to scan for manifests (default: current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Don't actually index, just show what would be indexed
    #[arg(long)]
    pub dry_run: bool,

    /// Show detailed per-package results
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// Number of packages to index concurrently
    #[arg(long, short = 'j', default_value = "4")]
    pub concurrency: usize,
}

impl InitCmd {
    pub async fn run(&self) -> Result<()> {
        // Check for API key first
        let config = local::LocalConfig::load()?;
        if !config.has_openai_key() {
            anyhow::bail!("OpenAI API key not configured. Run: idx config set-key <key>");
        }

        // Find or create .index/ directory
        let index_dir =
            local::get_index_dir().unwrap_or_else(|| self.path.join(local::INDEX_DIR_NAME));

        if !index_dir.exists() {
            std::fs::create_dir_all(&index_dir).context("Failed to create .index directory")?;
            println!("Created {}", index_dir.display());
        }

        println!("Index: {}", index_dir.display());
        println!("Scanning {} for dependencies...", self.path.display());

        let deps = self.collect_dependencies()?;

        if deps.is_empty() {
            println!("No dependencies found.");
            return Ok(());
        }

        println!("Found {} dependencies", deps.len());

        if self.dry_run {
            println!("\nDependencies:");
            for dep in &deps {
                println!("  {}:{}@{}", dep.registry, dep.name, dep.version);
            }
            println!("\n(dry run - not indexing)");
            return Ok(());
        }

        let indexer = Arc::new(LocalIndexer::new(&index_dir).await?);

        let indexed = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let total = deps.len();
        let verbose = self.verbose;
        let concurrency = self.concurrency.max(1);

        println!("Indexing with {} concurrent workers...", concurrency);

        stream::iter(deps.into_iter().map(|dep| {
            let indexer = Arc::clone(&indexer);
            let indexed = Arc::clone(&indexed);
            let skipped = Arc::clone(&skipped);
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
                        if result.chunks_indexed > 0 {
                            indexed.fetch_add(1, Ordering::Relaxed);
                            if verbose {
                                eprintln!(
                                    "  {}@{} -> indexed ({} chunks)",
                                    dep.name, dep.version, result.chunks_indexed
                                );
                            }
                        } else {
                            skipped.fetch_add(1, Ordering::Relaxed);
                            if verbose {
                                eprintln!("  {}@{} -> already indexed", dep.name, dep.version);
                            }
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

        // Clear the progress line
        if !verbose {
            print!("\r{}\r", " ".repeat(40));
        }

        let indexed = indexed.load(Ordering::Relaxed);
        let skipped = skipped.load(Ordering::Relaxed);
        let failed = failed.load(Ordering::Relaxed);

        println!("Results:");
        println!("  {} indexed", indexed);
        println!("  {} already indexed", skipped);
        if failed > 0 {
            println!("  {} failed", failed);
        }

        println!("\nDone!");

        Ok(())
    }

    fn collect_dependencies(&self) -> Result<Vec<Dependency>> {
        let mut all_deps = Vec::new();

        // Discover all manifest directories (handles monorepos)
        let manifest_dirs = discover_manifest_dirs(&self.path)?;

        if manifest_dirs.is_empty() {
            return Ok(vec![]);
        }

        // Show discovered roots if more than one
        if manifest_dirs.len() > 1 {
            println!("Found {} project roots:", manifest_dirs.len());
            for dir in &manifest_dirs {
                let rel = dir.strip_prefix(&self.path).unwrap_or(dir).display();
                let rel_str = rel.to_string();
                if rel_str.is_empty() {
                    println!("  .");
                } else {
                    println!("  {}", rel_str);
                }
            }
        }

        // Parse manifests from each discovered directory
        for dir in &manifest_dirs {
            if let Ok(deps) = parse_npm_deps(dir) {
                all_deps.extend(deps);
            }
            if let Ok(deps) = parse_cargo_deps(dir) {
                all_deps.extend(deps);
            }
            if let Ok(deps) = parse_python_deps(dir) {
                all_deps.extend(deps);
            }
            if let Ok(deps) = parse_maven_deps(dir) {
                all_deps.extend(deps);
            }
            if let Ok(deps) = parse_go_deps(dir) {
                all_deps.extend(deps);
            }
        }

        // Dedupe by (registry, name) - keep first occurrence
        let mut seen: HashMap<(String, String), usize> = HashMap::new();
        for (i, dep) in all_deps.iter().enumerate() {
            seen.entry((dep.registry.clone(), dep.name.clone()))
                .or_insert(i);
        }

        let mut indices: Vec<_> = seen.into_values().collect();
        indices.sort();

        let deps: Vec<_> = indices.into_iter().map(|i| all_deps[i].clone()).collect();

        Ok(deps)
    }
}
