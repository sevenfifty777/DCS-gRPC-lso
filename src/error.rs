#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Grpc(Box<tonic::Status>),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error("failed to open file")]
    File(#[from] std::io::Error),
    #[error("failed to draw chart")]
    Draw(#[from] crate::draw::DrawError),
    #[error("failed to parse ACMI (Tacview) file")]
    Tracview(#[from] tacview::ParseError),
    #[error("failed to send Discord message")]
    Discord(#[source] Box<serenity::prelude::SerenityError>),
    #[error("failed to deserialize JSON")]
    Serde(#[from] serde_json::Error),
    #[error("database error")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    RemovedOption(&'static str),
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
