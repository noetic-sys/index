//! crates.io registry client.

use std::io::Read;

use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use tar::Archive;
use tracing::debug;

use super::client::{PackageFile, PackageInfo, RegistryClient, VersionInfo};
use super::error::RegistryError;

const CRATES_API: &str = "https://crates.io/api/v1";
const CRATES_DOWNLOAD: &str = "https://static.crates.io/crates";

/// crates.io registry client.
pub struct CratesIoClient {
    client: Client,
    api_url: String,
}

impl CratesIoClient {
    pub fn new() -> Self {
        // crates.io requires a user agent
        let client = Client::builder()
            .user_agent("index-registry/0.1.0 (https://github.com/yourusername/index)")
            .build()
            .expect("failed to build http client");

        Self {
            client,
            api_url: CRATES_API.to_string(),
        }
    }
}

impl Default for CratesIoClient {
    fn default() -> Self {
        Self::new()
    }
}

// crates.io API response types
#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
    versions: Vec<CrateVersionInfo>,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    name: String,
    description: Option<String>,
    repository: Option<String>,
    max_version: String,
}

#[derive(Debug, Deserialize)]
struct CrateVersionInfo {
    num: String,
    license: Option<String>,
    dl_path: String,
}

#[derive(Debug, Deserialize)]
struct SingleVersionResponse {
    version: CrateVersionInfo,
}

impl RegistryClient for CratesIoClient {
    async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let url = format!("{}/crates/{}", self.api_url, name);
        debug!(package = name, url = %url, "fetching crate");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let crate_resp: CrateResponse = response.json().await?;

        let versions: Vec<String> = crate_resp.versions.iter().map(|v| v.num.clone()).collect();

        Ok(PackageInfo {
            name: crate_resp.krate.name,
            description: crate_resp.krate.description,
            repository: crate_resp.krate.repository,
            license: crate_resp.versions.first().and_then(|v| v.license.clone()),
            versions,
            latest_version: Some(crate_resp.krate.max_version),
        })
    }

    async fn get_version(&self, name: &str, version: &str) -> Result<VersionInfo, RegistryError> {
        let url = format!("{}/crates/{}/{}", self.api_url, name, version);
        debug!(package = name, version = version, url = %url, "fetching crate version");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let ver_resp: SingleVersionResponse = response.json().await?;

        // crates.io download URL
        let tarball_url = format!("{}/{}/{}/download", CRATES_DOWNLOAD, name, version);

        Ok(VersionInfo {
            name: name.to_string(),
            version: ver_resp.version.num,
            description: None, // Not in version response
            repository: None,
            license: ver_resp.version.license,
            tarball_url,
        })
    }

    async fn download_source(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<PackageFile>, RegistryError> {
        let tarball_url = format!("{}/{}/{}/download", CRATES_DOWNLOAD, name, version);

        debug!(
            package = name,
            version = version,
            tarball = %tarball_url,
            "downloading crate tarball"
        );

        let response = self.client.get(&tarball_url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let bytes = response.bytes().await?;
        extract_tarball(&bytes)
    }
}

/// Extract source files from a gzipped tarball.
fn extract_tarball(data: &[u8]) -> Result<Vec<PackageFile>, RegistryError> {
    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);

    let mut files = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let path_str = path.to_string_lossy().to_string();

        if entry.header().entry_type().is_dir() {
            continue;
        }

        // crates have crate-version/ prefix
        let clean_path = strip_first_component(&path_str);

        if !is_indexable_file(&clean_path) {
            continue;
        }

        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            files.push(PackageFile {
                path: clean_path,
                content,
            });
        }
    }

    debug!(file_count = files.len(), "extracted source files from tarball");
    Ok(files)
}

fn strip_first_component(path: &str) -> String {
    path.split_once('/')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Check if a file should be indexed (Rust source, examples, or documentation).
fn is_indexable_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Include markdown documentation files
    if path_lower.ends_with(".md") || path_lower.ends_with(".markdown") {
        return true;
    }

    // Must be a Rust source file
    if !path_lower.ends_with(".rs") {
        return false;
    }

    // Skip test/bench files (but NOT examples - we want those!)
    let skip_patterns = ["tests/", "benches/", "test_"];

    if skip_patterns.iter().any(|p| path_lower.contains(p)) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_indexable_file() {
        // Rust source files
        assert!(is_indexable_file("src/lib.rs"));
        assert!(is_indexable_file("src/parser/mod.rs"));

        // Examples should be included!
        assert!(is_indexable_file("examples/basic.rs"));
        assert!(is_indexable_file("examples/advanced/multi.rs"));

        // Markdown documentation
        assert!(is_indexable_file("README.md"));
        assert!(is_indexable_file("docs/guide.md"));
        assert!(is_indexable_file("CHANGELOG.markdown"));

        // Tests and benches still skipped
        assert!(!is_indexable_file("tests/integration.rs"));
        assert!(!is_indexable_file("benches/bench.rs"));

        // Non-source files still skipped
        assert!(!is_indexable_file("Cargo.toml"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_package_serde() {
        let client = CratesIoClient::new();
        let pkg = client.get_package("serde").await.unwrap();
        assert_eq!(pkg.name, "serde");
        assert!(!pkg.versions.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_download_serde() {
        let client = CratesIoClient::new();
        let files = client.download_source("serde", "1.0.193").await.unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path.ends_with(".rs")));
    }
}
