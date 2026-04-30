//! Verified State Machine for blackjack.
//!
//! This module is the derive-only consumer layer.  No manual `impl` blocks
//! live here — all proof infrastructure is wired up via the macros.
//!
//! ## State diagram
//!
//! ```text
//!                       bj_start_betting
//! Setup ───────────────────────────────► Betting
//!   ▲                                       │
//!   │                               bj_place_bet
//!   │                                       │
//!   │                          ┌────────────┴───────────────┐
//!   │                          ▼                            ▼
//!   │                     PlayerTurn ──bj_player_action──► DealerTurn
//!   │                          │                            │
//!   │                   (immediate                  bj_dealer_turn
//!   │                   blackjack/bust)                     │
//!   │                          │                            ▼
//!   │                          └──────────────────────► Finished
//!   └──────────────────────────────bj_restart──────────────┘
//! ```

use crate::contracts::{BlackjackConsistent, BlackjackRulesEvidence, NotBust, ValidAction};
use crate::display::BlackjackDisplayMode;
use crate::typestate::{GameFinished, GamePlayerTurn, GameResult, GameSetup};
use crate::{BasicAction, GameBetting, GameDealerTurn};
use elicitation::contracts::Established;
use elicitation::{Elicit, KaniCompose, KaniVariantState, VerifiedStateMachine, formal_method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(not(kani))]
use tracing::instrument;

// ── BlackjackState ────────────────────────────────────────────────────────────

/// State enum for the blackjack verified state machine.
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
pub enum BlackjackState {
    /// Game is in setup phase, awaiting the first player.
    Setup {
        /// Inner setup state.
        inner: GameSetup,
        /// Active display mode.
        display_mode: BlackjackDisplayMode,
    },
    /// Game is in betting phase — player places wager.
    Betting {
        /// Inner betting state.
        inner: GameBetting,
        /// Active display mode.
        display_mode: BlackjackDisplayMode,
    },
    /// Game is in player turn phase — player takes actions.
    PlayerTurn {
        /// Inner player-turn state.
        inner: GamePlayerTurn,
        /// Active display mode.
        display_mode: BlackjackDisplayMode,
    },
    /// Game is in dealer turn phase — dealer plays by fixed rules.
    DealerTurn {
        /// Inner dealer-turn state.
        inner: GameDealerTurn,
        /// Active display mode.
        display_mode: BlackjackDisplayMode,
    },
    /// Game is finished — outcomes determined, ready for restart.
    Finished {
        /// Inner finished state.
        inner: GameFinished,
        /// Active display mode.
        display_mode: BlackjackDisplayMode,
    },
}

// ── BlackjackMachine ──────────────────────────────────────────────────────────

/// Verified state machine for blackjack.
#[derive(VerifiedStateMachine)]
#[vsm(transitions = [
    bj_start_betting,
    bj_place_bet,
    bj_player_action,
    bj_dealer_turn,
    bj_restart,
])]
pub struct BlackjackMachine;

impl Default for BlackjackState {
    fn default() -> Self {
        Self::Setup {
            inner: GameSetup::default(),
            display_mode: BlackjackDisplayMode::default(),
        }
    }
}

// ── Transitions ───────────────────────────────────────────────────────────────

