//! Verified State Machine for tic-tac-toe.
//!
//! This module is the derive-only consumer layer.  No manual `impl` blocks
//! live here — all proof infrastructure is wired up via the macros.
//!
//! ## State diagram
//!
//! ```text
//! Setup ──ttt_start_game──► InProgress ──ttt_make_move──► InProgress
//!   ▲                                   └──ttt_make_move──► Finished
//!   └────────────────────ttt_restart──────────────────────────┘
//! ```

use crate::contracts::{PlayerTurn, SquareEmpty, TicTacToeConsistent, TicTacToeRulesEvidence};
use crate::display::TttDisplayMode;
use crate::typestate::{GameFinished, GameInProgress, GameResult, GameSetup};
use crate::{Move, Player};
use elicitation::contracts::Established;
use elicitation::{Elicit, KaniCompose, KaniVariantState, VerifiedStateMachine, formal_method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(not(kani))]
use tracing::instrument;

// ── TicTacToeState ────────────────────────────────────────────────────────────

/// State enum for the tic-tac-toe verified state machine.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,
    KaniVariantState,
    KaniCompose,
)]
pub enum TicTacToeState {
    /// Game is in setup phase, awaiting the first player assignment.
    Setup {
        /// Inner setup state.
        inner: GameSetup,
        /// Active display mode.
        display_mode: TttDisplayMode,
    },
    /// Game is in progress, accepting moves.
    InProgress {
        /// Inner in-progress state.
        inner: GameInProgress,
        /// Active display mode.
        display_mode: TttDisplayMode,
    },
    /// Game has ended with a winner or a draw.
    Finished {
        /// Inner finished state.
        inner: GameFinished,
        /// Active display mode.
        display_mode: TttDisplayMode,
    },
}

// ── TicTacToeMachine ──────────────────────────────────────────────────────────

/// Verified state machine for tic-tac-toe.
#[derive(VerifiedStateMachine)]
#[vsm(transitions = [ttt_start_game, ttt_make_move, ttt_restart])]
pub struct TicTacToeMachine;

impl Default for TicTacToeState {
    fn default() -> Self {
        Self::Setup {
            inner: GameSetup::default(),
            display_mode: TttDisplayMode::default(),
        }
    }
}

// ── Transitions ───────────────────────────────────────────────────────────────

/// Transition: assign the first player and start the game.
///
/// Only valid from the `Setup` state; all other states are passed through
/// unchanged (the proof token travels with the state).
#[formal_method(contracts = [TicTacToeConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn ttt_start_game(
    state: TicTacToeState,
    proof: Established<TicTacToeConsistent>,
    first_player: Player,
) -> (TicTacToeState, Established<TicTacToeConsistent>) {
    let TicTacToeState::Setup {
        inner: setup,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    (
        TicTacToeState::InProgress {
            inner: setup.start(first_player),
            display_mode,
        },
        proof,
    )
}

/// Transition: apply a validated move to an in-progress game.
///
/// Requires individual proofs for each move precondition:
/// - `square_proof`: the target square was empty (`Established<SquareEmpty>`)
/// - `turn_proof`: it is this player's turn (`Established<PlayerTurn>`)
///
/// These are assembled into a [`TicTacToeRulesEvidence`] bundle to re-issue
/// the [`TicTacToeConsistent`] invariant after the move.
#[formal_method(contracts = [TicTacToeConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof, square_proof, turn_proof)))]
pub fn ttt_make_move(
    state: TicTacToeState,
    proof: Established<TicTacToeConsistent>,
    mov: Move,
    square_proof: Established<SquareEmpty>,
    turn_proof: Established<PlayerTurn>,
) -> (TicTacToeState, Established<TicTacToeConsistent>) {
    let TicTacToeState::InProgress {
        inner: game,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    let new_proof = Established::prove(&TicTacToeRulesEvidence {
        square_empty: square_proof,
        player_turn: turn_proof,
    });
    match game.make_move(mov) {
        Ok(GameResult::InProgress(g)) => (
            TicTacToeState::InProgress {
                inner: g,
                display_mode,
            },
            new_proof,
        ),
        Ok(GameResult::Finished(g)) => (
            TicTacToeState::Finished {
                inner: g,
                display_mode,
            },
            new_proof,
        ),
        Err(_) => unreachable!("square_proof and turn_proof guarantee move validity"),
    }
}

// ── Invariant Predicate ───────────────────────────────────────────────────────

/// Structural invariant predicate for [`TicTacToeState`].
///
/// Runtime-evaluable form of [`crate::contracts::TicTacToeConsistent`] used by
/// Kani `#[kani::requires]` / `#[kani::ensures]` in contracted transition
/// harnesses.  Each variant enforces the structural guarantee expected of
/// well-formed tic-tac-toe state:
///
/// - `Setup`: the board is completely empty — no moves have been played yet.
/// - `InProgress`: move history has not exceeded the 9-square board capacity.
/// - `Finished`: at least 5 moves were played — the minimum to win.
///
/// Wired to the machine via
/// `#[prop(kani_invariant_fn = "tic_tac_toe_consistent", ...)]` on
/// [`crate::contracts::TicTacToeConsistent`].
#[cfg(kani)]
pub fn tic_tac_toe_consistent(state: &TicTacToeState) -> bool {
    match state {
        TicTacToeState::Setup { inner, .. } => {
            // Board must be completely empty at setup.
            inner
                .board()
                .squares()
                .iter()
                .all(|s| *s == crate::Square::Empty)
        }
        TicTacToeState::InProgress { inner, .. } => {
            // History must not exceed the 9-square board capacity.
            inner.history().len() <= 9
        }
        TicTacToeState::Finished { inner, .. } => {
            // A finished game needs at least 5 moves: X wins in 5 (3+2),
            // and a draw fills all 9 squares.
            inner.history().len() >= 5
        }
    }
}

/// Transition: restart a finished game, returning to setup.
///
/// Only valid from the `Finished` state; all other states are passed through.
#[formal_method(contracts = [TicTacToeConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn ttt_restart(
    state: TicTacToeState,
    proof: Established<TicTacToeConsistent>,
) -> (TicTacToeState, Established<TicTacToeConsistent>) {
    let TicTacToeState::Finished {
        inner: finished,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    (
        TicTacToeState::Setup {
            inner: finished.restart(),
            display_mode,
        },
        proof,
    )
}
