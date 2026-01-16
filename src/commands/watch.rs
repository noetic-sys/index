//! Watch command - watch manifests and auto-reindex on changes.

use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;
use futures::stream::{self, StreamExt};
use crate::types::Registry;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::local::{self, LocalIndexer};
use crate::manifests::{parse_cargo_deps, parse_go_deps, parse_maven_deps, parse_npm_deps, parse_python_deps, Dependency};

#[derive(Args)]
pub struct WatchCmd {
    /// Directory to watch (default: current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Number of packages to index concurrently
    #[arg(long, short = 'j', default_value = "4")]
    pub concurrency: usize,
}

impl WatchCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir()
            .context("No .index directory found. Run `idx init` first.")?;

        println!("Watching {} for manifest changes...", self.path.display());
        println!("Press Ctrl+C to stop.\n");

        // Initial sync
        self.sync_index(&index_dir).await?;

        // Set up file watcher
        let (tx, mut rx) = mpsc::channel(100);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.blocking_send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        // Watch manifest files
        let manifests = [
            "package.json",
            "package-lock.json",
            "Cargo.toml",
            "Cargo.lock",
            "pyproject.toml",
            "requirements.txt",
        ];

        for manifest in manifests {
            let path = self.path.join(manifest);
            if path.exists() {
                watcher.watch(&path, RecursiveMode::NonRecursive).ok();
            }
        }

        // Also watch workspace members if Cargo workspace
        if let Ok(content) = std::fs::read_to_string(self.path.join("Cargo.toml")) {
            if let Ok(toml) = content.parse::<toml::Value>() {
                if let Some(workspace) = toml.get("workspace") {
                    if let Some(members) = workspace.get("members").and_then(|m| m.as_array()) {
                        for member in members {
                            if let Some(member_path) = member.as_str() {
                                let member_toml = self.path.join(member_path).join("Cargo.toml");
                                if member_toml.exists() {
                                    watcher.watch(&member_toml, RecursiveMode::NonRecursive).ok();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Debounce and process changes
        let mut last_sync = std::time::Instant::now();
        let debounce = Duration::from_secs(2);

        loop {
            tokio::select! {
                Some(_event) = rx.recv() => {
                    // Debounce: only sync if enough time has passed
                    if last_sync.elapsed() > debounce {
                        println!("\nManifest changed, syncing...");
                        if let Err(e) = self.sync_index(&index_dir).await {
                            eprintln!("Sync failed: {}", e);
                        }
                        last_sync = std::time::Instant::now();
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("\nStopping watch.");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn sync_index(&self, index_dir: &PathBuf) -> Result<()> {
        let indexer = Arc::new(LocalIndexer::new(index_dir).await?);

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
        if let Ok(deps) = parse_maven_deps(&self.path) {
            manifest_deps.extend(deps);
        }
        if let Ok(deps) = parse_go_deps(&self.path) {
            manifest_deps.extend(deps);
        }

        // Find new packages to index
        let to_index: Vec<Dependency> = manifest_deps
            .into_iter()
            .filter(|d| !indexed_set.contains(&(d.registry.clone(), d.name.clone(), d.version.clone())))
            .collect();

        if to_index.is_empty() {
            println!("All dependencies are indexed.");
            return Ok(());
        }

        println!("Indexing {} new packages...", to_index.len());

        let indexed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let failed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        stream::iter(to_index.into_iter().map(|dep| {
            let indexer = Arc::clone(&indexer);
            let indexed = Arc::clone(&indexed);
            let failed = Arc::clone(&failed);
            async move {
                let registry = match Registry::from_str(&dep.registry) {
                    Ok(r) => r,
                    Err(_) => {
                        failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                };
                match indexer.index_package(registry, &dep.name, &dep.version).await {
                    Ok(result) => {
                        println!("  {}@{} -> {} chunks", dep.name, dep.version, result.chunks_indexed);
                        indexed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("  {}@{} -> failed: {}", dep.name, dep.version, e);
                        failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }))
        .buffer_unordered(self.concurrency)
        .collect::<Vec<()>>()
        .await;

        let indexed = indexed.load(std::sync::atomic::Ordering::Relaxed);
        let failed = failed.load(std::sync::atomic::Ordering::Relaxed);
        println!("Synced: {} indexed, {} failed", indexed, failed);

        Ok(())
    }
}
