// Error types for the file analyzer

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyzerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Path error: {0}")]
    PathError(String),

    #[error("Thread pool error: {0}")]
    ThreadPool(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
