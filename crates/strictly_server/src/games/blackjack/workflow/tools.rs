//! Blackjack workflow tools with proof-carrying contracts.
//!
//! Each free function carries explicit `Established<P>` precondition proofs
//! and returns `Established<Q>` postcondition proofs.  The compiler enforces
//! the correct call order — you cannot call `execute_play_action` without
//! first having `Established<BetPlaced>`.
//!
//! # Function summary
//!
//! | Function | Pre | Post | Description |
//! |----------|-----|------|-------------|
//! | [`execute_place_bet`] | `True` (implicit) | [`BetPlaced`] | Validates bet, deals initial cards |
//! | [`execute_play_action`] | [`BetPlaced`] | [`PlayerTurnComplete`] or recycled [`BetPlaced`] | Applies one player action |
//! | [`execute_dealer_turn`] | [`PlayerTurnComplete`] | [`HandResolved`] | Plays dealer and resolves outcomes |

use elicitation::contracts::Established;
use tracing::instrument;

use crate::games::blackjack::{
    ActionError, BasicAction, GameDealerTurn, GameFinished, GamePlayerTurn, GameResult,
    PlayerAction, GameBetting,
};

use super::propositions::{BetPlaced, HandResolved, PlayerTurnComplete};

// ── PlaceBetTool ─────────────────────────────────────────────────────────────

/// Output of [`execute_place_bet`]: either a `PlayerTurn` or an instant
/// `Finished` result (natural blackjack / dealer natural).
pub enum PlaceBetOutput {
    /// Normal play continues — player must take actions.
    PlayerTurn(GamePlayerTurn),
    /// Game ended immediately (natural blackjack or dealer natural).
    Finished(GameFinished),
}

/// Execute the bet-placement step with a pre-elicited bet amount.
///
/// Validates the bet against the current bankroll, deducts it, deals initial
/// cards, and returns the next game state together with `Established<BetPlaced>`.
///
/// The bet amount is elicited by the caller (`BlackjackWorkflow`) before this
/// call so that the communicator interaction is cleanly separated from the
/// game-logic transition.
///
/// **Pre:** (none — `True` assumed by caller)
/// **Post:** [`BetPlaced`]
#[instrument(skip(betting))]
pub fn execute_place_bet(
    betting: GameBetting,
    bet: u64,
) -> Result<(PlaceBetOutput, Established<BetPlaced>), ActionError> {
    let result = betting.place_bet(bet)?;
    let output = match result {
        GameResult::PlayerTurn(pt) => PlaceBetOutput::PlayerTurn(pt),
        GameResult::DealerTurn(dt) => {
            // Unusual: went straight to dealer — run dealer immediately.
            let finished = dt.play_dealer_turn();
            PlaceBetOutput::Finished(finished)
        }
        GameResult::Finished(f) => PlaceBetOutput::Finished(f),
    };
    Ok((output, Established::assert()))
}

// ── PlayActionTool ────────────────────────────────────────────────────────────

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
/// Returns either `InProgress` (hand continues, recycles `BetPlaced`) or
/// `Complete` (player turn finished, establishes `PlayerTurnComplete`).
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
        GameResult::Finished(f) => Ok(PlayActionResult::Complete(
            PlayActionOutput::Finished(f),
            Established::assert(),
        )),
    }
}

// ── DealerTurnTool ────────────────────────────────────────────────────────────

/// Execute the dealer turn.
///
/// **Pre:** [`PlayerTurnComplete`]
/// **Post:** [`HandResolved`]
#[instrument(skip(dealer_turn, _pre))]
pub fn execute_dealer_turn(
    dealer_turn: GameDealerTurn,
    _pre: Established<PlayerTurnComplete>,
) -> (GameFinished, Established<HandResolved>) {
    let finished = dealer_turn.play_dealer_turn();
    (finished, Established::assert())
}