/// Transition: initialise the bankroll and move from setup to betting.
///
/// Only valid from the `Setup` state; all other states are passed through.
#[formal_method(contracts = [BlackjackConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn bj_start_betting(
    state: BlackjackState,
    proof: Established<BlackjackConsistent>,
    initial_bankroll: u64,
) -> (BlackjackState, Established<BlackjackConsistent>) {
    let BlackjackState::Setup {
        inner: setup,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    (
        BlackjackState::Betting {
            inner: setup.start_betting(initial_bankroll),
            display_mode,
        },
        proof,
    )
}

/// Transition: place a bet and deal initial cards.
///
/// Transitions `Betting → PlayerTurn`, `Betting → DealerTurn` (rare: dealer
/// blackjack path), or `Betting → Finished` (immediate blackjack resolution).
/// If the bet is invalid or the shoe is exhausted the state is unchanged.
///
/// Only valid from the `Betting` state; all other states are passed through.
#[formal_method(contracts = [BlackjackConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn bj_place_bet(
    state: BlackjackState,
    proof: Established<BlackjackConsistent>,
    bet: u64,
) -> (BlackjackState, Established<BlackjackConsistent>) {
    let BlackjackState::Betting {
        inner: betting,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    let fallback = betting.clone();
    match betting.place_bet(bet) {
        Ok(GameResult::PlayerTurn(pt)) => (
            BlackjackState::PlayerTurn {
                inner: pt,
                display_mode,
            },
            proof,
        ),
        Ok(GameResult::DealerTurn(dt)) => (
            BlackjackState::DealerTurn {
                inner: dt,
                display_mode,
            },
            proof,
        ),
        Ok(GameResult::Finished(finished, _settled)) => (
            BlackjackState::Finished {
                inner: finished,
                display_mode,
            },
            proof,
        ),
        Err(_) => (
            BlackjackState::Betting {
                inner: fallback,
                display_mode,
            },
            proof,
        ),
    }
}

/// Transition: apply a validated player action to an in-progress hand.
///
/// Requires individual proofs for each precondition:
/// - `valid_proof`: action targets a valid hand index and it is the player's
///   turn (`Established<ValidAction>`)
/// - `bust_proof`: the targeted hand is not bust (`Established<NotBust>`)
///
/// These are assembled into a [`BlackjackRulesEvidence`] bundle to re-issue
/// the [`BlackjackConsistent`] invariant after the action.
///
/// Only valid from the `PlayerTurn` state; all other states are passed through.
#[formal_method(contracts = [BlackjackConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof, valid_proof, bust_proof)))]
pub fn bj_player_action(
    state: BlackjackState,
    proof: Established<BlackjackConsistent>,
    action: BasicAction,
    valid_proof: Established<ValidAction>,
    bust_proof: Established<NotBust>,
) -> (BlackjackState, Established<BlackjackConsistent>) {
    let BlackjackState::PlayerTurn {
        inner: pt,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    let new_proof = Established::prove(&BlackjackRulesEvidence {
        valid_action: valid_proof,
        not_bust: bust_proof,
    });
    match pt.action_on_current(action) {
        Ok(GameResult::PlayerTurn(pt2)) => (
            BlackjackState::PlayerTurn {
                inner: pt2,
                display_mode,
            },
            new_proof,
        ),
        Ok(GameResult::DealerTurn(dt)) => (
            BlackjackState::DealerTurn {
                inner: dt,
                display_mode,
            },
            new_proof,
        ),
        Ok(GameResult::Finished(finished, _settled)) => (
            BlackjackState::Finished {
                inner: finished,
                display_mode,
            },
            new_proof,
        ),
        Err(_) => unreachable!("valid_proof and bust_proof guarantee action validity"),
    }
}

/// Transition: play the dealer's hand to completion and settle outcomes.
///
/// Consumes `DealerTurn` and produces `Finished`.
/// Only valid from the `DealerTurn` state; all other states are passed through.
#[formal_method(contracts = [BlackjackConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn bj_dealer_turn(
    state: BlackjackState,
    proof: Established<BlackjackConsistent>,
) -> (BlackjackState, Established<BlackjackConsistent>) {
    let BlackjackState::DealerTurn {
        inner: dt,
        display_mode,
    } = state
    else {
        return (state, proof);
    };
    let (finished, _settled) = dt.play_dealer_turn();
    (
        BlackjackState::Finished {
            inner: finished,
            display_mode,
        },
        proof,
    )
}

/// Transition: restart a finished game, returning to setup.
///
/// Only valid from the `Finished` state; all other states are passed through.
#[formal_method(contracts = [BlackjackConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn bj_restart(
    state: BlackjackState,
    proof: Established<BlackjackConsistent>,
) -> (BlackjackState, Established<BlackjackConsistent>) {
    let BlackjackState::Finished { .. } = state else {
        return (state, proof);
    };
    (
        BlackjackState::Setup {
            inner: GameSetup::default(),
            display_mode: BlackjackDisplayMode::default(),
        },
        proof,
    )
}
