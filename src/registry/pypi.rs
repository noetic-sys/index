//! PyPI registry client.

use std::io::{Cursor, Read};

use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use tar::Archive;
use tracing::debug;
use zip::ZipArchive;

use super::client::{PackageFile, PackageInfo, RegistryClient, VersionInfo};
use super::error::RegistryError;

const PYPI_API: &str = "https://pypi.org/pypi";

/// PyPI registry client.
pub struct PypiClient {
    client: Client,
    api_url: String,
}

impl PypiClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_url: PYPI_API.to_string(),
        }
    }

    pub fn with_api_url(api_url: String) -> Self {
        Self {
            client: Client::new(),
            api_url,
        }
    }
}

impl Default for PypiClient {
    fn default() -> Self {
        Self::new()
    }
}

// PyPI API response types
#[derive(Debug, Deserialize)]
struct PypiPackageResponse {
    info: PypiInfo,
    releases: std::collections::HashMap<String, Vec<PypiRelease>>,
}

#[derive(Debug, Deserialize)]
struct PypiInfo {
    name: String,
    summary: Option<String>,
    home_page: Option<String>,
    license: Option<String>,
    version: String,
}

#[derive(Debug, Deserialize)]
struct PypiRelease {
    packagetype: String,
    url: String,
    filename: String,
}

#[derive(Debug, Deserialize)]
struct PypiVersionResponse {
    info: PypiInfo,
    urls: Vec<PypiRelease>,
}

impl RegistryClient for PypiClient {
    async fn get_package(&self, name: &str) -> Result<PackageInfo, RegistryError> {
        let url = format!("{}/{}/json", self.api_url, name);
        debug!(package = name, url = %url, "fetching pypi package");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::PackageNotFound(name.to_string()));
        }

        let pypi_pkg: PypiPackageResponse = response.json().await?;

        let versions: Vec<String> = pypi_pkg.releases.keys().cloned().collect();

        Ok(PackageInfo {
            name: pypi_pkg.info.name,
            description: pypi_pkg.info.summary,
            repository: pypi_pkg.info.home_page,
            license: pypi_pkg.info.license,
            versions,
            latest_version: Some(pypi_pkg.info.version),
        })
    }

    async fn get_version(&self, name: &str, version: &str) -> Result<VersionInfo, RegistryError> {
        let url = format!("{}/{}/{}/json", self.api_url, name, version);
        debug!(package = name, version = version, url = %url, "fetching pypi version");

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::VersionNotFound {
                package: name.to_string(),
                version: version.to_string(),
            });
        }

        let pypi_ver: PypiVersionResponse = response.json().await?;

        // Prefer sdist (source distribution), fall back to wheel
        let tarball_url = pypi_ver
            .urls
            .iter()
            .find(|r| r.packagetype == "sdist")
            .or_else(|| pypi_ver.urls.iter().find(|r| r.packagetype == "bdist_wheel"))
            .map(|r| r.url.clone())
            .ok_or_else(|| RegistryError::Archive("no source distribution found".into()))?;

        Ok(VersionInfo {
            name: pypi_ver.info.name,
            version: pypi_ver.info.version,
            description: pypi_ver.info.summary,
            repository: pypi_ver.info.home_page,
            license: pypi_ver.info.license,
            tarball_url,
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
            "downloading pypi source"
        );

        let response = self.client.get(&version_info.tarball_url).send().await?;
        let bytes = response.bytes().await?;

        // PyPI can serve .tar.gz or .whl (zip) files
        if version_info.tarball_url.ends_with(".whl")
            || version_info.tarball_url.ends_with(".zip")
        {
            extract_zip(&bytes)
        } else {
            extract_tarball(&bytes)
        }
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

        // PyPI sdists have package-version/ prefix - strip first component
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

/// Extract source files from a zip/wheel file.
fn extract_zip(data: &[u8]) -> Result<Vec<PackageFile>, RegistryError> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| RegistryError::Archive(e.to_string()))?;

    let mut files = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| RegistryError::Archive(e.to_string()))?;

        if file.is_dir() {
            continue;
        }

        let path = file.name().to_string();

        if !is_indexable_file(&path) {
            continue;
        }

        let mut content = String::new();
        if file.read_to_string(&mut content).is_ok() {
            files.push(PackageFile { path, content });
        }
    }

    debug!(file_count = files.len(), "extracted source files from zip");
    Ok(files)
}

/// Strip the first path component (e.g., "requests-2.28.0/src/..." -> "src/...")
fn strip_first_component(path: &str) -> String {
    path.split_once('/')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Check if a file should be indexed (Python source, examples, or documentation).
fn is_indexable_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Include markdown documentation files (and RST for Python ecosystem)
    if path_lower.ends_with(".md")
        || path_lower.ends_with(".markdown")
        || path_lower.ends_with(".rst")
    {
        return true;
    }

    // Must be .py or .pyi for Python source
    if !path_lower.ends_with(".py") && !path_lower.ends_with(".pyi") {
        return false;
    }

    // Skip test files (but NOT examples - we want those!)
    let skip_patterns = [
        "test_",
        "_test.py",
        "tests/",
        "test/",
        "__pycache__/",
        "conftest.py",
    ];

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
        // Python source files
        assert!(is_indexable_file("src/requests/api.py"));
        assert!(is_indexable_file("requests/__init__.py"));
        assert!(is_indexable_file("typing_extensions.pyi"));
        assert!(is_indexable_file("setup.py"));

        // Examples should be included!
        assert!(is_indexable_file("examples/basic.py"));
        assert!(is_indexable_file("example/advanced.py"));

        // Markdown and RST documentation
        assert!(is_indexable_file("README.md"));
        assert!(is_indexable_file("docs/guide.md"));
        assert!(is_indexable_file("docs/api.rst"));
        assert!(is_indexable_file("CHANGELOG.rst"));

        // Tests still skipped
        assert!(!is_indexable_file("tests/test_api.py"));
        assert!(!is_indexable_file("test_requests.py"));
    }

    #[test]
    fn test_strip_first_component() {
        assert_eq!(
            strip_first_component("requests-2.28.0/src/api.py"),
            "src/api.py"
        );
        assert_eq!(strip_first_component("single.py"), "single.py");
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_package_requests() {
        let client = PypiClient::new();
        let pkg = client.get_package("requests").await.unwrap();
        assert_eq!(pkg.name.to_lowercase(), "requests");
        assert!(!pkg.versions.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_download_requests() {
        let client = PypiClient::new();
        let files = client.download_source("requests", "2.31.0").await.unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.path.ends_with(".py")));
    }
}
