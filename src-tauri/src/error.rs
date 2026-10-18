//! Error type returned by every Tauri command, serialized as `{ kind, message }`.

use omnimark_core::{ConvertError, ErrorKind};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Convert(#[from] ConvertError),
    #[error("a conversion is already running")]
    Busy,
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Tauri(String),
}

impl From<tauri::Error> for AppError {
    fn from(error: tauri::Error) -> Self {
        Self::Tauri(error.to_string())
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum Kind {
    Convert(ErrorKind),
    App(&'static str),
}

impl AppError {
    fn kind(&self) -> Kind {
        match self {
            Self::Convert(e) => Kind::Convert(e.kind()),
            Self::Busy => Kind::App("busy"),
            Self::InvalidPath(_) => Kind::App("invalidPath"),
            Self::Io(_) | Self::Tauri(_) => Kind::App("io"),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", &self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_and_message() {
        let busy = serde_json::to_value(AppError::Busy).unwrap();
        assert_eq!(busy["kind"], "busy");
        let invalid = serde_json::to_value(AppError::InvalidPath("x".into())).unwrap();
        assert_eq!(invalid["kind"], "invalidPath");
        let convert = serde_json::to_value(AppError::Convert(ConvertError::TooLarge {
            size: 2,
            limit: 1,
        }))
        .unwrap();
        assert_eq!(convert["kind"], "tooLarge");
        assert!(convert["message"].as_str().unwrap().contains("too large"));
    }
}
