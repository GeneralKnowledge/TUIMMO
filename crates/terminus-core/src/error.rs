use thiserror::Error;

/// A command was refused or the session cannot act.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameError {
    #[error("unknown session")]
    UnknownSession,
    #[error("{0}")]
    Rejected(String),
}

/// The command line could not be parsed against the registry.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("empty command")]
    Empty,
    #[error("line is too long")]
    TooLong,
    #[error("unknown command '{0}'")]
    Unknown(String),
    #[error("{message}")]
    BadArgs { message: String, usage: String },
}
