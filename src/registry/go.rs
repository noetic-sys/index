//! Go module proxy client.

use std::io::{Cursor, Read};

use reqwest::Client;
use serde::Deserialize;
use tracing::debug;
use zip::ZipArchive;

use super::client::{PackageFile, PackageInfo, RegistryClient, VersionInfo};
use super::error::RegistryError;

const GO_PROXY: &str = "https://proxy.golang.org";

/// Go module proxy client.
pub struct GoClient {
    client: Client,
}

impl GoClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("index-registry/0.1.0")
            .build()
            .expect("failed to build http client");

        Self { client }
    }
}

impl Default for GoClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape module path for Go proxy URL.
/// Uppercase letters become ! followed by lowercase.
/// e.g., github.com/BurntSushi/toml -> github.com/!burnt!sushi/toml
fn escape_module(module: &str) -> String {
    let mut result = String::with_capacity(module.len() + 10);
    for c in module.chars() {
        if c.is_ascii_uppercase() {
            result.push('!');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Normalize version - ensure v prefix.
fn normalize_version(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{}", version)
    }
}

// Go proxy response types
#[derive(Debug, Deserialize)]
struct GoVersionInfo {
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Time")]
    _time: Option<String>,
}

impl RegistryClient for GoClient {
    async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let escaped = escape_module(name);
        let url = format!("{}/@v/list", escaped);
        let full_url = format!("{}/{}", GO_PROXY, url);

        debug!(package = name, url = %full_url, "fetching go module versions");

        let response = self.client.get(&full_url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let text = response.text().await?;

        // Version list is newline-separated
        let versions: Vec<String> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        if versions.is_empty() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        // Latest is typically last in list
        let latest = versions.last().cloned();

        Ok(PackageInfo {
            name: name.to_string(),
            description: None,
            repository: Some(format!("https://{}", name)),
            license: None,
            versions,
            latest_version: latest,
        })
    }

    async fn get_version(
        &self,
        name: &str,
        version: &str,
    ) -> Result<super::client::VersionInfo, RegistryError> {
        let escaped = escape_module(name);
        let v = normalize_version(version);
        let url = format!("{}/{}/@v/{}.info", GO_PROXY, escaped, v);

        debug!(package = name, version = version, url = %url, "fetching go module version");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let info: GoVersionInfo = response.json().await?;

        let zip_url = format!("{}/{}/@v/{}.zip", GO_PROXY, escaped, v);

        Ok(super::client::VersionInfo {
            name: name.to_string(),
            version: info.version,
            description: None,
            repository: Some(format!("https://{}", name)),
            license: None,
            tarball_url: zip_url,
        })
    }

    async fn download_source(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<PackageFile>, RegistryError> {
        let escaped = escape_module(name);
        let v = normalize_version(version);
        let url = format!("{}/{}/@v/{}.zip", GO_PROXY, escaped, v);

        debug!(
            package = name,
            version = version,
            url = %url,
            "downloading go module source"
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let bytes = response.bytes().await?;
        extract_module_zip(&bytes)
    }
}

/// Extract source files from a Go module zip.
fn extract_module_zip(data: &[u8]) -> Result<Vec<PackageFile>, RegistryError> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;

    let mut files = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;

        if entry.is_dir() {
            continue;
        }

        let full_path = entry.name().to_string();

        // Go module zips have module@version/ prefix, strip it
        let path = strip_module_prefix(&full_path);

        if !is_indexable_file(&path) {
            continue;
        }

        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            files.push(PackageFile { path, content });
        }
    }

    debug!(
        file_count = files.len(),
        "extracted source files from module zip"
    );
    Ok(files)
}

/// Strip the module@version/ prefix from paths.
fn strip_module_prefix(path: &str) -> String {
    // Format is: module@version/path/to/file.go
    // Find first / after @ and strip everything before it
    if let Some(at_pos) = path.find('@') {
        if let Some(slash_pos) = path[at_pos..].find('/') {
            return path[at_pos + slash_pos + 1..].to_string();
        }
    }
    path.to_string()
}

/// Check if a file should be indexed.
fn is_indexable_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Include Go source files
    if path_lower.ends_with(".go") {
        // Skip test files
        if path_lower.ends_with("_test.go") {
            return false;
        }
        // Skip vendor directory
        if path_lower.starts_with("vendor/") || path_lower.contains("/vendor/") {
            return false;
        }
        return true;
    }

    // Include documentation
    if path_lower.ends_with(".md") || path_lower.ends_with(".markdown") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_module() {
        assert_eq!(
            escape_module("github.com/BurntSushi/toml"),
            "github.com/!burnt!sushi/toml"
        );
        assert_eq!(
            escape_module("github.com/gin-gonic/gin"),
            "github.com/gin-gonic/gin"
        );
        assert_eq!(escape_module("golang.org/x/sync"), "golang.org/x/sync");
    }

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("1.9.1"), "v1.9.1");
        assert_eq!(normalize_version("v1.9.1"), "v1.9.1");
    }

    #[test]
    fn test_strip_module_prefix() {
        assert_eq!(
            strip_module_prefix("github.com/gin-gonic/gin@v1.9.1/gin.go"),
            "gin.go"
        );
        assert_eq!(
            strip_module_prefix("github.com/gin-gonic/gin@v1.9.1/internal/json/json.go"),
            "internal/json/json.go"
        );
    }

    #[test]
    fn test_is_indexable_file() {
        assert!(is_indexable_file("main.go"));
        assert!(is_indexable_file("internal/handler.go"));
        assert!(is_indexable_file("README.md"));

        // Skip tests
        assert!(!is_indexable_file("main_test.go"));
        assert!(!is_indexable_file("handler_test.go"));

        // Skip vendor
        assert!(!is_indexable_file("vendor/github.com/pkg/errors/errors.go"));

        // Skip non-source
        assert!(!is_indexable_file("go.mod"));
        assert!(!is_indexable_file("go.sum"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_package_gin() {
        let client = GoClient::new();
        let pkg = client
            .get_package("github.com/gin-gonic/gin")
            .await
            .unwrap();
        assert_eq!(pkg.name, "github.com/gin-gonic/gin");
        assert!(!pkg.versions.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_download_gin() {
        let client = GoClient::new();
        let files = client
            .download_source("github.com/gin-gonic/gin", "v1.9.1")
            .await
            .unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path.ends_with(".go")));
    }
}
