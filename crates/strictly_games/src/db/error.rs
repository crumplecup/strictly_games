//! Database error types.

use derive_more::{Display, Error};
use tracing::instrument;

/// Database error with location tracking.
#[derive(Debug, Clone, Display, Error)]
#[display("Database error: {} at {}:{}", message, file, line)]
pub struct DbError {
    /// Error message.
    pub message: String,
    /// Line number where error occurred.
    pub line: u32,
    /// Source file where error occurred.
    pub file: &'static str,
}

impl DbError {
    /// Creates a new database error with caller location tracking.
    #[track_caller]
    #[instrument(skip(message))] // ✅ CORRECT: ALL functions instrumented
    pub fn new(message: impl Into<String>) -> Self {
        let loc = std::panic::Location::caller();
        Self {
            message: message.into(),
            line: loc.line(),
            file: loc.file(),
        }
    }
}

// ✅ CORRECT: From impls don't need #[instrument] (conversion traits)
impl From<diesel::result::Error> for DbError {
    #[track_caller]
    fn from(err: diesel::result::Error) -> Self {
        Self::new(format!("Diesel error: {}", err))
    }
}

impl From<diesel::ConnectionError> for DbError {
    #[track_caller]
    fn from(err: diesel::ConnectionError) -> Self {
        Self::new(format!("Connection error: {}", err))
    }
}
