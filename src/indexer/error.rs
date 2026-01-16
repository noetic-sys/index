use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("tree-sitter error: {0}")]
    TreeSitter(String),

    #[error("embeddings error: {0}")]
    Embeddings(#[from] anyhow::Error),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("invalid file: {0}")]
    InvalidFile(String),
}
