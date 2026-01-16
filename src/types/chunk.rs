use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Registry, TenantId};

pub type ChunkId = Uuid;

/// Metadata stored in Turbopuffer alongside the embedding vector.
///
/// # Storage Strategy
/// - Embedding vector + these attributes → Turbopuffer
/// - Full code blob → Object storage (referenced by `storage_key`)
///
/// # Attribute Limits (Turbopuffer)
/// - Max filterable attribute: 4 KiB
/// - Max non-filterable attribute: 8 MiB
/// - We keep `snippet` small (~500-1000 chars) for fast retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub id: ChunkId,

    // -- Namespace & Ownership --
    /// Turbopuffer namespace (e.g., "public/npm/lodash" or "private/acme-corp/npm/utils")
    pub namespace: String,
    /// None for public packages, Some for private
    pub tenant_id: Option<TenantId>,

    // -- Package Identity --
    pub registry: Registry,
    pub package: String,
    pub version: String,

    // -- Version Metadata (for filtering) --
    /// Parsed major version number (e.g., 1 for "1.7.2")
    pub major_version: u32,
    /// Parsed minor version number (e.g., 7 for "1.7.2")
    pub minor_version: u32,
    /// Parsed patch version number (e.g., 2 for "1.7.2")
    pub patch_version: u32,
    /// True if this is the latest published version
    pub is_latest: bool,
    /// True if this is the latest within its major version
    pub is_latest_major: bool,

    // -- Code Location --
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,

    // -- Semantic Info --
    pub chunk_type: ChunkType,
    /// Visibility/access level (public, private, etc.)
    pub visibility: Visibility,
    /// Name of the function/class/method (e.g., "useState", "DataFrame")
    pub name: String,
    /// Full signature if available (e.g., "fn parse(input: &str) -> Result<T, E>")
    pub signature: Option<String>,
    /// Docstring or comment if present
    pub docstring: Option<String>,
    /// Parent class/module name if this is a method
    pub parent: Option<String>,

    // -- Content References --
    /// Preview snippet (~500-1000 chars) for search results
    pub snippet: String,
    /// Object storage key for the full code blob
    pub storage_key: String,
    /// SHA256 hash of the code content for deduplication
    pub content_hash: String,
}

/// Full chunk including the code content, returned after fetching from storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedChunk {
    #[serde(flatten)]
    pub metadata: ChunkMetadata,
    /// Full source code (fetched from object storage, not stored in Turbopuffer)
    pub code: String,
}

/// The semantic type of a code chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkType {
    Function,
    Method,
    Class,
    Interface,
    Type,
    Constant,
    Module,
    /// Code from an examples/ directory
    Example,
    /// Documentation content (README, docs/)
    Documentation,
}

/// Visibility/access level of a code element.
///
/// Used to filter search results - users may want only public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Fully public API (pub, export, public, Capitalized in Go)
    #[default]
    Public,
    /// Protected access (protected in Java, pub(super) in Rust)
    Protected,
    /// Internal/package-private (pub(crate) in Rust, lowercase in Go, default in Java)
    Internal,
    /// Private (private keyword, _ prefix, #private in JS)
    Private,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Internal => "internal",
            Visibility::Private => "private",
        }
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Visibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "public" | "pub" => Ok(Visibility::Public),
            "protected" => Ok(Visibility::Protected),
            "internal" | "crate" => Ok(Visibility::Internal),
            "private" | "priv" => Ok(Visibility::Private),
            _ => Err(format!("unknown visibility: {}", s)),
        }
    }
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkType::Function => "function",
            ChunkType::Method => "method",
            ChunkType::Class => "class",
            ChunkType::Interface => "interface",
            ChunkType::Type => "type",
            ChunkType::Constant => "constant",
            ChunkType::Module => "module",
            ChunkType::Example => "example",
            ChunkType::Documentation => "documentation",
        }
    }
}

