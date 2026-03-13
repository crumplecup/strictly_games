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
//! | [`execute_place_bet`] | `True` (implicit) | [`BetPlaced`] or [`PayoutSettled`] | Validates bet, deals initial cards |
//! | [`execute_play_action`] | [`BetPlaced`] | [`PlayerTurnComplete`] or recycled [`BetPlaced`] | Applies one player action |
//! | [`execute_dealer_turn`] | [`PlayerTurnComplete`] | [`PayoutSettled`] | Plays dealer and settles payout via BankrollLedger |

use elicitation::contracts::Established;
use tracing::instrument;

use crate::games::blackjack::{
    ActionError, BasicAction, GameBetting, GameDealerTurn, GameFinished, GamePlayerTurn,
    GameResult, PlayerAction,
};

use super::propositions::{BetPlaced, PayoutSettled, PlayerTurnComplete};

// ── PlaceBetTool ─────────────────────────────────────────────────────────────

/// Output of [`execute_place_bet`]: either normal player turn or an instant
/// finish with settlement proof.
///
/// All instant-finish paths (player natural, dealer natural, both naturals)
/// go through [`BankrollLedger::settle`] inside `place_bet`, so `Finished`
/// carries `Established<PayoutSettled>` — the compiler proves settlement ran.
pub enum PlaceBetOutput {
    /// Normal play continues — player must take actions.
    PlayerTurn(GamePlayerTurn, Established<BetPlaced>),
    /// Game ended immediately — carries proof that settlement occurred.
    Finished(GameFinished, Established<PayoutSettled>),
}

/// Execute the bet-placement step with a pre-elicited bet amount.
///
/// Validates the bet against the current bankroll, deducts it, deals initial
/// cards, and returns the next game state together with the appropriate proof.
///
/// - Normal path: returns `PlayerTurn` with `Established<BetPlaced>`
/// - Fast-finish (natural blackjack / dealer natural): returns `Finished`
///   with `Established<PayoutSettled>` — settlement already ran inside `place_bet`
///
/// **Pre:** (none — `True` assumed by caller)
/// **Post:** [`BetPlaced`] on normal path, [`PayoutSettled`] on fast-finish
#[instrument(skip(betting))]
pub fn execute_place_bet(
    betting: GameBetting,
    bet: u64,
) -> Result<PlaceBetOutput, ActionError> {
    let result = betting.place_bet(bet)?;
    let output = match result {
        GameResult::PlayerTurn(pt) => PlaceBetOutput::PlayerTurn(pt, Established::assert()),
        GameResult::Finished(f, settled) => PlaceBetOutput::Finished(f, settled),
        GameResult::DealerTurn(_) => {
            // place_bet never emits DealerTurn — all dealer-natural paths return
            // Finished with the settlement proof already embedded.  This arm is
            // statically unreachable but required by exhaustive matching.
            unreachable!("place_bet never emits GameResult::DealerTurn")
        }
    };
    Ok(output)
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
        GameResult::Finished(f, _settled) => Ok(PlayActionResult::Complete(
            PlayActionOutput::Finished(f),
            Established::assert(),
        )),
    }
}

// ── DealerTurnTool ────────────────────────────────────────────────────────────

/// Execute the dealer turn.
///
/// **Pre:** [`PlayerTurnComplete`]
/// **Post:** [`PayoutSettled`] — proof that `BankrollLedger::settle` ran with
/// a valid `BetDeducted` token; the final bankroll is arithmetically correct.
#[instrument(skip(dealer_turn, _pre))]
pub fn execute_dealer_turn(
    dealer_turn: GameDealerTurn,
    _pre: Established<PlayerTurnComplete>,
) -> (GameFinished, Established<PayoutSettled>) {
    dealer_turn.play_dealer_turn()
}
