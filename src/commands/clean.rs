//! Clean command - remove packages registered by the current project.
//!
//! Uses ref-counting: a package version is only deleted from the global store
//! when no other project references it.

use anyhow::Result;
use clap::Args;

use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct CleanCmd {
    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Directory to use as the project root (default: current directory)
    #[arg(default_value = ".")]
    pub path: std::path::PathBuf,
}

impl CleanCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir();
        let project_id = local::project_id(
            &self.path.canonicalize().unwrap_or_else(|_| self.path.clone()),
        );

        let indexer = LocalIndexer::new(&index_dir).await?;
        let db = indexer.db();

        let packages = db.list_project_packages(&project_id).await?;

        if packages.is_empty() {
            println!("No packages registered for this project.");
            return Ok(());
        }

        // Determine what will be deleted vs kept
        let mut to_delete = Vec::new();
        let mut to_keep = Vec::new();

        for (registry, name, version) in &packages {
            let refs = db.version_ref_count(registry, name, version).await?;
            if refs <= 1 {
                to_delete.push((registry.clone(), name.clone(), version.clone()));
            } else {
                to_keep.push((registry.clone(), name.clone(), version.clone()));
            }
        }

        println!(
            "This project has {} registered packages.",
            packages.len()
        );
        if !to_delete.is_empty() {
            println!(
                "  {} will be removed from the global store (no other projects use them)",
                to_delete.len()
            );
        }
        if !to_keep.is_empty() {
            println!(
                "  {} will be kept (referenced by other projects)",
                to_keep.len()
            );
        }

        if !self.yes {
            print!("Continue? [y/N] ");
            std::io::Write::flush(&mut std::io::stdout())?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        }

        // Delete packages with zero remaining references
        let storage = indexer.storage();
        let vectors = indexer.vectors();

        for (registry, name, version) in &to_delete {
            // Remove blobs
            storage.delete_package(registry, name, version).await?;

            // Remove from SQLite (cascades to chunks)
            if let Some(pkg) = db.find_package(registry, name).await?
                && let Some(ver) = db.find_version_by_package(&pkg.id, version).await?
            {
                let namespaces = db.delete_version(&ver.id).await?;
                for ns in &namespaces {
                    let _ = vectors.delete_namespace(ns).await;
                }
            }
        }

        // Unregister this project
        db.unregister_project(&project_id).await?;

        println!(
            "Done. Removed {} packages, kept {} (still used by other projects).",
            to_delete.len(),
            to_keep.len()
        );

        Ok(())
    }
}
