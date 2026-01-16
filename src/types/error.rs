use thiserror::Error;

/// Low-level data errors that bubble up through the system.
///
/// Domain-specific errors (SearchError, IndexError, etc.) wrap these
/// and add context before converting to API responses.
#[derive(Error, Debug)]
pub enum DataError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("vector database error: {0}")]
    VectorDb(String),

    #[error("object storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("external service error: {0}")]
    ExternalService(String),
}
