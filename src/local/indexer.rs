//! Local indexing service.
//!
//! Orchestrates:
//! 1. Downloading packages from registries
//! 2. Parsing code with tree-sitter
//! 3. Generating embeddings
//! 4. Storing vectors and blobs locally

use std::path::Path;

use crate::indexer::{CodeChunk, Language, get_parser};
use crate::registry::{PackageFile, RegistryClients};
use crate::types::Registry;
use anyhow::{Context, Result};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use tracing::info;
use uuid::Uuid;

use super::LocalConfig;
use super::db::LocalDb;
use super::models::{CreateChunk, CreatePackage, VectorRecord};
use super::storage::LocalStorage;
use super::vector::VectorStore;

/// Local indexer service.
pub struct LocalIndexer {
    db: LocalDb,
    storage: LocalStorage,
    vectors: VectorStore,
    config: LocalConfig,
}

/// Result of indexing a package version.
#[derive(Debug)]
pub struct IndexResult {
    pub version_id: String,
    pub chunks_indexed: usize,
    pub files_processed: usize,
    /// True if this version was already indexed/skipped
    pub skipped: bool,
}

impl LocalIndexer {
    /// Create a new local indexer.
    pub async fn new(index_dir: &Path) -> Result<Self> {
        let db = LocalDb::open(&index_dir.join("db.sqlite")).await?;
        let storage = LocalStorage::new(index_dir.join("blobs")).await?;
        let vectors = VectorStore::open(&index_dir.join("vectors")).await?;
        let config = LocalConfig::load()?;

        Ok(Self {
            db,
            storage,
            vectors,
            config,
        })
    }

    /// Index a package from a registry.
    pub async fn index_package(
        &self,
        registry: Registry,
        name: &str,
        version: &str,
    ) -> Result<IndexResult> {
        info!(registry = %registry, name, version, "indexing package");

        // Get or create package
        let package_id = self
            .db
            .get_or_create_package(&CreatePackage {
                registry: registry.as_str().to_string(),
                name: name.to_string(),
                description: None, // Will be updated after download
            })
            .await?;

        // Get or create version
        let (version_id, should_skip) = self.db.get_or_create_version(&package_id, version).await?;

        if should_skip {
            info!("version already indexed or skipped");
            return Ok(IndexResult {
                version_id,
                chunks_indexed: 0,
                files_processed: 0,
                skipped: true,
            });
        }

        // Download package
        info!("downloading package");
        let client = RegistryClients::new(registry);

        let (_pkg_info, files) = match async {
            let pkg_info = client.get_version(name, version).await?;
            let files = client.download_source(name, version).await?;
            Ok::<_, anyhow::Error>((pkg_info, files))
        }
        .await
        {
            Ok(result) => result,
            Err(e) => {
                // Mark as failed
                self.db
                    .mark_version_failed(&version_id, &e.to_string())
                    .await?;
                return Err(e);
            }
        };

        // Parse files
        info!(files = files.len(), "parsing files");
        let chunks = self.parse_files(&files)?;

        if chunks.is_empty() {
            info!("no chunks extracted");
            // Mark as indexed with 0 chunks (valid state - package has no indexable code)
            self.db.mark_version_indexed(&version_id, 0).await?;
            return Ok(IndexResult {
                version_id,
                chunks_indexed: 0,
                files_processed: files.len(),
                skipped: false,
            });
        }

        // Generate embeddings
        info!(chunks = chunks.len(), "generating embeddings");
        let embeddings = match self.generate_embeddings(&chunks).await {
            Ok(e) => e,
            Err(e) => {
                self.db
                    .mark_version_failed(&version_id, &e.to_string())
                    .await?;
                return Err(e);
            }
        };

        // Build namespace
        let namespace = format!("{}/{}/{}", registry.as_str(), name, version);

        // Store everything
        info!("storing chunks");
        let mut vector_records = Vec::new();
        let mut db_chunks = Vec::new();

        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            let chunk_id = Uuid::new_v4().to_string();
            let content_hash = hex::encode(Sha256::digest(chunk.code.as_bytes()));

            // Store blob
            let storage_key = self
                .storage
                .put(registry.as_str(), name, version, chunk.code.as_bytes())
                .await?;

            // Prepare vector record
            vector_records.push(VectorRecord {
                chunk_id: chunk_id.clone(),
                content_hash: content_hash.clone(),
                vector: embedding.clone(),
            });

            // Prepare DB record
            db_chunks.push(CreateChunk {
                id: chunk_id,
                version_id: version_id.clone(),
                namespace: namespace.clone(),
                chunk_type: format!("{:?}", chunk.chunk_type).to_lowercase(),
                name: chunk.name.clone(),
                file_path: chunk.file_path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                visibility: format!("{:?}", chunk.visibility).to_lowercase(),
                signature: chunk.signature.clone(),
                docstring: chunk.documentation.clone(),
                snippet: chunk.snippet(500),
                storage_key,
                content_hash,
                vector: embedding.clone(),
            });
        }