impl std::fmt::Display for ChunkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ChunkType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "function" | "fn" => Ok(ChunkType::Function),
            "method" => Ok(ChunkType::Method),
            "class" => Ok(ChunkType::Class),
            "interface" => Ok(ChunkType::Interface),
            "type" => Ok(ChunkType::Type),
            "constant" | "const" => Ok(ChunkType::Constant),
            "module" | "mod" => Ok(ChunkType::Module),
            "example" => Ok(ChunkType::Example),
            "documentation" | "doc" | "docs" => Ok(ChunkType::Documentation),
            _ => Err(format!("unknown chunk type: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_from_str() {
        assert_eq!("public".parse::<Visibility>().unwrap(), Visibility::Public);
        assert_eq!("pub".parse::<Visibility>().unwrap(), Visibility::Public);
        assert_eq!("protected".parse::<Visibility>().unwrap(), Visibility::Protected);
        assert_eq!("internal".parse::<Visibility>().unwrap(), Visibility::Internal);
        assert_eq!("crate".parse::<Visibility>().unwrap(), Visibility::Internal);
        assert_eq!("private".parse::<Visibility>().unwrap(), Visibility::Private);
        assert_eq!("priv".parse::<Visibility>().unwrap(), Visibility::Private);
    }

    #[test]
    fn test_visibility_from_str_case_insensitive() {
        assert_eq!("PUBLIC".parse::<Visibility>().unwrap(), Visibility::Public);
        assert_eq!("Private".parse::<Visibility>().unwrap(), Visibility::Private);
    }

    #[test]
    fn test_visibility_from_str_invalid() {
        assert!("invalid".parse::<Visibility>().is_err());
        assert!("".parse::<Visibility>().is_err());
    }

    #[test]
    fn test_visibility_roundtrip() {
        for vis in [Visibility::Public, Visibility::Protected, Visibility::Internal, Visibility::Private] {
            let s = vis.as_str();
            let parsed: Visibility = s.parse().unwrap();
            assert_eq!(vis, parsed);
        }
    }

    #[test]
    fn test_visibility_display() {
        assert_eq!(format!("{}", Visibility::Public), "public");
        assert_eq!(format!("{}", Visibility::Private), "private");
    }

    #[test]
    fn test_chunk_type_from_str() {
        assert_eq!("function".parse::<ChunkType>().unwrap(), ChunkType::Function);
        assert_eq!("fn".parse::<ChunkType>().unwrap(), ChunkType::Function);
        assert_eq!("method".parse::<ChunkType>().unwrap(), ChunkType::Method);
        assert_eq!("class".parse::<ChunkType>().unwrap(), ChunkType::Class);
        assert_eq!("interface".parse::<ChunkType>().unwrap(), ChunkType::Interface);
        assert_eq!("type".parse::<ChunkType>().unwrap(), ChunkType::Type);
        assert_eq!("constant".parse::<ChunkType>().unwrap(), ChunkType::Constant);
        assert_eq!("const".parse::<ChunkType>().unwrap(), ChunkType::Constant);
        assert_eq!("module".parse::<ChunkType>().unwrap(), ChunkType::Module);
        assert_eq!("mod".parse::<ChunkType>().unwrap(), ChunkType::Module);
        assert_eq!("example".parse::<ChunkType>().unwrap(), ChunkType::Example);
        assert_eq!("documentation".parse::<ChunkType>().unwrap(), ChunkType::Documentation);
        assert_eq!("doc".parse::<ChunkType>().unwrap(), ChunkType::Documentation);
        assert_eq!("docs".parse::<ChunkType>().unwrap(), ChunkType::Documentation);
    }

    #[test]
    fn test_chunk_type_from_str_case_insensitive() {
        assert_eq!("FUNCTION".parse::<ChunkType>().unwrap(), ChunkType::Function);
        assert_eq!("Documentation".parse::<ChunkType>().unwrap(), ChunkType::Documentation);
        assert_eq!("EXAMPLE".parse::<ChunkType>().unwrap(), ChunkType::Example);
    }

    #[test]
    fn test_chunk_type_from_str_invalid() {
        assert!("invalid".parse::<ChunkType>().is_err());
        assert!("".parse::<ChunkType>().is_err());
        assert!("func".parse::<ChunkType>().is_err());
    }

    #[test]
    fn test_chunk_type_roundtrip() {
        for ct in [
            ChunkType::Function,
            ChunkType::Method,
            ChunkType::Class,
            ChunkType::Interface,
            ChunkType::Type,
            ChunkType::Constant,
            ChunkType::Module,
            ChunkType::Example,
            ChunkType::Documentation,
        ] {
            let s = ct.as_str();
            let parsed: ChunkType = s.parse().unwrap();
            assert_eq!(ct, parsed);
        }
    }

    #[test]
    fn test_chunk_type_display() {
        assert_eq!(format!("{}", ChunkType::Function), "function");
        assert_eq!(format!("{}", ChunkType::Documentation), "documentation");
        assert_eq!(format!("{}", ChunkType::Example), "example");
    }
}
