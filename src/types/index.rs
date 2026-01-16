use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Language, PackageMetadata, TenantId};

pub type JobId = Uuid;

/// Request to index a private package.
///
/// Triggered via:
/// - CLI: `index index --name my-package --version 1.0.0`
/// - GitHub Action
/// - Direct API call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRequest {
    pub package_name: String,
    pub version: String,
    pub source: IndexSource,
    pub language: Option<Language>,
    pub metadata: Option<PackageMetadata>,
}

/// Where to get the package source code from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IndexSource {
    /// Direct upload as base64-encoded tarball (for small packages)
    Tarball { content_base64: String },

    /// Clone from git reference (for GitHub App / CI integrations)
    GitRef {
        repo_url: String,
        #[serde(rename = "ref")]
        git_ref: String,
        /// Subdirectory to index (for monorepos)
        path: Option<String>,
    },

    /// Upload via presigned URL (for large packages)
    PresignedUpload { upload_id: String },
}

/// An indexing job tracked in Postgres.
///
/// # Lifecycle
/// 1. Created with status `Queued` when request received
/// 2. Worker picks up, transitions through `Downloading` → `Parsing` → `Embedding` → `Storing`
/// 3. Ends in `Completed` or `Failed`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexJob {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub package_name: String,
    pub version: String,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub stats: Option<IndexStats>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Downloading,
    Parsing,
    Embedding,
    Storing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub files_processed: u32,
    pub chunks_indexed: u32,
    pub duration_ms: u64,
}
