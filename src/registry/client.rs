//! Registry client trait and common types.

use serde::{Deserialize, Serialize};

use super::error::RegistryError;

/// Metadata about a package from a registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub versions: Vec<String>,
    pub latest_version: Option<String>,
}

/// Metadata about a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub tarball_url: String,
}

/// A file extracted from a package.
#[derive(Debug, Clone)]
pub struct PackageFile {
    pub path: String,
    pub content: String,
}

/// Trait for registry clients.
///
/// Each registry (npm, pypi, crates, etc.) implements this trait
/// to provide package fetching capabilities.
pub trait RegistryClient: Send + Sync {
    /// Get package metadata.
    fn get_package(&self, name: &str) -> impl Future<Output = Result<PackageInfo, RegistryError>> + Send;

    /// Get version metadata.
    fn get_version(&self, name: &str, version: &str) -> impl Future<Output = Result<VersionInfo, RegistryError>> + Send;

    /// Download and extract package source files.
    ///
    /// Returns a list of (path, content) pairs for all source files.
    /// Filters out non-source files (binaries, etc.).
    fn download_source(&self, name: &str, version: &str) -> impl Future<Output = Result<Vec<PackageFile>, RegistryError>> + Send;
}

use std::future::Future;
