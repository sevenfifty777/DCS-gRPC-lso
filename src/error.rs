#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Grpc(Box<tonic::Status>),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error("I/O error: {0}")]
    File(#[from] std::io::Error),
    #[error("I/O error for `{path}`: {source}")]
    FileAt {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to draw chart: {0}")]
    Draw(#[from] crate::draw::DrawError),
    #[error("failed to parse ACMI (Tacview) file: {0}")]
    Tracview(#[from] tacview::ParseError),
    #[error("failed to send Discord message: {0}")]
    Discord(#[source] Box<serenity::prelude::SerenityError>),
    #[error("failed to deserialize JSON: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid JSON in `{path}` at line {line}, column {column}: {source}")]
    JsonAt {
        path: std::path::PathBuf,
        line: usize,
        column: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid baseline manifest: {0}")]
    InvalidBaselineManifest(String),
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database error for `{path}`: {source}")]
    DbAt {
        path: std::path::PathBuf,
        #[source]
        source: rusqlite::Error,
    },
}

impl Error {
    pub fn file_at(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        Self::FileAt {
            path: path.into(),
            source,
        }
    }

    pub fn json_at(path: impl Into<std::path::PathBuf>, source: serde_json::Error) -> Self {
        Self::JsonAt {
            path: path.into(),
            line: source.line(),
            column: source.column(),
            source,
        }
    }

    pub fn db_at(path: impl Into<std::path::PathBuf>, source: rusqlite::Error) -> Self {
        Self::DbAt {
            path: path.into(),
            source,
        }
    }
}

impl From<tonic::Status> for Error {
    fn from(error: tonic::Status) -> Self {
        Self::Grpc(Box::new(error))
    }
}

impl From<Box<tonic::Status>> for Error {
    fn from(error: Box<tonic::Status>) -> Self {
        Self::Grpc(error)
    }
}

impl From<serenity::prelude::SerenityError> for Error {
    fn from(error: serenity::prelude::SerenityError) -> Self {
        Self::Discord(Box::new(error))
    }
}
