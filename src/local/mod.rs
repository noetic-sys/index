//! Local mode - self-contained indexing without a server.
//!
//! Stores indices in `.index/` directory within the project:
//! - `db.sqlite` - package metadata and vector embeddings
//! - `blobs/` - code chunks (content-addressed)

#![allow(dead_code)]

mod config;
mod db;
mod indexer;
pub mod mcp;
mod models;
mod search;
mod storage;
mod vector;

pub use config::LocalConfig;
pub use indexer::LocalIndexer;
pub use search::LocalSearch;

use std::path::{Path, PathBuf};

/// The name of the index directory.
pub const INDEX_DIR_NAME: &str = ".index";

/// Find the `.index/` directory by walking up from the given path.
pub fn find_index_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let index_dir = current.join(INDEX_DIR_NAME);
        if index_dir.is_dir() {
            return Some(index_dir);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Check if we're in local mode (`.index/` exists in cwd or parents).
pub fn is_local_mode() -> bool {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| find_index_root(&cwd))
        .is_some()
}

/// Get the index directory for the current working directory.
pub fn get_index_dir() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| find_index_root(&cwd))
}
