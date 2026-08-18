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
}

impl fmt::Debug for ChordriftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storexa(error) => formatter.debug_tuple("Storexa").field(error).finish(),
            Self::Query(_) => formatter.write_str("Query(..)"),
            Self::Output(_) => formatter.write_str("Output(..)"),
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
