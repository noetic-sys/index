//! Data models for local storage.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// Version Status
// ============================================================================

/// Indexing status for a package version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus {
    /// Currently being indexed
    #[default]
    Pending,
    /// Successfully indexed
    Indexed,
    /// Indexing failed
    Failed,
    /// User explicitly skipped
    Skipped,
}

impl std::fmt::Display for VersionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Indexed => write!(f, "indexed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

impl std::str::FromStr for VersionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "indexed" => Ok(Self::Indexed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(format!("invalid version status: {}", s)),
        }
    }
}

// ============================================================================
// Package Models
// ============================================================================

/// A package row from the local index (unique by registry + name).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PackageRow {
    pub id: String,
    pub registry: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

/// Input for creating a package.
#[derive(Debug, Clone)]
pub struct CreatePackage {
    pub registry: String,
    pub name: String,
    pub description: Option<String>,
}

// ============================================================================
// Version Models
// ============================================================================

/// A package version row from the local index.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct VersionRow {
    pub id: String,
    pub package_id: String,
    pub version: String,
    pub status: String, // stored as text, convert with VersionStatus
    pub error_message: Option<String>,
    pub chunk_count: i32,
    pub indexed_at: Option<String>,
    pub created_at: String,
}

impl VersionRow {
    pub fn status(&self) -> VersionStatus {
        self.status.parse().unwrap_or_default()
    }
}

/// Version with joined package info.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct VersionWithPackage {
    // Version fields
    pub version_id: String,
    pub version: String,
    pub status: String,
    pub error_message: Option<String>,
    pub chunk_count: i32,
    pub indexed_at: Option<String>,
    // Package fields
    pub package_id: String,
    pub registry: String,
    pub name: String,
    pub description: Option<String>,
}

impl VersionWithPackage {
    pub fn status(&self) -> VersionStatus {
        self.status.parse().unwrap_or_default()
    }

    /// Build the namespace string for this version.
    pub fn namespace(&self) -> String {
        format!("{}/{}/{}", self.registry, self.name, self.version)
    }
}

/// Input for creating a version.
#[derive(Debug, Clone)]
pub struct CreateVersion {
    pub package_id: String,
    pub version: String,
}

// ============================================================================
// Chunk Models
// ============================================================================

/// A code chunk row from the local index.
#[derive(Debug, Clone, FromRow)]
pub struct ChunkRow {
    pub id: String,
    pub version_id: String,
    pub namespace: String,
    pub chunk_type: String,
    pub name: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub visibility: String,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub snippet: String,
    pub storage_key: String,
    pub content_hash: String,
    pub vector: Vec<u8>,
}

/// Chunk with joined package/version info.
#[derive(Debug, Clone, FromRow)]
pub struct ChunkWithPackage {
    pub id: String,
    pub namespace: String,
    pub chunk_type: String,
    pub name: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub visibility: String,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub snippet: String,
    pub storage_key: String,
    pub registry: String,
    pub package_name: String,
    pub version: String,
}

/// Input for inserting a new chunk.
#[derive(Debug, Clone)]
pub struct CreateChunk {
    pub id: String,
    pub version_id: String,
    pub namespace: String,
    pub chunk_type: String,
    pub name: String,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub visibility: String,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub snippet: String,
    pub storage_key: String,
    pub content_hash: String,
    pub vector: Vec<f32>,
}

/// Existing chunk info for deduplication.
#[derive(Debug, Clone)]
pub struct ExistingChunk {
    pub content_hash: String,
    pub vector: Vec<f32>,
}

// ============================================================================
// Vector Models
// ============================================================================

/// Vector dimension for embeddings (text-embedding-3-small).
pub const VECTOR_DIM: i32 = 1536;

/// A record to insert into the vector store.
#[derive(Debug, Clone)]
pub struct VectorRecord {
    pub chunk_id: String,
    pub content_hash: String,
    pub vector: Vec<f32>,
}

/// A search hit from vector similarity search.
#[derive(Debug, Clone)]
pub struct VectorSearchHit {
    pub chunk_id: String,
    /// L2 distance (lower = more similar)
    pub distance: f32,
}

impl VectorSearchHit {
    /// Convert distance to similarity score (0-1, higher = more similar).
    pub fn score(&self) -> f32 {
        1.0 / (1.0 + self.distance)
    }
}

// ============================================================================
// Stats Models
// ============================================================================

/// Index statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub package_count: u32,
    pub version_count: u32,
    pub indexed_count: u32,
    pub failed_count: u32,
    pub skipped_count: u32,
    pub chunk_count: u32,
}

// ============================================================================
// Search Models
// ============================================================================

/// Search result returned from search (combines vector hit with chunk data).
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub registry: String,
    pub package: String,
    pub version: String,
    pub chunk_type: String,
    pub name: String,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub visibility: String,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub snippet: String,
    pub storage_key: String,
    pub score: f32,
}

/// Convert f32 vector to bytes for SQLite storage.
pub fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert bytes back to f32 vector.
pub fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_round_trip() {
        let original = vec![0.1_f32, 0.2, 0.3, -0.5, 1.0];
        let bytes = vector_to_bytes(&original);
        let recovered = bytes_to_vector(&bytes);

        assert_eq!(original.len(), recovered.len());
        for (a, b) in original.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_version_status() {
        assert_eq!(VersionStatus::Pending.to_string(), "pending");
        assert_eq!("indexed".parse::<VersionStatus>(), Ok(VersionStatus::Indexed));
        assert!("invalid".parse::<VersionStatus>().is_err());
    }
}
