use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::TenantId;

/// Which package namespaces to search/discover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    /// Only public packages (safe default for unauthenticated)
    #[default]
    Public,
    /// Only tenant's private packages (requires tenant_id)
    Private,
    /// Both public and private (requires auth)
    All,
}

impl std::fmt::Display for SearchScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchScope::Public => write!(f, "public"),
            SearchScope::Private => write!(f, "private"),
            SearchScope::All => write!(f, "all"),
        }
    }
}

impl std::str::FromStr for SearchScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "public" => Ok(SearchScope::Public),
            "private" => Ok(SearchScope::Private),
            "all" | "both" => Ok(SearchScope::All),
            _ => Err(format!("unknown search scope: {}", s)),
        }
    }
}

/// Package registries we index.
///
/// # Registry ↔ Language Mapping
/// Each registry is primarily associated with one language ecosystem:
/// - Npm → JavaScript/TypeScript
/// - Pypi → Python
/// - Crates → Rust
/// - Go → Go
/// - Maven → Java/Kotlin
///
/// Users can filter by registry at query time - this is NOT a tenant-level setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Registry {
    Npm,
    Pypi,
    Crates,
    Go,
    Maven,
}

impl Registry {
    pub fn as_str(&self) -> &'static str {
        match self {
            Registry::Npm => "npm",
            Registry::Pypi => "pypi",
            Registry::Crates => "crates",
            Registry::Go => "go",
            Registry::Maven => "maven",
        }
    }

    /// Returns the public discovery namespace for this registry.
    /// e.g., "public/_discover/npm"
    pub fn public_discover_namespace(&self) -> String {
        format!("public/_discover/{}", self.as_str())
    }

    /// Returns the private discovery namespace for a tenant.
    /// e.g., "private/acme-corp/_discover/npm"
    pub fn private_discover_namespace(&self, tenant: &str) -> String {
        format!("private/{}/_discover/{}", tenant, self.as_str())
    }

    /// Returns the public package namespace.
    /// e.g., "public/npm/lodash"
    pub fn public_package_namespace(&self, package: &str) -> String {
        format!("public/{}/{}", self.as_str(), package)
    }

    /// Returns the private package namespace for a tenant.
    /// e.g., "private/acme-corp/npm/internal-utils"
    pub fn private_package_namespace(&self, tenant: &str, package: &str) -> String {
        format!("private/{}/{}/{}", tenant, self.as_str(), package)
    }
}

impl std::fmt::Display for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Registry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "npm" => Ok(Registry::Npm),
            "pypi" => Ok(Registry::Pypi),
            "crates" => Ok(Registry::Crates),
            "go" => Ok(Registry::Go),
            "maven" => Ok(Registry::Maven),
            _ => Err(format!("unknown registry: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Language {
    TypeScript,
    JavaScript,
    Python,
    Rust,
    Go,
    Java,
    Kotlin,
    #[default]
    Unknown,
}


/// A package that has been indexed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub registry: Registry,
    pub name: String,
    pub version: String,
    /// None for public packages, Some(tenant_id) for private packages
    pub tenant_id: Option<TenantId>,
    pub indexed_at: DateTime<Utc>,
    pub chunk_count: u32,
    pub metadata: PackageMetadata,
}

impl Package {
    /// Returns the Turbopuffer namespace for this package.
    ///
    /// Public: "public/npm/lodash"
    /// Private: "private/{uuid}/npm/internal-utils"
    pub fn namespace(&self) -> String {
        match &self.tenant_id {
            None => self.registry.public_package_namespace(&self.name),
            Some(tenant) => self
                .registry
                .private_package_namespace(&tenant.to_string(), &self.name),
        }
    }

    /// Returns the discovery namespace for this package.
    ///
    /// Public: "public/_discover/npm"
    /// Private: "private/{uuid}/_discover/npm"
    pub fn discover_namespace(&self) -> String {
        match &self.tenant_id {
            None => self.registry.public_discover_namespace(),
            Some(tenant) => self
                .registry
                .private_discover_namespace(&tenant.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub language: Language,
}

/// Metadata stored in discovery namespace vectors.
/// One vector per package - used for cross-package discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryMetadata {
    pub registry: Registry,
    pub package: String,
    pub version: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub is_latest: bool,
    /// None for public packages, Some for private
    pub tenant_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_from_str() {
        assert_eq!("npm".parse::<Registry>().unwrap(), Registry::Npm);
        assert_eq!("pypi".parse::<Registry>().unwrap(), Registry::Pypi);
        assert_eq!("crates".parse::<Registry>().unwrap(), Registry::Crates);
        assert_eq!("go".parse::<Registry>().unwrap(), Registry::Go);
        assert_eq!("maven".parse::<Registry>().unwrap(), Registry::Maven);
    }

    #[test]
    fn test_registry_from_str_case_insensitive() {
        assert_eq!("NPM".parse::<Registry>().unwrap(), Registry::Npm);
        assert_eq!("PyPi".parse::<Registry>().unwrap(), Registry::Pypi);
        assert_eq!("CRATES".parse::<Registry>().unwrap(), Registry::Crates);
    }

    #[test]
    fn test_registry_from_str_invalid() {
        assert!("invalid".parse::<Registry>().is_err());
        assert!("".parse::<Registry>().is_err());
    }

    #[test]
    fn test_registry_roundtrip() {
        for registry in [
            Registry::Npm,
            Registry::Pypi,
            Registry::Crates,
            Registry::Go,
            Registry::Maven,
        ] {
            let s = registry.as_str();
            let parsed: Registry = s.parse().unwrap();
            assert_eq!(registry, parsed);
        }
    }

    #[test]
    fn test_registry_public_namespaces() {
        assert_eq!(
            Registry::Npm.public_discover_namespace(),
            "public/_discover/npm"
        );
        assert_eq!(
            Registry::Npm.public_package_namespace("axios"),
            "public/npm/axios"
        );
        assert_eq!(
            Registry::Crates.public_package_namespace("serde"),
            "public/crates/serde"
        );
    }

    #[test]
    fn test_registry_private_namespaces() {
        assert_eq!(
            Registry::Npm.private_discover_namespace("acme-corp"),
            "private/acme-corp/_discover/npm"
        );
        assert_eq!(
            Registry::Npm.private_package_namespace("acme-corp", "internal-utils"),
            "private/acme-corp/npm/internal-utils"
        );
        assert_eq!(
            Registry::Crates.private_package_namespace("acme-corp", "my-lib"),
            "private/acme-corp/crates/my-lib"
        );
    }

    #[test]
    fn test_package_namespace() {
        // Public package
        let public_pkg = Package {
            registry: Registry::Npm,
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
            tenant_id: None,
            indexed_at: chrono::Utc::now(),
            chunk_count: 100,
            metadata: PackageMetadata::default(),
        };
        assert_eq!(public_pkg.namespace(), "public/npm/lodash");
        assert_eq!(public_pkg.discover_namespace(), "public/_discover/npm");

        // Private package
        let tenant_uuid = uuid::Uuid::nil(); // Use nil UUID for deterministic test
        let private_pkg = Package {
            registry: Registry::Npm,
            name: "internal-utils".to_string(),
            version: "1.0.0".to_string(),
            tenant_id: Some(tenant_uuid),
            indexed_at: chrono::Utc::now(),
            chunk_count: 50,
            metadata: PackageMetadata::default(),
        };
        assert_eq!(
            private_pkg.namespace(),
            format!("private/{}/npm/internal-utils", tenant_uuid)
        );
        assert_eq!(
            private_pkg.discover_namespace(),
            format!("private/{}/_discover/npm", tenant_uuid)
        );
    }
}
