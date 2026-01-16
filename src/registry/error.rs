//! Registry client errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error("version not found: {package}@{version}")]
    VersionNotFound { package: String, version: String },

    #[error("invalid package: {0}")]
    InvalidPackage(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("archive error: {0}")]
    Archive(String),

    #[error("unsupported registry: {0}")]
    UnsupportedRegistry(String),

    #[error("rate limited")]
    RateLimited,
}
