//! Registry clients for fetching packages from npm, PyPI, crates.io, etc.
//!
//! This crate provides async clients for downloading package source code
//! from various package registries.
//!
//! # Example
//!
//! ```ignore
//! use crate::registry::{RegistryClients, PackageFile};
//! use crate::types::Registry;
//!
//! let client = RegistryClients::new(Registry::Npm);
//! let files: Vec<PackageFile> = client.download_source("lodash", "4.17.21").await?;
//! ```

mod client;
mod crates_io;
mod error;
mod go;
mod maven;
mod npm;
mod pypi;

pub use client::{PackageFile, PackageInfo, RegistryClient, VersionInfo};
pub use crates_io::CratesIoClient;
pub use error::RegistryError;
pub use go::GoClient;
pub use maven::MavenClient;
pub use npm::NpmClient;
pub use pypi::PypiClient;

use crate::types::Registry;

/// Unified registry client that dispatches to the appropriate implementation.
pub enum RegistryClients {
    Npm(NpmClient),
    Pypi(PypiClient),
    Crates(CratesIoClient),
    Maven(MavenClient),
    Go(GoClient),
}

impl RegistryClients {
    /// Create a new client for the given registry.
    pub fn new(registry: Registry) -> Self {
        match registry {
            Registry::Npm => Self::Npm(NpmClient::new()),
            Registry::Pypi => Self::Pypi(PypiClient::new()),
            Registry::Crates => Self::Crates(CratesIoClient::new()),
            Registry::Maven => Self::Maven(MavenClient::new()),
            Registry::Go => Self::Go(GoClient::new()),
        }
    }

    /// Get package metadata.
    pub async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        match self {
            Self::Npm(c) => c.get_package(name).await,
            Self::Pypi(c) => c.get_package(name).await,
            Self::Crates(c) => c.get_package(name).await,
            Self::Maven(c) => c.get_package(name).await,
            Self::Go(c) => c.get_package(name).await,
        }
    }

    /// Get version metadata.
    pub async fn get_version(
        &self,
        name: &str,
        version: &str,
    ) -> Result<VersionInfo, RegistryError> {
        match self {
            Self::Npm(c) => c.get_version(name, version).await,
            Self::Pypi(c) => c.get_version(name, version).await,
            Self::Crates(c) => c.get_version(name, version).await,
            Self::Maven(c) => c.get_version(name, version).await,
            Self::Go(c) => c.get_version(name, version).await,
        }
    }

    /// Download and extract package source files.
    pub async fn download_source(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<PackageFile>, RegistryError> {
        match self {
            Self::Npm(c) => c.download_source(name, version).await,
            Self::Pypi(c) => c.download_source(name, version).await,
            Self::Crates(c) => c.download_source(name, version).await,
            Self::Maven(c) => c.download_source(name, version).await,
            Self::Go(c) => c.download_source(name, version).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_clients() {
        let _npm = RegistryClients::new(Registry::Npm);
        let _pypi = RegistryClients::new(Registry::Pypi);
        let _crates = RegistryClients::new(Registry::Crates);
        let _maven = RegistryClients::new(Registry::Maven);
        let _go = RegistryClients::new(Registry::Go);
    }
}
