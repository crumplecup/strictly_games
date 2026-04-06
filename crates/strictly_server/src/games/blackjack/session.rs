//! Per-connection blackjack game phase state.

use std::sync::Arc;
use strictly_blackjack::{GameBetting, GameFinished, GamePlayerTurn};
use tokio::sync::Mutex;

/// The current phase of a single agent's blackjack session.
///
/// Stored per-connection (each HTTP connection gets its own `GameServer`).
/// Transitions are driven by the dynamic tool handlers in [`super::factories`].
pub enum BlackjackPhase {
    /// No game in progress.
    Idle,
    /// Waiting for the player to place a bet.
    Betting(Box<GameBetting>),
    /// Player is taking actions (hit / stand / …).
    PlayerTurn(Box<GamePlayerTurn>),
    /// Hand finished; player may deal again or cash out.
    ///
    /// Dealer turn is auto-played before transitioning here.
    /// Bankroll is propagated via [`super::factories::NextContext`], not stored here.
    Finished,
}

/// Shared, async-safe handle to the phase state for a single connection.
pub type BlackjackSession = Arc<Mutex<BlackjackPhase>>;

/// Create a new session in the `Idle` phase.
pub fn new_session() -> BlackjackSession {
    Arc::new(Mutex::new(BlackjackPhase::Idle))
}

/// Format a player-turn state for the agent.
pub fn describe_player_turn(game: &GamePlayerTurn) -> String {
    let hand = &game.player_hands()[game.current_hand_index()];
    let dealer_card = &game.dealer_hand().cards()[0];
    format!(
        "Your hand: {} (value: {})\nDealer shows: {}\n",
        hand.display(),
        hand.value().best(),
        dealer_card
    )
}

/// Format a finished-game state for the agent.
pub fn describe_finished(game: &GameFinished) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "Dealer's hand: {}\n",
        game.dealer_hand().display()
    ));
    for (i, (hand, outcome)) in game
        .player_hands()
        .iter()
        .zip(game.outcomes().iter())
        .enumerate()
    {
        s.push_str(&format!(
            "Hand {}: {} — {}\n",
            i + 1,
            hand.display(),
            outcome
        ));
        let payout = outcome.calculate_payout(game.bets()[i]);
        if payout > 0 {
            s.push_str(&format!("Won: ${payout}\n"));
        } else if payout < 0 {
            s.push_str(&format!("Lost: ${}\n", payout.unsigned_abs()));
        } else {
            s.push_str("Push\n");
        }
    }
    s.push_str(&format!("💰 Bankroll: ${}\n", game.bankroll()));
    s
}
