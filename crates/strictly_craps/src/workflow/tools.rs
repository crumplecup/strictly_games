//! Craps workflow tools with proof-carrying contracts.
//!
//! Each free function carries explicit `Established<P>` precondition proofs
//! and returns `Established<Q>` postcondition proofs. The compiler enforces
//! the correct call order.
//!
//! | Function | Pre | Post | Description |
//! |---|---|---|---|
//! | [`execute_place_bets`] | `True` | [`BetsPlaced`] | Validates bets, deducts from bankrolls |
//! | [`execute_comeout_roll`] | [`BetsPlaced`] | [`PointEstablished`] or [`RoundSettled`] | Rolls come-out, classifies result |
//! | [`execute_point_roll`] | [`PointEstablished`] | [`PointEstablished`] or [`RoundSettled`] | Rolls during point phase |

use elicitation::contracts::Established;
use tracing::instrument;

use crate::ledger::RoundSettled;
use crate::typestate::{
    ComeOutResult, GameBetting, GameComeOut, GamePointPhase, GameResolved, PointRollResult,
};
use crate::{ActiveBet, CrapsError, CrapsErrorKind};

use super::propositions::{BetsPlaced, PointEstablished};

// ── PlaceBets ────────────────────────────────────────────────────────────────

/// Execute the bet-placement step.
///
/// Validates all bets against current bankrolls and transitions to come-out.
///
/// **Pre:** (none — `True` assumed by caller)
/// **Post:** [`BetsPlaced`]
#[instrument(skip(betting, seat_bets))]
pub fn execute_place_bets(
    betting: GameBetting,
    seat_bets: Vec<Vec<ActiveBet>>,
) -> Result<(GameComeOut, Established<BetsPlaced>), CrapsError> {
    // Validate: each seat's total wager must not exceed bankroll
    for (i, bets) in seat_bets.iter().enumerate() {
        let total: u64 = bets.iter().map(|b| b.amount()).sum();
        let bankroll = betting.bankrolls().get(i).copied().unwrap_or(0);
        if total > bankroll {
            return Err(CrapsErrorKind::InsufficientFunds {
                need: total,
                have: bankroll,
            }
            .into());
        }
        // Validate no zero bets
        for bet in bets {
            if bet.amount() == 0 {
                return Err(CrapsErrorKind::InvalidBet(0).into());
            }
        }
    }

    let comeout = betting.start_comeout(seat_bets);
    Ok((comeout, Established::assert()))
}

// ── ComeOutRoll ──────────────────────────────────────────────────────────────

/// Output of [`execute_comeout_roll`].
pub enum ComeOutOutput {
    /// Point was established — carry proof forward.
    PointSet(GamePointPhase, Established<PointEstablished>),
    /// Natural or craps — round resolved immediately.
    Resolved(GameResolved, Established<RoundSettled>),
}

/// Execute the come-out roll.
///
/// **Pre:** [`BetsPlaced`]
/// **Post:** [`PointEstablished`] on point, [`RoundSettled`] on natural/craps
#[instrument(skip(comeout, _pre))]
pub fn execute_comeout_roll(
    comeout: GameComeOut,
    roll: crate::DiceRoll,
    _pre: Established<BetsPlaced>,
) -> ComeOutOutput {
    match comeout.roll(roll) {
        ComeOutResult::PointSet(point_phase) => {
            ComeOutOutput::PointSet(point_phase, Established::assert())
        }
        ComeOutResult::Resolved(resolved) => {
            ComeOutOutput::Resolved(resolved, Established::assert())
        }
    }
}

// ── PointRoll ────────────────────────────────────────────────────────────────

/// Output of [`execute_point_roll`].
pub enum PointRollOutput {
    /// Roll did not resolve — carry proof forward for next roll.
    Continue(GamePointPhase, Established<PointEstablished>),
    /// Point made or seven-out — round resolved.
    Resolved(GameResolved, Established<RoundSettled>),
}

/// Execute a point-phase roll.
///
/// **Pre:** [`PointEstablished`]
/// **Post:** [`PointEstablished`] on continue, [`RoundSettled`] on resolution
#[instrument(skip(point_phase, _pre))]
pub fn execute_point_roll(
    point_phase: GamePointPhase,
    roll: crate::DiceRoll,
    _pre: Established<PointEstablished>,
) -> PointRollOutput {
    match point_phase.roll(roll) {
        PointRollResult::Continue(next) => PointRollOutput::Continue(next, Established::assert()),
        PointRollResult::Resolved(resolved) => {
            PointRollOutput::Resolved(resolved, Established::assert())
        }
    }
}
