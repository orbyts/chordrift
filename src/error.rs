use std::fmt;

use thiserror::Error;

/// The result type returned by Chordrift operations.
pub type Result<T> = std::result::Result<T, ChordriftError>;

/// Errors produced by Chordrift infrastructure.
#[derive(Error)]
#[non_exhaustive]
pub enum ChordriftError {
    /// Storexa rejected configuration or a database lifecycle operation.
    #[error(transparent)]
    Storexa(#[from] storexa::StorexaError),

    /// An application-owned Chordrift query failed.
    #[error("Chordrift database query failed")]
    Query(#[source] sqlx::Error),

    /// A command report could not be written.
    #[error("failed to write Chordrift command output")]
    Output(#[source] std::io::Error),

    /// Application or provider configuration is missing or invalid.
    #[error("configuration error: {0}")]
    Configuration(String),

    /// System credential storage failed.
    #[error("credential storage failed")]
    Credential(#[source] keyring::Error),

    /// A provider HTTP request failed before a response was available.
    #[error("provider request failed")]
    Http(#[source] reqwest::Error),

    /// A provider response could not be decoded.
    #[error("provider response was invalid")]
    Json(#[source] serde_json::Error),

    /// A ZIP archive could not be opened or decoded.
    #[error("Spotify archive could not be read")]
    Archive(#[source] zip::result::ZipError),

    /// Spotify rejected an API request.
    #[error("Spotify API request failed with status {status}: {message}")]
    SpotifyApi {
        /// HTTP status returned by Spotify.
        status: u16,
        /// Secret-free error explanation returned by Spotify.
        message: String,
    },
}

impl fmt::Debug for ChordriftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storexa(error) => formatter.debug_tuple("Storexa").field(error).finish(),
            Self::Query(_) => formatter.write_str("Query(..)"),
            Self::Output(_) => formatter.write_str("Output(..)"),
            Self::Configuration(message) => formatter
                .debug_tuple("Configuration")
                .field(message)
                .finish(),
            Self::Credential(_) => formatter.write_str("Credential(..)"),
            Self::Http(_) => formatter.write_str("Http(..)"),
            Self::Json(_) => formatter.write_str("Json(..)"),
            Self::Archive(_) => formatter.write_str("Archive(..)"),
            Self::SpotifyApi { status, message } => formatter
                .debug_struct("SpotifyApi")
                .field("status", status)
                .field("message", message)
                .finish(),
        }
    }
}

impl ChordriftError {
    /// Returns structured database diagnostics that cannot contain connection secrets.
    pub fn safe_diagnostic(&self) -> Option<String> {
        let Self::Query(error) = self else {
            return None;
        };
        match error {
            sqlx::Error::Database(database) => Some(format!(
                "database_code={} constraint={} table={}",
                database.code().as_deref().unwrap_or("unknown"),
                database.constraint().unwrap_or("unknown"),
                database.table().unwrap_or("unknown")
            )),
            sqlx::Error::ColumnDecode { index, .. } => Some(format!("column_decode index={index}")),
            sqlx::Error::ColumnNotFound(column) => {
                Some(format!("column_not_found column={column}"))
            }
            sqlx::Error::TypeNotFound { type_name } => {
                Some(format!("type_not_found type={type_name}"))
            }
            _ => Some("database_driver_error".to_owned()),
        }
    }
}

impl From<sqlx::Error> for ChordriftError {
    fn from(error: sqlx::Error) -> Self {
        Self::Query(error)
    }
}

impl From<std::io::Error> for ChordriftError {
    fn from(error: std::io::Error) -> Self {
        Self::Output(error)
    }
}

impl From<reqwest::Error> for ChordriftError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for ChordriftError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<zip::result::ZipError> for ChordriftError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Archive(error)
    }
}

#[cfg(test)]
mod tests {
    use super::ChordriftError;

    #[test]
    fn redacts_query_driver_details() {
        let error = ChordriftError::Query(sqlx::Error::Protocol(
            "postgresql://listener:secret@example.com/chordrift".to_owned(),
        ));

        assert_eq!(format!("{error}"), "Chordrift database query failed");
        assert_eq!(format!("{error:?}"), "Query(..)");
    }
}
