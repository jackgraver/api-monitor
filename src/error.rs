use std::fmt;
use std::io;

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Db(rusqlite::Error),
    Config(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "i/o error: {e}"),
            AppError::Db(e) => write!(f, "database error: {e}"),
            AppError::Config(msg) => write!(f, "configuration error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Io(e) => Some(e),
            AppError::Db(e) => Some(e),
            AppError::Config(_) => None,
        }
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e)
    }
}

#[derive(Debug)]
pub enum RequestError {
    MissingAuthToken,
    ClientBuild(reqwest::Error),
    Send(reqwest::Error),
    ReadBody(reqwest::Error),
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestError::MissingAuthToken => write!(
                f,
                "AUTH_TOKEN is not set. Add it to a .env file in the project root."
            ),
            RequestError::ClientBuild(e) => write!(f, "failed to build HTTP client: {e}"),
            RequestError::Send(e) => write!(f, "request failed: {e}"),
            RequestError::ReadBody(e) => write!(f, "failed to read response body: {e}"),
        }
    }
}

impl std::error::Error for RequestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RequestError::MissingAuthToken => None,
            RequestError::ClientBuild(e)
            | RequestError::Send(e)
            | RequestError::ReadBody(e) => Some(e),
        }
    }
}
