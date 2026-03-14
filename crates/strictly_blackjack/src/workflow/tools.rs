//! Blackjack workflow tools with proof-carrying contracts.
//!
//! Each free function carries explicit `Established<P>` precondition proofs
//! and returns `Established<Q>` postcondition proofs.  The compiler enforces
//! the correct call order.
//!
//! | Function | Pre | Post | Description |
//! |---|---|---|---|
//! | [`execute_place_bet`] | `True` | [`BetPlaced`] or [`PayoutSettled`] | Validates bet, deals initial cards |
//! | [`execute_play_action`] | [`BetPlaced`] | [`PlayerTurnComplete`] or recycled [`BetPlaced`] | Applies one player action |
//! | [`execute_dealer_turn`] | [`PlayerTurnComplete`] | [`PayoutSettled`] | Plays dealer and settles payout |

use elicitation::contracts::Established;
use tracing::instrument;

use crate::{
    ActionError, BasicAction, GameBetting, GameDealerTurn, GameFinished, GamePlayerTurn,
    GameResult, PayoutSettled, PlayerAction,
};

use super::propositions::{BetPlaced, PlayerTurnComplete};

// ── PlaceBet ─────────────────────────────────────────────────────────────────

/// Output of [`execute_place_bet`]: either normal player turn or instant finish.
///
/// All instant-finish paths (player natural, dealer natural, both naturals) go
/// through [`crate::BankrollLedger::settle`] inside `place_bet`, so `Finished`
/// carries `Established<PayoutSettled>` — the compiler proves settlement ran.
pub enum PlaceBetOutput {
    /// Normal play continues — player must take actions.
    PlayerTurn(GamePlayerTurn, Established<BetPlaced>),
    /// Game ended immediately — carries proof that settlement occurred.
    Finished(GameFinished, Established<PayoutSettled>),
}

/// Execute the bet-placement step.
///
/// Validates the bet against the current bankroll, deducts it, deals initial
/// cards, and returns the next game state together with the appropriate proof.
///
/// **Pre:** (none — `True` assumed by caller)
/// **Post:** [`BetPlaced`] on normal path, [`PayoutSettled`] on fast-finish
#[instrument(skip(betting))]
pub fn execute_place_bet(betting: GameBetting, bet: u64) -> Result<PlaceBetOutput, ActionError> {
    let result = betting.place_bet(bet)?;
    let output = match result {
        GameResult::PlayerTurn(pt) => PlaceBetOutput::PlayerTurn(pt, Established::assert()),
        GameResult::Finished(f, settled) => PlaceBetOutput::Finished(f, settled),
        GameResult::DealerTurn(_) => {
            // place_bet never emits DealerTurn — statically unreachable.
            unreachable!("place_bet never emits GameResult::DealerTurn")
        }
    };
    Ok(output)
}

// ── PlayAction ────────────────────────────────────────────────────────────────

/// Output of [`execute_play_action`] when the hand is over.
pub enum PlayActionOutput {
    /// Dealer must play.
    DealerTurn(GameDealerTurn),
    /// Game over immediately (e.g. bust).
    Finished(GameFinished),
}

/// Result of one [`execute_play_action`] call.
pub enum PlayActionResult {
    /// Hand still ongoing — carry the `BetPlaced` proof forward.
    InProgress(GamePlayerTurn, Established<BetPlaced>),
    /// Player turn complete — carry the `PlayerTurnComplete` proof forward.
    Complete(PlayActionOutput, Established<PlayerTurnComplete>),
}

/// Apply one player action.
///
/// Returns either `InProgress` (hand continues, recycles [`BetPlaced`]) or
/// `Complete` (player turn finished, establishes [`PlayerTurnComplete`]).
///
/// **Pre:** [`BetPlaced`]
/// **Post:** [`PlayerTurnComplete`] on terminal action, [`BetPlaced`] on Hit
#[instrument(skip(player_turn, _pre))]
pub fn execute_play_action(
    player_turn: GamePlayerTurn,
    action: BasicAction,
    _pre: Established<BetPlaced>,
) -> Result<PlayActionResult, ActionError> {
    let pa = PlayerAction::new(action, player_turn.current_hand_index());
    let result = player_turn.take_action(pa)?;

    match result {
        GameResult::PlayerTurn(next) => {
            Ok(PlayActionResult::InProgress(next, Established::assert()))
        }
        GameResult::DealerTurn(dt) => Ok(PlayActionResult::Complete(
            PlayActionOutput::DealerTurn(dt),
            Established::assert(),
        )),
        GameResult::Finished(f, _settled) => Ok(PlayActionResult::Complete(
            PlayActionOutput::Finished(f),
            Established::assert(),
        )),
    }
}

// ── DealerTurn ────────────────────────────────────────────────────────────────

/// Execute the dealer turn.
///
/// **Pre:** [`PlayerTurnComplete`]
/// **Post:** [`PayoutSettled`] — proof that [`crate::BankrollLedger::settle`]
/// ran with a valid [`crate::BetDeducted`] token.
#[instrument(skip(dealer_turn, _pre))]
pub fn execute_dealer_turn(
    dealer_turn: GameDealerTurn,
    _pre: Established<PlayerTurnComplete>,
) -> (GameFinished, Established<PayoutSettled>) {
    dealer_turn.play_dealer_turn()
}