        // Insert into vector store
        if let Err(e) = self.vectors.insert(&namespace, vector_records).await {
            // Include full error chain
            let error_msg = format!("{:#}", e);
            self.db.mark_version_failed(&version_id, &error_msg).await?;
            return Err(e);
        }

        // Insert into SQLite
        if let Err(e) = self.db.insert_chunks(&db_chunks).await {
            self.db
                .mark_version_failed(&version_id, &e.to_string())
                .await?;
            return Err(e);
        }

        let chunks_indexed = db_chunks.len();

        // Mark as successfully indexed
        self.db
            .mark_version_indexed(&version_id, chunks_indexed as i32)
            .await?;

        info!(chunks_indexed, "indexing complete");

        Ok(IndexResult {
            version_id,
            chunks_indexed,
            files_processed: files.len(),
            skipped: false,
        })
    }

    /// Parse files into code chunks.
    fn parse_files(&self, files: &[PackageFile]) -> Result<Vec<CodeChunk>> {
        let chunks: Vec<_> = files
            .par_iter()
            .filter(|f| !self.should_skip(&f.path))
            .filter_map(|f| {
                let language = Language::from_path(&f.path)?;
                let parser = get_parser(language).ok()?;
                parser.parse(&f.content, &f.path).ok()
            })
            .flatten()
            .collect();

        Ok(chunks)
    }

    /// Check if a file should be skipped.
    fn should_skip(&self, path: &str) -> bool {
        let path_lower = path.to_lowercase();

        const SKIP_DIRS: &[&str] = &[
            "node_modules/",
            "__pycache__/",
            ".git/",
            "target/",
            "dist/",
            "build/",
            ".next/",
            "coverage/",
        ];

        const SKIP_PATTERNS: &[&str] = &[".min.js", ".bundle.js", ".map", ".d.ts", ".lock", ".env"];

        for dir in SKIP_DIRS {
            if path_lower.contains(dir) {
                return true;
            }
        }

        for pattern in SKIP_PATTERNS {
            if path_lower.ends_with(pattern) {
                return true;
            }
        }

        false
    }

    /// Generate embeddings for chunks.
    async fn generate_embeddings(&self, chunks: &[CodeChunk]) -> Result<Vec<Vec<f32>>> {
        let api_key = self
            .config
            .openai_api_key
            .as_ref()
            .context("OpenAI API key not configured. Run: idx config set-key")?;

        let client = reqwest::Client::new();
        let mut all_embeddings = Vec::with_capacity(chunks.len());

        // Batch embeddings (max 100 per request)
        const BATCH_SIZE: usize = 100;

        info!(total_chunks = chunks.len(), "generating embeddings");

        for (batch_idx, batch) in chunks.chunks(BATCH_SIZE).enumerate() {
            let texts: Vec<String> = batch.iter().map(|c| c.embedding_text()).collect();
            let total_chars: usize = texts.iter().map(|t| t.len()).sum();

            let max_text_len = texts.iter().map(|t| t.len()).max().unwrap_or(0);
            info!(
                batch = batch_idx + 1,
                texts = texts.len(),
                total_chars,
                max_text_len,
                "sending embedding batch"
            );

            let resp = client
                .post(format!("{}/v1/embeddings", self.config.openai_base_url))
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&serde_json::json!({
                    "model": self.config.embedding_model,
                    "input": texts,
                }))
                .send()
                .await
                .context("Failed to call embeddings API")?;

            let status = resp.status();
            let body = resp.text().await.context("Failed to read response body")?;

            if !status.is_success() {
                anyhow::bail!(
                    "Embeddings API error (batch {}, {} texts, {} chars): {} - {}",
                    batch_idx + 1,
                    texts.len(),
                    total_chars,
                    status,
                    body
                );
            }

            let response: EmbeddingResponse = serde_json::from_str(&body).with_context(|| {
                format!(
                    "Failed to parse embeddings response (batch {}, {} texts, {} chars): {}",
                    batch_idx + 1,
                    texts.len(),
                    total_chars,
                    body
                )
            })?;

            for data in response.data {
                all_embeddings.push(data.embedding);
            }
        }

        Ok(all_embeddings)
    }

    /// Get the underlying database.
    pub fn db(&self) -> &LocalDb {
        &self.db
    }

    /// Get the underlying vector store.
    pub fn vectors(&self) -> &VectorStore {
        &self.vectors
    }

    /// Get the underlying storage.
    pub fn storage(&self) -> &LocalStorage {
        &self.storage
    }
}

#[derive(Debug, serde::Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, serde::Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
