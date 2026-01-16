//! Local filesystem storage for code blobs.
//!
//! Blobs are stored per-package for easy cleanup:
//! ```text
//! .index/blobs/{registry}/{name}/{version}/{content_hash}
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Content-addressed blob storage organized by package.
pub struct LocalStorage {
    blobs_dir: PathBuf,
}

impl LocalStorage {
    /// Create a new storage instance at the given directory.
    pub async fn new(blobs_dir: PathBuf) -> Result<Self> {
        tokio::fs::create_dir_all(&blobs_dir)
            .await
            .context("Failed to create blobs directory")?;

        Ok(Self { blobs_dir })
    }

    /// Store a blob for a package, returns the storage key.
    pub async fn put(
        &self,
        registry: &str,
        name: &str,
        version: &str,
        content: &[u8],
    ) -> Result<String> {
        let hash = hex::encode(Sha256::digest(content));
        let key = format!("{}/{}/{}/{}", registry, name, version, hash);
        let path = self.blobs_dir.join(&key);

        if !path.exists() {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&path, content).await?;
        }

        Ok(key)
    }

    /// Get a blob by storage key.
    pub async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.blobs_dir.join(key);
        tokio::fs::read(&path)
            .await
            .context("Blob not found")
    }

    /// Check if a blob exists.
    pub async fn exists(&self, key: &str) -> bool {
        self.blobs_dir.join(key).exists()
    }

    /// Delete a blob by key.
    pub async fn delete(&self, key: &str) -> Result<()> {
        let path = self.blobs_dir.join(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// Delete all blobs for a package version.
    pub async fn delete_package(&self, registry: &str, name: &str, version: &str) -> Result<()> {
        let path = self.blobs_dir.join(registry).join(name).join(version);
        if path.exists() {
            tokio::fs::remove_dir_all(&path).await?;
        }
        Ok(())
    }

    /// Get the content hash from a storage key.
    pub fn hash_from_key(key: &str) -> Option<&str> {
        key.rsplit('/').next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_put_and_get() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().join("blobs")).await.unwrap();

        let content = b"hello world";
        let key = storage.put("npm", "lodash", "4.17.21", content).await.unwrap();

        assert!(key.starts_with("npm/lodash/4.17.21/"));
        assert!(storage.exists(&key).await);

        let retrieved = storage.get(&key).await.unwrap();
        assert_eq!(retrieved, content);
    }

    #[tokio::test]
    async fn test_content_addressed_within_package() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().join("blobs")).await.unwrap();

        let content = b"same content";
        let key1 = storage.put("npm", "axios", "1.0.0", content).await.unwrap();
        let key2 = storage.put("npm", "axios", "1.0.0", content).await.unwrap();

        assert_eq!(key1, key2);
    }

    #[tokio::test]
    async fn test_delete_package() {
        let dir = tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().join("blobs")).await.unwrap();

        let key1 = storage.put("npm", "react", "18.0.0", b"chunk1").await.unwrap();
        let key2 = storage.put("npm", "react", "18.0.0", b"chunk2").await.unwrap();

        assert!(storage.exists(&key1).await);
        assert!(storage.exists(&key2).await);

        storage.delete_package("npm", "react", "18.0.0").await.unwrap();

        assert!(!storage.exists(&key1).await);
        assert!(!storage.exists(&key2).await);
    }

    #[tokio::test]
    async fn test_hash_from_key() {
        let hash = LocalStorage::hash_from_key("npm/lodash/4.17.21/abc123");
        assert_eq!(hash, Some("abc123"));
    }
}
