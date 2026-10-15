use serde::{Deserialize, Serialize};

/// Machine-readable error category, shared with the UI and golden tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Unsupported,
    Corrupt,
    Encrypted,
    TooLarge,
    Cancelled,
    Io,
    MissingComponent,
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("unsupported format")]
    Unsupported,
    #[error("corrupt or unreadable file: {0}")]
    Corrupt(String),
    #[error("file is password protected")]
    Encrypted,
    #[error("file is too large ({size} bytes, limit {limit})")]
    TooLarge { size: u64, limit: u64 },
    #[error("cancelled")]
    Cancelled,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("missing component: {0}")]
    MissingComponent(String),
}

impl ConvertError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Unsupported => ErrorKind::Unsupported,
            Self::Corrupt(_) => ErrorKind::Corrupt,
            Self::Encrypted => ErrorKind::Encrypted,
            Self::TooLarge { .. } => ErrorKind::TooLarge,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Io(_) => ErrorKind::Io,
            Self::MissingComponent(_) => ErrorKind::MissingComponent,
        }
    }

    /// Wraps any parser error as `Corrupt`.
    pub fn corrupt(err: impl std::fmt::Display) -> Self {
        Self::Corrupt(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ConvertError>;
