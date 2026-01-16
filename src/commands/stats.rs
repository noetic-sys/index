//! Stats command - show index statistics.

use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct StatsCmd;

impl StatsCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir()
            .context("No .index directory found. Run `idx init` first.")?;

        let indexer = LocalIndexer::new(&index_dir).await?;

        // Get package count
        let packages = indexer.db().list_packages().await?;

        // Get chunk count
        let namespaces = indexer.db().get_namespaces().await?;
        let mut total_chunks = 0;
        for ns in &namespaces {
            let chunks = indexer.db().get_chunks_by_namespace(ns).await?;
            total_chunks += chunks.len();
        }

        // Get storage sizes
        let db_size = get_file_size(&index_dir.join("db.sqlite"));
        let vectors_size = get_dir_size(&index_dir.join("vectors"));
        let blobs_size = get_dir_size(&index_dir.join("blobs"));
        let total_size = db_size + vectors_size + blobs_size;

        // Count by registry
        let mut crates_count = 0;
        let mut npm_count = 0;
        let mut pypi_count = 0;
        for pkg in &packages {
            match pkg.registry.as_str() {
                "crates" => crates_count += 1,
                "npm" => npm_count += 1,
                "pypi" => pypi_count += 1,
                _ => {}
            }
        }

        println!("Index: {}", index_dir.display());
        println!();
        println!("Packages:    {}", packages.len());
        if crates_count > 0 {
            println!("  crates:    {}", crates_count);
        }
        if npm_count > 0 {
            println!("  npm:       {}", npm_count);
        }
        if pypi_count > 0 {
            println!("  pypi:      {}", pypi_count);
        }
        println!();
        println!("Chunks:      {}", total_chunks);
        println!("Namespaces:  {}", namespaces.len());
        println!();
        println!("Storage:");
        println!("  Database:  {}", format_size(db_size));
        println!("  Vectors:   {}", format_size(vectors_size));
        println!("  Blobs:     {}", format_size(blobs_size));
        println!("  Total:     {}", format_size(total_size));

        Ok(())
    }
}

fn get_file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn get_dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    walkdir(path)
}

fn walkdir(path: &Path) -> u64 {
    let mut size = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                size += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            } else if path.is_dir() {
                size += walkdir(&path);
            }
        }
    }
    size
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
