//! Crate error type.

use thiserror::Error;

/// Anything that can go wrong while parsing, resolving, or rendering DNS config.
#[derive(Debug, Error)]
pub enum Error {
    /// Input JSON could not be parsed into the raw model.
    #[error("failed to parse input JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// Input Pkl could not be evaluated into the raw model.
    #[error("failed to evaluate input Pkl: {0}")]
    Pkl(String),

    /// A record payload had the wrong shape for its type, or some other local failure.
    #[error("{0}")]
    Message(String),

    /// One or more declarative validation rules were violated. All violations are
    /// reported at once.
    #[error("validation failed:\n  - {}", .messages.join("\n  - "))]
    Validation { messages: Vec<String> },

    /// I/O failure reading input or writing output.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Build a [`Error::Message`] from anything string-like.
    pub fn msg(message: impl Into<String>) -> Self {
        Error::Message(message.into())
    }
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
