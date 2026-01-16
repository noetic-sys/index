//! SQLite database operations for local index.

use std::path::Path;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::models::{
    bytes_to_vector, vector_to_bytes, ChunkRow, ChunkWithPackage, CreateChunk, CreatePackage,
    ExistingChunk, PackageRow,
};

/// Local SQLite database.
pub struct LocalDb {
    pool: SqlitePool,
}

impl LocalDb {
    /// Open or create the database at the given path.
    pub async fn open(db_path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("Failed to open SQLite database")?;

        let db = Self { pool };
        db.migrate().await?;

        Ok(db)
    }

    /// Run database migrations.
    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS packages (
                id TEXT PRIMARY KEY,
                registry TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                description TEXT,
                indexed_at TEXT NOT NULL,
                UNIQUE(registry, name, version)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                package_id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                chunk_type TEXT NOT NULL,
                name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                visibility TEXT NOT NULL,
                signature TEXT,
                docstring TEXT,
                snippet TEXT NOT NULL,
                storage_key TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                vector BLOB NOT NULL,
                FOREIGN KEY (package_id) REFERENCES packages(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunks_namespace ON chunks(namespace)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunks_package ON chunks(package_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON chunks(content_hash)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ==================== Package Operations ====================

    /// Insert or update a package.
    pub async fn upsert_package(&self, input: &CreatePackage) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query(
            r#"
            INSERT INTO packages (id, registry, name, version, description, indexed_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(registry, name, version) DO UPDATE SET
                description = excluded.description,
                indexed_at = excluded.indexed_at
            RETURNING id
            "#,
        )
        .bind(&id)
        .bind(&input.registry)
        .bind(&input.name)
        .bind(&input.version)
        .bind(&input.description)
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.get("id"))
    }

    /// Find a package by registry, name, and version.
    pub async fn find_package(
        &self,
        registry: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<PackageRow>> {
        let row = sqlx::query_as::<_, PackageRow>(
            "SELECT * FROM packages WHERE registry = ? AND name = ? AND version = ?",
        )
        .bind(registry)
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// List all packages.
    pub async fn list_packages(&self) -> Result<Vec<PackageRow>> {
        let rows = sqlx::query_as::<_, PackageRow>(
            "SELECT * FROM packages ORDER BY indexed_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // ==================== Chunk Operations ====================

    /// Insert a chunk.
    pub async fn insert_chunk(&self, chunk: &CreateChunk) -> Result<()> {
        let vector_bytes = vector_to_bytes(&chunk.vector);

        sqlx::query(
            r#"
            INSERT INTO chunks (
                id, package_id, namespace, chunk_type, name, file_path,
                start_line, end_line, visibility, signature, docstring,
                snippet, storage_key, content_hash, vector
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chunk.id)
        .bind(&chunk.package_id)
        .bind(&chunk.namespace)
        .bind(&chunk.chunk_type)
        .bind(&chunk.name)
        .bind(&chunk.file_path)
        .bind(chunk.start_line as i64)
        .bind(chunk.end_line as i64)
        .bind(&chunk.visibility)
        .bind(&chunk.signature)
        .bind(&chunk.docstring)
        .bind(&chunk.snippet)
        .bind(&chunk.storage_key)
        .bind(&chunk.content_hash)
        .bind(&vector_bytes)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Batch insert chunks.
    pub async fn insert_chunks(&self, chunks: &[CreateChunk]) -> Result<()> {
        for chunk in chunks {
            self.insert_chunk(chunk).await?;
        }
        Ok(())
    }

    /// Delete all chunks for a package.
    pub async fn delete_package_chunks(&self, package_id: &str) -> Result<Vec<String>> {
        // Get namespaces before deleting
        let namespaces: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT namespace FROM chunks WHERE package_id = ?",
        )
        .bind(package_id)
        .fetch_all(&self.pool)
        .await?;

        sqlx::query("DELETE FROM chunks WHERE package_id = ?")
            .bind(package_id)
            .execute(&self.pool)
            .await?;

        Ok(namespaces)
    }

    /// Delete a package and all its chunks.
    pub async fn delete_package(&self, package_id: &str) -> Result<Vec<String>> {
        let namespaces = self.delete_package_chunks(package_id).await?;

        sqlx::query("DELETE FROM packages WHERE id = ?")
            .bind(package_id)
            .execute(&self.pool)
            .await?;

        Ok(namespaces)
    }

    /// Get all chunks in a namespace (for building HNSW index).
    pub async fn get_chunks_by_namespace(&self, namespace: &str) -> Result<Vec<ChunkRow>> {
        let rows = sqlx::query_as::<_, ChunkRow>(
            "SELECT * FROM chunks WHERE namespace = ?",
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Get chunk by ID with package info.
    pub async fn get_chunk_with_package(&self, id: &str) -> Result<Option<ChunkWithPackage>> {
        let row = sqlx::query_as::<_, ChunkWithPackage>(
            r#"
            SELECT
                c.id, c.namespace, c.chunk_type, c.name, c.file_path,
                c.start_line, c.end_line, c.visibility, c.signature,
                c.docstring, c.snippet, c.storage_key,
                p.registry, p.name as package_name, p.version
            FROM chunks c
            JOIN packages p ON c.package_id = p.id
            WHERE c.id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Get existing chunks for deduplication.
    pub async fn get_chunks_for_dedup(&self, namespace: &str) -> Result<Vec<ExistingChunk>> {
        let rows = sqlx::query("SELECT content_hash, vector FROM chunks WHERE namespace = ?")
            .bind(namespace)
            .fetch_all(&self.pool)
            .await?;

        let chunks = rows
            .into_iter()
            .map(|row| {
                let content_hash: String = row.get("content_hash");
                let vector_bytes: Vec<u8> = row.get("vector");
                ExistingChunk {
                    content_hash,
                    vector: bytes_to_vector(&vector_bytes),
                }
            })
            .collect();

        Ok(chunks)
    }

    /// Get all distinct namespaces.
    pub async fn get_namespaces(&self) -> Result<Vec<String>> {
        let namespaces: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT namespace FROM chunks")
                .fetch_all(&self.pool)
                .await?;

        Ok(namespaces)
    }
}
