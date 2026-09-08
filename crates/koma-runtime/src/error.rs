//! Runtime errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("package error: {0}")]
    Package(String),
    #[error("chapter `{0}` not found")]
    ChapterNotFound(String),
    #[error("user state error: {0}")]
    UserState(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
