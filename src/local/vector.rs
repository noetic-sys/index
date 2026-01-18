//! LanceDB vector storage for similarity search.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};

use super::models::{VECTOR_DIM, VectorRecord, VectorSearchHit};

/// LanceDB-based vector store.
pub struct VectorStore {
    db: Connection,
}

impl VectorStore {
    /// Open or create a vector store at the given path.
    pub async fn open(path: &Path) -> Result<Self> {
        let db = lancedb::connect(path.to_str().unwrap())
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        Ok(Self { db })
    }

    /// Get or create a table for a namespace.
    pub async fn get_or_create_table(&self, namespace: &str) -> Result<Table> {
        let table_name = sanitize_table_name(namespace);

        if let Ok(table) = self.db.open_table(&table_name).execute().await {
            return Ok(table);
        }

        let schema = Self::schema();
        let empty_batch = Self::empty_batch(&schema)?;
        let batches = RecordBatchIterator::new(vec![Ok(empty_batch)], schema);

        let table = self
            .db
            .create_table(&table_name, Box::new(batches))
            .execute()
            .await
            .with_context(|| {
                format!(
                    "Failed to create table '{}' (namespace: {})",
                    table_name, namespace
                )
            })?;

        Ok(table)
    }

    /// Insert vectors into a namespace.
    pub async fn insert(&self, namespace: &str, records: Vec<VectorRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let table = self.get_or_create_table(namespace).await?;
        let batch = Self::records_to_batch(&records)?;
        let batches = RecordBatchIterator::new(vec![Ok(batch)], Self::schema());

        table
            .add(Box::new(batches))
            .execute()
            .await
            .context("Failed to insert vectors")?;

        Ok(())
    }

    /// Search for similar vectors in a namespace.
    pub async fn search(
        &self,
        namespace: &str,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchHit>> {
        let table_name = sanitize_table_name(namespace);

        let table = match self.db.open_table(&table_name).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let results = table
            .query()
            .nearest_to(query_vector)
            .context("Invalid query vector")?
            .limit(limit)
            .execute()
            .await
            .context("Failed to execute search")?
            .try_collect::<Vec<_>>()
            .await
            .context("Failed to collect results")?;

        let mut hits = Vec::new();
        for batch in results {
            hits.extend(Self::batch_to_hits(&batch)?);
        }

        hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        hits.truncate(limit);

        Ok(hits)
    }

    /// Search across multiple namespaces.
    pub async fn search_multi(
        &self,
        namespaces: &[String],
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchHit>> {
        let mut all_hits = Vec::new();

        for ns in namespaces {
            let hits = self.search(ns, query_vector, limit).await?;
            all_hits.extend(hits);
        }

        all_hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        all_hits.truncate(limit);

        Ok(all_hits)
    }

    /// Delete all vectors for a namespace.
    pub async fn delete_namespace(&self, namespace: &str) -> Result<()> {
        let table_name = sanitize_table_name(namespace);
        self.db.drop_table(&table_name).await.ok();
        Ok(())
    }

    /// List all namespaces (tables).
    pub async fn list_namespaces(&self) -> Result<Vec<String>> {
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("Failed to list tables")?;

        Ok(tables
            .into_iter()
            .map(|t| unsanitize_table_name(&t))
            .collect())
    }

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("chunk_id", DataType::Utf8, false),
            Field::new("content_hash", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    VECTOR_DIM,
                ),
                false,
            ),
        ]))
    }

    fn empty_batch(schema: &Arc<Schema>) -> Result<RecordBatch> {
        let chunk_ids = StringArray::from(Vec::<String>::new());
        let content_hashes = StringArray::from(Vec::<String>::new());
        let vectors = FixedSizeListArray::new(
            Arc::new(Field::new("item", DataType::Float32, true)),
            VECTOR_DIM,
            Arc::new(Float32Array::from(Vec::<f32>::new())),
            None,
        );

        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(chunk_ids),
                Arc::new(content_hashes),
                Arc::new(vectors),
            ],
        )
        .context("Failed to create empty batch")
    }

    fn records_to_batch(records: &[VectorRecord]) -> Result<RecordBatch> {
        let chunk_ids: Vec<&str> = records.iter().map(|r| r.chunk_id.as_str()).collect();
        let content_hashes: Vec<&str> = records.iter().map(|r| r.content_hash.as_str()).collect();
        let flat_vectors: Vec<f32> = records
            .iter()
            .flat_map(|r| r.vector.iter().copied())
            .collect();

        let chunk_id_array = StringArray::from(chunk_ids);
        let content_hash_array = StringArray::from(content_hashes);
        let vector_array = FixedSizeListArray::new(
            Arc::new(Field::new("item", DataType::Float32, true)),
            VECTOR_DIM,
            Arc::new(Float32Array::from(flat_vectors)),
            None,
        );

        RecordBatch::try_new(
            Self::schema(),
            vec![
                Arc::new(chunk_id_array),
                Arc::new(content_hash_array),
                Arc::new(vector_array),
            ],
        )
        .context("Failed to create record batch")
    }

    fn batch_to_hits(batch: &RecordBatch) -> Result<Vec<VectorSearchHit>> {
        let chunk_ids = batch
            .column_by_name("chunk_id")
            .context("Missing chunk_id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid chunk_id type")?;

        let distances = batch
            .column_by_name("_distance")
            .context("Missing _distance column")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .context("Invalid _distance type")?;

        let mut hits = Vec::with_capacity(batch.num_rows());
        for i in 0..batch.num_rows() {
            hits.push(VectorSearchHit {
                chunk_id: chunk_ids.value(i).to_string(),
                distance: distances.value(i),
            });
        }

        Ok(hits)
    }
}

fn sanitize_table_name(namespace: &str) -> String {
    // LanceDB: "Table names can only contain alphanumeric characters, underscores, hyphens, and periods"
    // We need to escape: / and @
    namespace
        .replace('/', "--S--") // slash
        .replace('@', "--A--") // at
}

fn unsanitize_table_name(table_name: &str) -> String {
    table_name.replace("--S--", "/").replace("--A--", "@")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_insert_and_search() {
        let dir = tempdir().unwrap();
        let store = VectorStore::open(dir.path()).await.unwrap();

        let records = vec![
            VectorRecord {
                chunk_id: "chunk1".to_string(),
                content_hash: "hash1".to_string(),
                vector: vec![1.0; VECTOR_DIM as usize],
            },
            VectorRecord {
                chunk_id: "chunk2".to_string(),
                content_hash: "hash2".to_string(),
                vector: vec![0.0; VECTOR_DIM as usize],
            },
        ];

        store.insert("test/namespace", records).await.unwrap();

        let query = vec![1.0; VECTOR_DIM as usize];
        let results = store.search("test/namespace", &query, 10).await.unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].chunk_id, "chunk1");
    }

    #[tokio::test]
    async fn test_delete_namespace() {
        let dir = tempdir().unwrap();
        let store = VectorStore::open(dir.path()).await.unwrap();

        let records = vec![VectorRecord {
            chunk_id: "chunk1".to_string(),
            content_hash: "hash1".to_string(),
            vector: vec![1.0; VECTOR_DIM as usize],
        }];

        store.insert("to_delete", records).await.unwrap();

        let namespaces = store.list_namespaces().await.unwrap();
        assert!(namespaces.contains(&"to_delete".to_string()));

        store.delete_namespace("to_delete").await.unwrap();

        let namespaces = store.list_namespaces().await.unwrap();
        assert!(!namespaces.contains(&"to_delete".to_string()));
    }
}
