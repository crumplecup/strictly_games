//! Per-connection blackjack game phase state.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use strictly_blackjack::{GameBetting, GameFinished, GamePlayerTurn};
use tokio::sync::Mutex;

/// The current phase of a single agent's blackjack session.
///
/// Stored per-connection (each HTTP connection gets its own `GameServer`).
/// Transitions are driven by the dynamic tool handlers in [`super::factories`].
#[derive(Debug)]
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

/// Serializable snapshot of the current blackjack phase for REST observers.
///
/// Polled by the TUI spectator loop to render the game state without sharing
/// process memory with the MCP handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackStateView {
    /// Current phase: `"idle"`, `"betting"`, `"player_turn"`, or `"finished"`.
    pub phase: String,
    /// Player's bankroll in the current phase (0 when idle or finished).
    pub bankroll: u64,
    /// Human-readable summary of the game state for TUI display.
    pub description: String,
    /// True when the session has ended (agent cashed out).
    pub is_terminal: bool,
}

impl BlackjackStateView {
    /// Builds a view from the current phase lock guard.
    pub fn from_phase(phase: &BlackjackPhase) -> Self {
        match phase {
            BlackjackPhase::Idle => Self {
                phase: "idle".to_string(),
                bankroll: 0,
                description: "No active session.".to_string(),
                is_terminal: true,
            },
            BlackjackPhase::Betting(game) => {
                let bankroll = game.bankroll();
                Self {
                    phase: "betting".to_string(),
                    bankroll,
                    description: format!("💰 Bankroll: ${bankroll}\n\nWaiting for bet..."),
                    is_terminal: false,
                }
            }
            BlackjackPhase::PlayerTurn(game) => {
                let bankroll = game.bankroll();
                Self {
                    phase: "player_turn".to_string(),
                    bankroll,
                    description: describe_player_turn(game),
                    is_terminal: false,
                }
            }
            BlackjackPhase::Finished => Self {
                phase: "finished".to_string(),
                bankroll: 0,
                description: "Hand complete. Awaiting next decision...".to_string(),
                is_terminal: false,
            },
        }
    }
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
