//! First-class invariants for tic-tac-toe.
//!
//! Invariants are logical properties that must hold throughout game execution.
//! They are testable independently and serve as documentation of system guarantees.

/// A logical property that must hold for a given state.
///
/// Invariants express system guarantees that should never be violated.
/// They are checked in debug builds and can be tested independently.
pub trait Invariant<S> {
    /// Checks if the invariant holds for the given state.
    fn holds(state: &S) -> bool;
    
    /// Human-readable description of the invariant.
    fn description() -> &'static str;
}

pub mod monotonic_board;
pub mod alternating_turn;
pub mod history_consistent;

pub use monotonic_board::MonotonicBoardInvariant;
pub use alternating_turn::AlternatingTurnInvariant;
pub use history_consistent::HistoryConsistentInvariant;
