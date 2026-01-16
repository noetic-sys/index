//! Code indexer using tree-sitter for parsing.
//!
//! This crate provides:
//! - Language-agnostic parsing via tree-sitter
//! - Per-language implementations for extracting code chunks + docstrings
//! - Chunking strategy for semantic search
//! - Workspace detection for monorepos

mod chunk;
mod error;
mod language;
mod languages;
pub mod workspace;

pub use chunk::{ChunkBuilder, CodeChunk};
pub use error::IndexerError;
pub use language::{Language, LanguageParser};
pub use languages::get_parser;
pub use workspace::{DetectedPackage, analyze_repo};
