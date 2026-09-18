use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse {path}: {message}")]
    Parse { path: String, message: String },

    #[error("request failed: {0}")]
    Http(String),

    #[error("invalid url `{url}`: {message}")]
    Url { url: String, message: String },

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Error::Io { path: path.into(), source }
    }

    pub fn parse(path: impl Into<String>, message: impl std::fmt::Display) -> Self {
        Error::Parse { path: path.into(), message: message.to_string() }
    }
}

/// Tauri commands need the error to cross the IPC boundary as JSON.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
