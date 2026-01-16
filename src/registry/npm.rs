//! npm registry client.

use std::collections::HashMap;
use std::io::Read;

use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use tar::Archive;
use tracing::debug;

use super::client::{PackageFile, PackageInfo, RegistryClient, VersionInfo};
use super::error::RegistryError;

const NPM_REGISTRY: &str = "https://registry.npmjs.org";

/// npm registry client.
pub struct NpmClient {
    client: Client,
    registry_url: String,
}

impl NpmClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            registry_url: NPM_REGISTRY.to_string(),
        }
    }

    pub fn with_registry_url(registry_url: String) -> Self {
        Self {
            client: Client::new(),
            registry_url,
        }
    }
}

impl Default for NpmClient {
    fn default() -> Self {
        Self::new()
    }
}

// npm registry response types
#[derive(Debug, Deserialize)]
struct NpmPackageResponse {
    name: String,
    description: Option<String>,
    license: Option<LicenseField>,
    repository: Option<Repository>,
    #[serde(rename = "dist-tags")]
    dist_tags: Option<HashMap<String, String>>,
    versions: HashMap<String, NpmVersionInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LicenseField {
    Simple(String),
    Complex {
        #[serde(rename = "type")]
        license_type: String,
    },
}

impl LicenseField {
    fn as_str(&self) -> &str {
        match self {
            LicenseField::Simple(s) => s,
            LicenseField::Complex { license_type } => license_type,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Repository {
    url: Option<String>,
    #[serde(rename = "type")]
    _repo_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmVersionInfo {
    name: String,
    version: String,
    description: Option<String>,
    license: Option<LicenseField>,
    repository: Option<Repository>,
    dist: NpmDist,
}

#[derive(Debug, Deserialize)]
struct NpmDist {
    tarball: String,
}

impl RegistryClient for NpmClient {
    async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let url = format!("{}/{}", self.registry_url, name);
        debug!(package = name, url = %url, "fetching npm package");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(RegistryError::RateLimited);
        }

        let npm_pkg: NpmPackageResponse = response.json().await?;

        let versions: Vec<String> = npm_pkg.versions.keys().cloned().collect();
        let latest = npm_pkg
            .dist_tags
            .as_ref()
            .and_then(|tags| tags.get("latest").cloned());

        Ok(PackageInfo {
            name: npm_pkg.name,
            description: npm_pkg.description,
            repository: npm_pkg.repository.and_then(|r| r.url),
            license: npm_pkg.license.map(|l| l.as_str().to_string()),
            versions,
            latest_version: latest,
        })
    }

    async fn get_version(&self, name: &str, version: &str) -> Result<VersionInfo, RegistryError> {
        let url = format!("{}/{}/{}", self.registry_url, name, version);
        debug!(package = name, version = version, url = %url, "fetching npm version");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let npm_ver: NpmVersionInfo = response.json().await?;

        Ok(VersionInfo {
            name: npm_ver.name,
            version: npm_ver.version,
            description: npm_ver.description,
            repository: npm_ver.repository.and_then(|r| r.url),
            license: npm_ver.license.map(|l| l.as_str().to_string()),
            tarball_url: npm_ver.dist.tarball,
        })
    }

    async fn download_source(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<PackageFile>, RegistryError> {
        let version_info = self.get_version(name, version).await?;

        debug!(
            package = name,
            version = version,
            tarball = %version_info.tarball_url,
            "downloading npm tarball"
        );

        let response = self.client.get(&version_info.tarball_url).send().await?;
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
        let path_str = path.to_string_lossy();

        // Skip directories
        if entry.header().entry_type().is_dir() {
            continue;
        }

        // npm tarballs have a "package/" prefix - strip it
        let clean_path = path_str
            .strip_prefix("package/")
            .unwrap_or(&path_str)
            .to_string();

        // Skip non-indexable files
        if !is_indexable_file(&clean_path) {
            continue;
        }

        // Read content
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            files.push(PackageFile {
                path: clean_path,
                content,
            });
        }
    }

    debug!(
        file_count = files.len(),
        "extracted source files from tarball"
    );
    Ok(files)
}

/// Check if a file should be indexed (source code, examples, or documentation).
fn is_indexable_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Include markdown documentation files
    if path_lower.ends_with(".md") || path_lower.ends_with(".markdown") {
        // But skip node_modules markdown
        if path_lower.contains("node_modules/") {
            return false;
        }
        return true;
    }

    // Source file extensions
    let source_extensions = [
        ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".py", ".pyi", ".rs", ".go",
        ".java",
    ];

    // Must have a source extension
    if !source_extensions
        .iter()
        .any(|ext| path_lower.ends_with(ext))
    {
        return false;
    }

    // Skip minified/bundled files
    if path_lower.contains(".min.")
        || path_lower.contains(".bundle.")
        || path_lower.contains(".prod.")
    {
        return false;
    }

    // Skip common non-source directories (but NOT examples/docs - we want those!)
    let skip_dirs = [
        "node_modules/",
        "dist/",
        "build/",
        "__pycache__/",
        ".git/",
        "test/",
        "tests/",
        "__tests__/",
        "spec/",
        "benchmark/",
        "benchmarks/",
    ];

    if skip_dirs.iter().any(|dir| path_lower.contains(dir)) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_indexable_file() {
        // Source files
        assert!(is_indexable_file("src/index.ts"));
        assert!(is_indexable_file("lib/utils.js"));
        assert!(is_indexable_file("src/main.py"));
        assert!(is_indexable_file("src/lib.rs"));

        // Examples should be included!
        assert!(is_indexable_file("examples/basic.ts"));
        assert!(is_indexable_file("example/advanced.js"));
        assert!(is_indexable_file("docs/api.ts"));

        // Markdown documentation
        assert!(is_indexable_file("README.md"));
        assert!(is_indexable_file("docs/guide.md"));
        assert!(is_indexable_file("CHANGELOG.markdown"));

        // Non-indexable files
        assert!(!is_indexable_file("package.json"));
        assert!(!is_indexable_file("dist/bundle.js"));
        assert!(!is_indexable_file("src/index.min.js"));
        assert!(!is_indexable_file("node_modules/lodash/index.js"));
        assert!(!is_indexable_file("node_modules/foo/README.md"));
        assert!(!is_indexable_file("test/index.test.ts"));
    }

    #[test]
    fn test_license_field_parsing() {
        let simple: LicenseField = serde_json::from_str(r#""MIT""#).unwrap();
        assert_eq!(simple.as_str(), "MIT");

        let complex: LicenseField = serde_json::from_str(r#"{"type": "Apache-2.0"}"#).unwrap();
        assert_eq!(complex.as_str(), "Apache-2.0");
    }

    // Integration tests would hit the actual npm registry
    // Run with: cargo test --package index-registry -- --ignored
    #[tokio::test]
    #[ignore]
    async fn test_get_package_lodash() {
        let client = NpmClient::new();
        let pkg = client.get_package("lodash").await.unwrap();
        assert_eq!(pkg.name, "lodash");
        assert!(pkg.versions.len() > 100);
    }

    #[tokio::test]
    #[ignore]
    async fn test_download_lodash() {
        let client = NpmClient::new();
        let files = client.download_source("lodash", "4.17.21").await.unwrap();
        assert!(!files.is_empty());
        // lodash has .js files
        assert!(files.iter().any(|f| f.path.ends_with(".js")));
    }
}
