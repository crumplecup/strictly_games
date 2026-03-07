//! Core domain types for blackjack outcomes.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};

/// Outcome of a blackjack hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit)]
pub enum Outcome {
    /// Player won (hand closer to 21 than dealer).
    Win,
    /// Player got blackjack (natural 21 with 2 cards).
    Blackjack,
    /// Push (tie with dealer).
    Push,
    /// Player lost (dealer closer to 21, or player bust).
    Loss,
    /// Player surrendered (forfeited half bet).
    Surrender,
}

impl Outcome {
    /// Returns the payout multiplier for this outcome.
    ///
    /// - Win: 1.0 (return bet + 1x)
    /// - Blackjack: 1.5 (return bet + 1.5x)
    /// - Push: 0.0 (return bet)
    /// - Loss: -1.0 (lose bet)
    /// - Surrender: -0.5 (lose half bet)
    pub fn payout_multiplier(self) -> f64 {
        match self {
            Outcome::Win => 1.0,
            Outcome::Blackjack => 1.5,
            Outcome::Push => 0.0,
            Outcome::Loss => -1.0,
            Outcome::Surrender => -0.5,
        }
    }

    /// Calculates the payout for a given bet and outcome.
    ///
    /// Returns the net change in bankroll (positive for win, negative for loss).
    pub fn calculate_payout(self, bet: u64) -> i64 {
        let bet_i64 = bet as i64;
        match self {
            Outcome::Win => bet_i64,
            Outcome::Blackjack => (bet_i64 * 3) / 2, // 3:2 payout
            Outcome::Push => 0,
            Outcome::Loss => -bet_i64,
            Outcome::Surrender => -bet_i64 / 2,
        }
    }

    /// Returns true if the player won or got blackjack.
    pub fn is_win(self) -> bool {
        matches!(self, Outcome::Win | Outcome::Blackjack)
    }

    /// Returns true if the player lost.
    pub fn is_loss(self) -> bool {
        matches!(self, Outcome::Loss | Outcome::Surrender)
    }

    /// Returns true if the outcome was a push.
    pub fn is_push(self) -> bool {
        matches!(self, Outcome::Push)
    }
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Win => write!(f, "Win"),
            Outcome::Blackjack => write!(f, "Blackjack!"),
            Outcome::Push => write!(f, "Push"),
            Outcome::Loss => write!(f, "Loss"),
            Outcome::Surrender => write!(f, "Surrender"),
        }
    }
}
