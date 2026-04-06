//! Local mode - self-contained indexing without a server.
//!
//! Stores indices in the global index directory:
//! - `db.sqlite` - package metadata and chunk data
//! - `blobs/` - code chunks (content-addressed)
//! - `vectors/` - LanceDB vector tables
//!
//! Default location: `~/Library/Application Support/idx` (macOS), `~/.local/share/idx` (Linux)
//! Override with the `IDX_DIR` environment variable.

#![allow(dead_code)]

mod config;
mod db;
mod indexer;
pub mod mcp;
pub mod models;
mod search;
mod storage;
mod vector;

pub use config::LocalConfig;
pub use indexer::LocalIndexer;
pub use search::LocalSearch;

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Get the global index directory.
///
/// Resolution order:
/// 1. `$IDX_DIR` environment variable (for CI / testing isolation)
/// 2. Platform data dir: `~/Library/Application Support/idx` (macOS), `~/.local/share/idx` (Linux)
pub fn get_index_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("IDX_DIR") {
        return PathBuf::from(dir);
    }

    dirs::data_dir()
        .expect("Could not determine data directory. Set IDX_DIR to override.")
        .join("idx")
}

/// Derive a stable project ID from the canonical project root path.
///
/// Uses the first 16 bytes of SHA-256(canonical_path) — 32 hex chars, collision-resistant enough.
pub fn project_id(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let hash = Sha256::digest(canonical.to_string_lossy().as_bytes());
    hex::encode(&hash[..16])
}
