//! Local search service.

use anyhow::{Context, Result};

use super::LocalConfig;
use super::db::LocalDb;
use super::models::SearchResult;
use super::storage::LocalStorage;
use super::vector::VectorStore;

/// Local search service.
pub struct LocalSearch {
    db: LocalDb,
    vectors: VectorStore,
    storage: LocalStorage,
    config: LocalConfig,
}

impl LocalSearch {
    /// Create a new search service from index directory.
    pub async fn new(index_dir: &std::path::Path) -> Result<Self> {
        let db = LocalDb::open(&index_dir.join("db.sqlite")).await?;
        let vectors = VectorStore::open(&index_dir.join("vectors")).await?;
        let storage = LocalStorage::new(index_dir.join("blobs")).await?;
        let config = LocalConfig::load()?;

        Ok(Self {
            db,
            vectors,
            storage,
            config,
        })
    }

    /// Search for code chunks.
    pub async fn search(
        &self,
        query: &str,
        package: Option<&str>,
        registry: Option<&str>,
        version: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // Generate query embedding
        let query_embedding = self.embed(query).await?;

        // Determine which namespaces to search
        let namespaces = if let Some(pkg) = package {
            // Search specific package
            if let Some(reg) = registry {
                if let Some(ver) = version {
                    vec![format!("{}/{}/{}", reg, pkg, ver)]
                } else {
                    // Search all versions of this package in this registry
                    self.db
                        .get_namespaces()
                        .await?
                        .into_iter()
                        .filter(|ns| ns.starts_with(&format!("{}/{}/", reg, pkg)))
                        .collect()
                }
            } else {
                // Search all registries for this package
                self.db
                    .get_namespaces()
                    .await?
                    .into_iter()
                    .filter(|ns| {
                        ns.contains(&format!("/{}/", pkg)) || ns.ends_with(&format!("/{}", pkg))
                    })
                    .collect()
            }
        } else {
            // Search all namespaces
            self.db.get_namespaces().await?
        };

        if namespaces.is_empty() {
            return Ok(vec![]);
        }

        // Search vectors
        let hits = self
            .vectors
            .search_multi(&namespaces, &query_embedding, limit)
            .await?;

        // Fetch chunk details
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(chunk) = self.db.get_chunk_with_package(&hit.chunk_id).await? {
                results.push(SearchResult {
                    id: chunk.id,
                    registry: chunk.registry,
                    package: chunk.package_name,
                    version: chunk.version,
                    chunk_type: chunk.chunk_type,
                    name: chunk.name,
                    file_path: chunk.file_path,
                    start_line: chunk.start_line as u32,
                    end_line: chunk.end_line as u32,
                    visibility: chunk.visibility,
                    signature: chunk.signature,
                    docstring: chunk.docstring,
                    snippet: chunk.snippet,
                    storage_key: chunk.storage_key,
                    score: hit.score(),
                });
            }
        }

        Ok(results)
    }

    /// Get full code for a chunk.
    pub async fn get_code(&self, storage_key: &str) -> Result<String> {
        let bytes = self.storage.get(storage_key).await?;
        String::from_utf8(bytes).context("Invalid UTF-8 in stored code")
    }

    /// Generate embedding for a query.
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let api_key = self
            .config
            .openai_api_key
            .as_ref()
            .context("OpenAI API key not configured. Run: idx config set-key")?;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/embeddings", self.config.openai_base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": self.config.embedding_model,
                "input": text,
            }))
            .send()
            .await
            .context("Failed to call embeddings API")?
            .error_for_status()
            .context("Embeddings API returned error")?
            .json::<EmbeddingResponse>()
            .await
            .context("Failed to parse embeddings response")?;

        response
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("No embedding returned")
    }

    /// List indexed packages.
    pub async fn list_packages(&self) -> Result<Vec<super::models::PackageRow>> {
        self.db.list_packages().await
    }

    /// Get the database.
    pub fn db(&self) -> &LocalDb {
        &self.db
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
