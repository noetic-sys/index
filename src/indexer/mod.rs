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

pub use chunk::CodeChunk;
pub use language::Language;
pub use languages::get_parser;
