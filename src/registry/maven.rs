//! Maven Central registry client.

use std::io::{Cursor, Read};

use reqwest::Client;
use serde::Deserialize;
use tracing::debug;
use zip::ZipArchive;

use super::client::{PackageFile, PackageInfo, RegistryClient, VersionInfo};
use super::error::RegistryError;

const MAVEN_SEARCH_API: &str = "https://search.maven.org/solrsearch/select";
const MAVEN_REPO: &str = "https://repo1.maven.org/maven2";

/// Maven Central registry client.
pub struct MavenClient {
    client: Client,
}

impl MavenClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("index-registry/0.1.0")
            .build()
            .expect("failed to build http client");

        Self { client }
    }
}

impl Default for MavenClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse groupId:artifactId from package name.
fn parse_coordinates(name: &str) -> Result<(&str, &str), RegistryError> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() != 2 {
        return Err(RegistryError::InvalidPackage(format!(
            "Maven coordinates must be groupId:artifactId, got: {}",
            name
        )));
    }
    Ok((parts[0], parts[1]))
}

/// Convert groupId to path (com.google.guava -> com/google/guava).
fn group_to_path(group_id: &str) -> String {
    group_id.replace('.', "/")
}

// Maven Central Search API response types
#[derive(Debug, Deserialize)]
struct SearchResponse {
    response: SearchResponseBody,
}

#[derive(Debug, Deserialize)]
struct SearchResponseBody {
    docs: Vec<SearchDoc>,
}

#[derive(Debug, Deserialize)]
struct SearchDoc {
    g: String, // groupId
    a: String, // artifactId
    v: String, // version
    #[serde(rename = "latestVersion")]
    latest_version: Option<String>,
}

impl RegistryClient for MavenClient {
    async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let (group_id, artifact_id) = parse_coordinates(name)?;

        let url = format!(
            "{}?q=g:{}+AND+a:{}&core=gav&rows=100&wt=json",
            MAVEN_SEARCH_API, group_id, artifact_id
        );

        debug!(package = name, url = %url, "fetching maven package");

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let search_resp: SearchResponse = response.json().await?;

        if search_resp.response.docs.is_empty() {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let versions: Vec<String> = search_resp
            .response
            .docs
            .iter()
            .map(|d| d.v.clone())
            .collect();

        let latest = search_resp
            .response
            .docs
            .first()
            .and_then(|d| d.latest_version.clone())
            .or_else(|| versions.first().cloned());

        Ok(PackageInfo {
            name: name.to_string(),
            description: None, // Maven search API doesn't return description
            repository: None,
            license: None,
            versions,
            latest_version: latest,
        })
    }

    async fn get_version(&self, name: &str, version: &str) -> Result<VersionInfo, RegistryError> {
        let (group_id, artifact_id) = parse_coordinates(name)?;
        let group_path = group_to_path(group_id);

        // Check if the sources JAR exists
        let sources_url = format!(
            "{}/{}/{}/{}/{}-{}-sources.jar",
            MAVEN_REPO, group_path, artifact_id, version, artifact_id, version
        );

        debug!(package = name, version = version, url = %sources_url, "checking maven version");

        let response = self.client.head(&sources_url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        Ok(VersionInfo {
            name: name.to_string(),
            version: version.to_string(),
            description: None,
            repository: None,
            license: None,
            tarball_url: sources_url,
        })
    }

    async fn download_source(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<PackageFile>, RegistryError> {
        let (group_id, artifact_id) = parse_coordinates(name)?;
        let group_path = group_to_path(group_id);

        let sources_url = format!(
            "{}/{}/{}/{}/{}-{}-sources.jar",
            MAVEN_REPO, group_path, artifact_id, version, artifact_id, version
        );

        debug!(
            package = name,
            version = version,
            url = %sources_url,
            "downloading maven sources"
        );

        let response = self.client.get(&sources_url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // Try the main JAR if sources JAR doesn't exist (some packages don't have sources)
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let bytes = response.bytes().await?;
        extract_sources_jar(&bytes)
    }
}

/// Extract source files from a sources JAR (which is just a ZIP file).
fn extract_sources_jar(data: &[u8]) -> Result<Vec<PackageFile>, RegistryError> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;

    let mut files = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;

        if entry.is_dir() {
            continue;
        }

        let path = entry.name().to_string();

        if !is_indexable_file(&path) {
            continue;
        }

        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            files.push(PackageFile { path, content });
        }
    }

    debug!(file_count = files.len(), "extracted source files from JAR");
    Ok(files)
}

/// Check if a file should be indexed.
fn is_indexable_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Include Java source files
    if path_lower.ends_with(".java") {
        // Skip test files
        if path_lower.contains("/test/") || path_lower.contains("test.java") {
            return false;
        }
        return true;
    }

    // Include Kotlin source files (some Maven packages are Kotlin)
    if path_lower.ends_with(".kt") || path_lower.ends_with(".kts") {
        if path_lower.contains("/test/") {
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
    fn test_parse_coordinates() {
        let (g, a) = parse_coordinates("com.google.guava:guava").unwrap();
        assert_eq!(g, "com.google.guava");
        assert_eq!(a, "guava");
    }

    #[test]
    fn test_parse_coordinates_invalid() {
        assert!(parse_coordinates("invalid").is_err());
        assert!(parse_coordinates("too:many:parts").is_err());
    }

    #[test]
    fn test_group_to_path() {
        assert_eq!(group_to_path("com.google.guava"), "com/google/guava");
        assert_eq!(group_to_path("org.apache.commons"), "org/apache/commons");
    }

    #[test]
    fn test_is_indexable_file() {
        assert!(is_indexable_file("src/main/java/com/example/App.java"));
        assert!(is_indexable_file("com/google/common/collect/Lists.java"));
        assert!(is_indexable_file("README.md"));

        // Skip tests
        assert!(!is_indexable_file("src/test/java/com/example/AppTest.java"));

        // Skip non-source files
        assert!(!is_indexable_file("META-INF/MANIFEST.MF"));
        assert!(!is_indexable_file("pom.xml"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_package_guava() {
        let client = MavenClient::new();
        let pkg = client.get_package("com.google.guava:guava").await.unwrap();
        assert_eq!(pkg.name, "com.google.guava:guava");
        assert!(!pkg.versions.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_download_guava() {
        let client = MavenClient::new();
        let files = client
            .download_source("com.google.guava:guava", "33.0.0-jre")
            .await
            .unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path.ends_with(".java")));
    }
}
