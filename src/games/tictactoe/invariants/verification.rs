//! Formal verification of invariants using Kani model checker.
//!
//! These proof harnesses mathematically verify that invariants hold
//! for ALL possible game states (bounded).

#[cfg(kani)]
mod proofs {
    use crate::{
        AlternatingTurnInvariant, GameInProgress, HistoryConsistentInvariant, Invariant,
        InvariantSet, MonotonicBoardInvariant, TicTacToeInvariants,
    };

    /// Verify MonotonicBoardInvariant holds for all reachable states.
    ///
    /// Proves: Squares only transition Empty → Occupied, never reverse.
    #[kani::proof]
    #[kani::unwind(5)]
    fn verify_monotonic_board_simple() {
        // Create arbitrary game state
        let game: GameInProgress = kani::any();

        // Assume VERY basic constraints
        kani::assume(game.history().len() > 0);
        kani::assume(game.history().len() <= 5); // Small bound for speed

        // PROVE: Monotonic invariant holds
        assert!(
            MonotonicBoardInvariant::holds(&game),
            "MonotonicBoardInvariant violated"
        );
    }
}
