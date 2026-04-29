//! Verified State Machine for craps.
//!
//! This module is the derive-only consumer layer.  No manual `impl` blocks
//! live here — all proof infrastructure is wired up via the macros.
//!
//! ## State diagram
//!
//! ```text
//!                  craps_start_betting
//! Setup ───────────────────────────────► Betting
//!                                           │
//!                                  craps_place_bets
//!                                           │
//!                                           ▼
//!                                        ComeOut
//!                                           │
//!                               craps_comeout_roll
//!                                           │
//!                          ┌────────────────┴──────────────────┐
//!                          ▼                                   ▼
//!                      PointPhase ──craps_point_roll──► PointPhase
//!                          │                                   │
//!                          └──────────────┬────────────────────┘
//!                                         │ (seven-out or point made)
//!                                         ▼
//!                                      Resolved
//!                                         │
//!                               craps_next_round
//!                                         │
//!                                         ▼
//!                                       Betting
//! ```

use crate::contracts::{CrapsConsistent, CrapsRulesEvidence};
use crate::typestate::{ComeOutResult, GameSetup, PointRollResult};
use crate::workflow::BetsPlaced;
use crate::{ActiveBet, DiceRoll, GameBetting, GameComeOut, GamePointPhase, GameResolved};
use elicitation::contracts::Established;
use elicitation::{Elicit, KaniCompose, KaniVariantState, VerifiedStateMachine, formal_method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(not(kani))]
use tracing::instrument;

// ── CrapsState ────────────────────────────────────────────────────────────────

/// State enum for the craps verified state machine.
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
pub enum CrapsState {
    /// Table is in setup phase, awaiting bankroll initialisation.
    Setup(GameSetup),
    /// Players are placing bets before the come-out roll.
    Betting(GameBetting),
    /// Come-out roll phase — shooter's first roll of the round.
    ComeOut(GameComeOut),
    /// Point has been established; shooter keeps rolling.
    PointPhase(GamePointPhase),
    /// Round has resolved (natural, craps, point made, or seven-out).
    Resolved(GameResolved),
}

// ── CrapsMachine ──────────────────────────────────────────────────────────────

/// Verified state machine for craps.
#[derive(VerifiedStateMachine)]
#[vsm(transitions = [
    craps_start_betting,
    craps_place_bets,
    craps_comeout_roll,
    craps_point_roll,
    craps_next_round,
])]
pub struct CrapsMachine;

impl Default for CrapsState {
    fn default() -> Self {
        Self::Setup(GameSetup::default())
    }
}

// ── Transitions ───────────────────────────────────────────────────────────────

/// Transition: initialise bankrolls and move from setup to betting.
///
/// Only valid from the `Setup` state; all other states are passed through
/// unchanged.
#[formal_method(contracts = [CrapsConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn craps_start_betting(
    state: CrapsState,
    proof: Established<CrapsConsistent>,
    bankrolls: Vec<u64>,
) -> (CrapsState, Established<CrapsConsistent>) {
    let CrapsState::Setup(setup) = state else {
        return (state, proof);
    };
    (CrapsState::Betting(setup.start_betting(bankrolls)), proof)
}

/// Transition: validate and place bets, moving to the come-out roll phase.
///
/// Requires a [`BetsPlaced`] proof that all seat bets are valid and funded.
/// Assembles [`CrapsRulesEvidence`] from that proof to re-issue
/// [`CrapsConsistent`] for the new state.
///
/// Only valid from the `Betting` state; all other states are passed through.
#[formal_method(contracts = [CrapsConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof, bets_proof)))]
pub fn craps_place_bets(
    state: CrapsState,
    proof: Established<CrapsConsistent>,
    seat_bets: Vec<Vec<ActiveBet>>,
    bets_proof: Established<BetsPlaced>,
) -> (CrapsState, Established<CrapsConsistent>) {
    let CrapsState::Betting(betting) = state else {
        return (state, proof);
    };
    let new_proof = Established::prove(&CrapsRulesEvidence {
        bets_placed: bets_proof,
    });
    (
        CrapsState::ComeOut(betting.start_comeout(seat_bets)),
        new_proof,
    )
}

/// Transition: execute the come-out roll, establishing the point or resolving.
///
/// A 7 or 11 (natural) or 2/3/12 (craps) resolves immediately to `Resolved`.
/// Any other value (4, 5, 6, 8, 9, 10) establishes the point → `PointPhase`.
///
/// Only valid from the `ComeOut` state; all other states are passed through.
#[formal_method(contracts = [CrapsConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn craps_comeout_roll(
    state: CrapsState,
    proof: Established<CrapsConsistent>,
    dice: DiceRoll,
) -> (CrapsState, Established<CrapsConsistent>) {
    let CrapsState::ComeOut(comeout) = state else {
        return (state, proof);
    };
    match comeout.roll(dice) {
        ComeOutResult::PointSet(pp) => (CrapsState::PointPhase(pp), proof),
        ComeOutResult::Resolved(r) => (CrapsState::Resolved(r), proof),
    }
}

/// Transition: execute a point-phase roll.
///
/// If the shooter rolls the point or a seven, the round resolves → `Resolved`.
/// Any other value keeps the round in `PointPhase`.
///
/// Only valid from the `PointPhase` state; all other states are passed through.
#[formal_method(contracts = [CrapsConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn craps_point_roll(
    state: CrapsState,
    proof: Established<CrapsConsistent>,
    dice: DiceRoll,
) -> (CrapsState, Established<CrapsConsistent>) {
    let CrapsState::PointPhase(pp) = state else {
        return (state, proof);
    };
    match pp.roll(dice) {
        PointRollResult::Continue(pp2) => (CrapsState::PointPhase(pp2), proof),
        PointRollResult::Resolved(r) => (CrapsState::Resolved(r), proof),
    }
}

/// Transition: settle the resolved round and start the next betting phase.
///
/// Rotates the shooter index and resets bets with updated bankrolls.
///
/// Only valid from the `Resolved` state; all other states are passed through.
#[formal_method(contracts = [CrapsConsistent])]
#[cfg_attr(not(kani), instrument(skip(proof)))]
pub fn craps_next_round(
    state: CrapsState,
    proof: Established<CrapsConsistent>,
    updated_bankrolls: Vec<u64>,
) -> (CrapsState, Established<CrapsConsistent>) {
    let CrapsState::Resolved(resolved) = state else {
        return (state, proof);
    };
    (
        CrapsState::Betting(resolved.next_round(updated_bankrolls)),
        proof,
    )
}
