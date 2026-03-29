//! Error types for craps game actions.

use derive_more::{Display, Error};

/// Specific error conditions for craps.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display)]
pub enum CrapsErrorKind {
    /// Bet amount exceeds available bankroll.
    #[display("Insufficient funds: need {}, have {}", need, have)]
    InsufficientFunds {
        /// Amount required.
        need: u64,
        /// Amount available.
        have: u64,
    },

    /// Bet amount is zero or otherwise invalid.
    #[display("Invalid bet amount: {}", _0)]
    InvalidBet(u64),

    /// Bet type not allowed at current lesson level.
    #[display(
        "Bet {} requires lesson level {}, current level is {}",
        bet,
        required,
        current
    )]
    BetNotUnlocked {
        /// The bet that was attempted.
        bet: String,
        /// The lesson level required.
        required: u8,
        /// The player's current lesson level.
        current: u8,
    },

    /// Odds bet placed without a corresponding line bet.
    #[display("Odds bet requires an active {} bet", line_bet)]
    MissingLineBet {
        /// The line bet that should exist.
        line_bet: String,
    },

    /// Odds bet exceeds the table maximum multiple.
    #[display("Odds amount {} exceeds {}× maximum of {}", amount, multiple, max)]
    OddsExceedMax {
        /// The odds amount attempted.
        amount: u64,
        /// The allowed multiple.
        multiple: u8,
        /// The computed maximum.
        max: u64,
    },

    /// Come/Don't Come bet placed on the come-out roll (not allowed).
    #[display("Come bets cannot be placed during the come-out roll")]
    ComeBetOnComeOut,

    /// Game phase does not allow this action.
    #[display("Invalid action in current phase: {}", _0)]
    InvalidPhase(String),
}

/// Top-level craps error with source location.
#[derive(Debug, Clone, Display, Error)]
#[display("Craps error: {} at {}:{}", kind, file, line)]
pub struct CrapsError {
    /// The specific error condition.
    pub kind: CrapsErrorKind,
    /// Source line number.
    pub line: u32,
    /// Source file path.
    pub file: &'static str,
}

impl CrapsError {
    /// Creates a new error, capturing the caller's location.
    #[track_caller]
    pub fn new(kind: CrapsErrorKind) -> Self {
        let loc = std::panic::Location::caller();
        Self {
            kind,
            line: loc.line(),
            file: loc.file(),
        }
    }
}

impl From<CrapsErrorKind> for CrapsError {
    #[track_caller]
    fn from(kind: CrapsErrorKind) -> Self {
        Self::new(kind)
    }
}
