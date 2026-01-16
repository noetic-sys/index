//! SQLite database operations for local index.

use std::path::Path;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::models::{
    ChunkRow, ChunkWithPackage, CreateChunk, CreatePackage, ExistingChunk, IndexStats, PackageRow,
    VersionRow, VersionStatus, VersionWithPackage, bytes_to_vector, vector_to_bytes,
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
        // Check if we need to migrate from old schema
        let has_old_schema = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='packages' AND sql LIKE '%version TEXT%'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0) > 0;

        if has_old_schema {
            self.migrate_from_v1().await?;
        }

        // Packages table (unique by registry + name)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS packages (
                id TEXT PRIMARY KEY,
                registry TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(registry, name)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Versions table (unique by package_id + version)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS versions (
                id TEXT PRIMARY KEY,
                package_id TEXT NOT NULL,
                version TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                error_message TEXT,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                indexed_at TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (package_id) REFERENCES packages(id),
                UNIQUE(package_id, version)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Chunks table (references version_id)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                version_id TEXT NOT NULL,
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
                FOREIGN KEY (version_id) REFERENCES versions(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_versions_package ON versions(package_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_versions_status ON versions(status)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunks_namespace ON chunks(namespace)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunks_version ON chunks(version_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON chunks(content_hash)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Migrate from v1 schema (single packages table with version column).
    async fn migrate_from_v1(&self) -> Result<()> {
        tracing::info!("Migrating database from v1 schema...");

        // Rename old tables
        sqlx::query("ALTER TABLE packages RENAME TO packages_v1")
            .execute(&self.pool)
            .await?;

        sqlx::query("ALTER TABLE chunks RENAME TO chunks_v1")
            .execute(&self.pool)
            .await?;

        // New tables will be created by migrate()
        // Data migration would happen here if needed
        Ok(())
    }

    // ==================== Package Operations ====================

    /// Get or create a package, returning its ID.
    pub async fn get_or_create_package(&self, input: &CreatePackage) -> Result<String> {
        if let Some(pkg) = self.find_package(&input.registry, &input.name).await? {
            return Ok(pkg.id);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO packages (id, registry, name, description, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&input.registry)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Find a package by registry and name.
    pub async fn find_package(&self, registry: &str, name: &str) -> Result<Option<PackageRow>> {
        let row = sqlx::query_as::<_, PackageRow>(
            "SELECT * FROM packages WHERE registry = ? AND name = ?",
        )
        .bind(registry)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// List all packages.
    pub async fn list_packages(&self) -> Result<Vec<PackageRow>> {
        let rows =
            sqlx::query_as::<_, PackageRow>("SELECT * FROM packages ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?;

        Ok(rows)
    }

    /// Delete a package and all its versions/chunks.
    pub async fn delete_package(&self, package_id: &str) -> Result<Vec<String>> {
        let namespaces: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT c.namespace FROM chunks c
            JOIN versions v ON c.version_id = v.id
            WHERE v.package_id = ?
            "#,
        )
        .bind(package_id)
        .fetch_all(&self.pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM chunks WHERE version_id IN (
                SELECT id FROM versions WHERE package_id = ?
            )
            "#,
        )
        .bind(package_id)
        .execute(&self.pool)
        .await?;

        sqlx::query("DELETE FROM versions WHERE package_id = ?")
            .bind(package_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM packages WHERE id = ?")
            .bind(package_id)
            .execute(&self.pool)
            .await?;

        Ok(namespaces)
    }

    // ==================== Version Operations ====================

    /// Get or create a version, returning (version_id, should_skip).
    pub async fn get_or_create_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<(String, bool)> {
        if let Some(ver) = self.find_version_by_package(package_id, version).await? {
            let status = ver.status();
            let should_skip = matches!(status, VersionStatus::Indexed | VersionStatus::Skipped);
            return Ok((ver.id, should_skip));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO versions (id, package_id, version, status, created_at)
            VALUES (?, ?, ?, 'pending', ?)
            "#,
        )
        .bind(&id)
        .bind(package_id)
        .bind(version)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok((id, false))
    }

    /// Find a version by package_id and version string.
    pub async fn find_version_by_package(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<VersionRow>> {
        let row = sqlx::query_as::<_, VersionRow>(
            "SELECT * FROM versions WHERE package_id = ? AND version = ?",
        )
        .bind(package_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Find a version by registry, name, and version.
    pub async fn find_version(
        &self,
        registry: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<VersionWithPackage>> {
        let row = sqlx::query_as::<_, VersionWithPackage>(
            r#"
            SELECT
                v.id as version_id, v.version, v.status, v.error_message,
                v.chunk_count, v.indexed_at,
                p.id as package_id, p.registry, p.name, p.description
            FROM versions v
            JOIN packages p ON v.package_id = p.id
            WHERE p.registry = ? AND p.name = ? AND v.version = ?
            "#,
        )
        .bind(registry)
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// List all versions with package info.
    pub async fn list_versions(&self) -> Result<Vec<VersionWithPackage>> {
        let rows = sqlx::query_as::<_, VersionWithPackage>(
            r#"
            SELECT
                v.id as version_id, v.version, v.status, v.error_message,
                v.chunk_count, v.indexed_at,
                p.id as package_id, p.registry, p.name, p.description
            FROM versions v
            JOIN packages p ON v.package_id = p.id
            ORDER BY v.indexed_at DESC NULLS LAST, v.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// List versions by status.
    pub async fn list_versions_by_status(
        &self,
        status: VersionStatus,
    ) -> Result<Vec<VersionWithPackage>> {
        let rows = sqlx::query_as::<_, VersionWithPackage>(
            r#"
            SELECT
                v.id as version_id, v.version, v.status, v.error_message,
                v.chunk_count, v.indexed_at,
                p.id as package_id, p.registry, p.name, p.description
            FROM versions v
            JOIN packages p ON v.package_id = p.id
            WHERE v.status = ?
            ORDER BY v.created_at DESC
            "#,
        )
        .bind(status.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Mark a version as successfully indexed.
    pub async fn mark_version_indexed(&self, version_id: &str, chunk_count: i32) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE versions
            SET status = 'indexed', chunk_count = ?, indexed_at = ?, error_message = NULL
            WHERE id = ?
            "#,
        )
        .bind(chunk_count)
        .bind(&now)
        .bind(version_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a version as failed.
    pub async fn mark_version_failed(&self, version_id: &str, error: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE versions
            SET status = 'failed', error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(error)
        .bind(version_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a version as skipped.
    pub async fn mark_version_skipped(&self, version_id: &str) -> Result<()> {
        sqlx::query("UPDATE versions SET status = 'skipped' WHERE id = ?")
            .bind(version_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Mark a version for retry (set status to pending).
    pub async fn mark_version_pending(&self, version_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE versions SET status = 'pending', error_message = NULL WHERE id = ?",
        )
        .bind(version_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a version and its chunks.
    pub async fn delete_version(&self, version_id: &str) -> Result<Vec<String>> {
        let namespaces: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT namespace FROM chunks WHERE version_id = ?")
                .bind(version_id)
                .fetch_all(&self.pool)
                .await?;

        sqlx::query("DELETE FROM chunks WHERE version_id = ?")
            .bind(version_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM versions WHERE id = ?")
            .bind(version_id)
            .execute(&self.pool)
            .await?;

        Ok(namespaces)
    }

    // ==================== Chunk Operations ====================

    /// Insert a chunk.
    pub async fn insert_chunk(&self, chunk: &CreateChunk) -> Result<()> {
        let vector_bytes = vector_to_bytes(&chunk.vector);

        sqlx::query(
            r#"
            INSERT INTO chunks (
                id, version_id, namespace, chunk_type, name, file_path,
                start_line, end_line, visibility, signature, docstring,
                snippet, storage_key, content_hash, vector
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chunk.id)
        .bind(&chunk.version_id)
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

    /// Delete all chunks for a version.
    pub async fn delete_version_chunks(&self, version_id: &str) -> Result<Vec<String>> {
        let namespaces: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT namespace FROM chunks WHERE version_id = ?")
                .bind(version_id)
                .fetch_all(&self.pool)
                .await?;

        sqlx::query("DELETE FROM chunks WHERE version_id = ?")
            .bind(version_id)
            .execute(&self.pool)
            .await?;

        Ok(namespaces)
    }

    /// Get all chunks in a namespace.
    pub async fn get_chunks_by_namespace(&self, namespace: &str) -> Result<Vec<ChunkRow>> {
        let rows = sqlx::query_as::<_, ChunkRow>("SELECT * FROM chunks WHERE namespace = ?")
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
                p.registry, p.name as package_name, v.version
            FROM chunks c
            JOIN versions v ON c.version_id = v.id
            JOIN packages p ON v.package_id = p.id
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
        let namespaces: Vec<String> = sqlx::query_scalar("SELECT DISTINCT namespace FROM chunks")
            .fetch_all(&self.pool)
            .await?;

        Ok(namespaces)
    }

    // ==================== Stats ====================

    /// Get index statistics.
    pub async fn get_stats(&self) -> Result<IndexStats> {
        let package_count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM packages")
                .fetch_one(&self.pool)
                .await?;

        let version_count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM versions")
                .fetch_one(&self.pool)
                .await?;

        let indexed_count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM versions WHERE status = 'indexed'")
                .fetch_one(&self.pool)
                .await?;

        let failed_count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM versions WHERE status = 'failed'")
                .fetch_one(&self.pool)
                .await?;

        let skipped_count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM versions WHERE status = 'skipped'")
                .fetch_one(&self.pool)
                .await?;

        let chunk_count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
                .fetch_one(&self.pool)
                .await?;

        Ok(IndexStats {
            package_count: package_count as u32,
            version_count: version_count as u32,
            indexed_count: indexed_count as u32,
            failed_count: failed_count as u32,
            skipped_count: skipped_count as u32,
            chunk_count: chunk_count as u32,
        })
    }
}
