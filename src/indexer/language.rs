use crate::types::Registry;

use super::chunk::CodeChunk;
use super::error::IndexerError;

/// Supported languages for parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    TypeScript,
    JavaScript,
    Python,
    Rust,
    Go,
    Java,
    /// Markdown files (README, docs)
    Markdown,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "ts" | "tsx" | "mts" | "cts" => Some(Language::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "py" | "pyi" => Some(Language::Python),
            "rs" => Some(Language::Rust),
            "go" => Some(Language::Go),
            "java" => Some(Language::Java),
            "md" | "markdown" => Some(Language::Markdown),
            _ => None,
        }
    }

    /// Detect language from file path.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?;
        Self::from_extension(ext)
    }

    /// Infer likely language from registry.
    pub fn from_registry(registry: Registry) -> Vec<Self> {
        match registry {
            Registry::Npm => vec![Language::TypeScript, Language::JavaScript],
            Registry::Pypi => vec![Language::Python],
            Registry::Crates => vec![Language::Rust],
            Registry::Go => vec![Language::Go],
            Registry::Maven => vec![Language::Java],
        }
    }

    /// File extensions this language typically uses.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::TypeScript => &["ts", "tsx", "mts", "cts"],
            Language::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Language::Python => &["py", "pyi"],
            Language::Rust => &["rs"],
            Language::Go => &["go"],
            Language::Java => &["java"],
            Language::Markdown => &["md", "markdown"],
        }
    }

    /// Name of the language.
    pub fn name(&self) -> &'static str {
        match self {
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Python => "Python",
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::Java => "Java",
            Language::Markdown => "Markdown",
        }
    }
}

/// Trait for language-specific parsers.
///
/// Each language has different:
/// - Tree-sitter grammar
/// - Node types for functions, classes, etc.
/// - Docstring/comment conventions
pub trait LanguageParser: Send + Sync {
    /// Parse source code and extract code chunks.
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError>;

    /// The language this parser handles.
    fn language(&self) -> Language;

    /// Check if a file should be parsed (by extension).
    fn should_parse(&self, file_path: &str) -> bool {
        if let Some(lang) = Language::from_path(file_path) {
            lang == self.language()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
        assert_eq!(Language::from_extension("java"), Some(Language::Java));
        assert_eq!(Language::from_extension("txt"), None);
    }

    #[test]
    fn test_language_from_path() {
        assert_eq!(Language::from_path("src/index.ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("lib/utils.py"), Some(Language::Python));
        assert_eq!(Language::from_path("main.rs"), Some(Language::Rust));
    }

    #[test]
    fn test_language_from_registry() {
        let npm_langs = Language::from_registry(Registry::Npm);
        assert!(npm_langs.contains(&Language::TypeScript));
        assert!(npm_langs.contains(&Language::JavaScript));

        let pypi_langs = Language::from_registry(Registry::Pypi);
        assert!(pypi_langs.contains(&Language::Python));

        let crates_langs = Language::from_registry(Registry::Crates);
        assert!(crates_langs.contains(&Language::Rust));
    }
}
