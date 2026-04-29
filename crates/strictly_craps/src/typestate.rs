//! Phase-specific typestate structs for craps.
//!
//! Each phase is a distinct type with phase-specific fields.
//! This encodes invariants at compile time — you cannot roll dice
//! without first having placed bets, and you cannot settle without
//! a resolved round.
//!
//! # Phase chain
//!
//! ```text
//! GameSetup → GameBetting → GameComeOut → GamePointPhase → GameResolved
//!                              │                              │
//!                              └── Natural/Craps ────────────►│
//! ```

use elicitation::Elicit;
use serde::{Deserialize, Serialize};

use crate::{ActiveBet, DiceRoll, Point};

/// Maximum number of active bets per seat (for Kani bounded verification).
pub const MAX_BETS_PER_SEAT: usize = 20;

/// Maximum number of seats at a craps table.
pub const MAX_SEATS: usize = 8;

/// Maximum number of rolls in a single round (for Kani bounded verification).
pub const MAX_ROLLS_PER_ROUND: usize = 100;

// ─────────────────────────────────────────────────────────────
//  Setup Phase
// ─────────────────────────────────────────────────────────────

/// Game in setup phase — table is being configured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
#[cfg_attr(kani, derive(elicitation::KaniCompose))]
pub struct GameSetup {
    /// Number of seats at the table.
    num_seats: usize,
    /// Maximum odds multiple allowed.
    max_odds: u8,
}

impl GameSetup {
    /// Creates a new game setup.
    pub fn new(num_seats: usize, max_odds: u8) -> Self {
        Self {
            num_seats: num_seats.min(MAX_SEATS),
            max_odds,
        }
    }

    /// Returns the number of seats.
    pub fn num_seats(&self) -> usize {
        self.num_seats
    }

    /// Returns the maximum odds multiple.
    pub fn max_odds(&self) -> u8 {
        self.max_odds
    }

    /// Transitions to the betting phase with initial bankrolls.
    pub fn start_betting(self, bankrolls: Vec<u64>) -> GameBetting {
        GameBetting {
            bankrolls,
            max_odds: self.max_odds,
            shooter_idx: 0,
        }
    }
}

impl Default for GameSetup {
    fn default() -> Self {
        Self::new(1, 3)
    }
}

// ─────────────────────────────────────────────────────────────
//  Betting Phase
// ─────────────────────────────────────────────────────────────

/// Betting phase — players place bets before the come-out roll.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
#[cfg_attr(kani, derive(elicitation::KaniCompose))]
pub struct GameBetting {
    /// Current bankrolls for each seat.
    bankrolls: Vec<u64>,
    /// Maximum odds multiple.
    max_odds: u8,
    /// Index of the current shooter.
    shooter_idx: usize,
}

impl GameBetting {
    /// Returns the bankrolls.
    pub fn bankrolls(&self) -> &[u64] {
        &self.bankrolls
    }

    /// Returns the maximum odds multiple.
    pub fn max_odds(&self) -> u8 {
        self.max_odds
    }

    /// Returns the shooter index.
    pub fn shooter_idx(&self) -> usize {
        self.shooter_idx
    }

    /// Transitions to the come-out phase with placed bets.
    pub fn start_comeout(self, seat_bets: Vec<Vec<ActiveBet>>) -> GameComeOut {
        GameComeOut {
            bankrolls: self.bankrolls,
            seat_bets,
            max_odds: self.max_odds,
            shooter_idx: self.shooter_idx,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Come-Out Roll Phase
// ─────────────────────────────────────────────────────────────

/// Come-out roll phase — the shooter's first roll of a new round.
///
/// A 7 or 11 is a natural (pass wins), 2/3/12 is craps (pass loses),
/// anything else establishes the point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
#[cfg_attr(kani, derive(elicitation::KaniCompose))]
pub struct GameComeOut {
    /// Current bankrolls.
    bankrolls: Vec<u64>,
    /// Active bets per seat.
    seat_bets: Vec<Vec<ActiveBet>>,
    /// Maximum odds multiple.
    max_odds: u8,
    /// Current shooter.
    shooter_idx: usize,
}

impl GameComeOut {
    /// Returns the bankrolls.
    pub fn bankrolls(&self) -> &[u64] {
        &self.bankrolls
    }

    /// Returns the active bets per seat.
    pub fn seat_bets(&self) -> &[Vec<ActiveBet>] {
        &self.seat_bets
    }

    /// Returns the shooter index.
    pub fn shooter_idx(&self) -> usize {
        self.shooter_idx
    }

    /// Returns the maximum odds multiple.
    pub fn max_odds(&self) -> u8 {
        self.max_odds
    }

    /// Processes the come-out roll.
    ///
    /// Returns either a resolved game (natural/craps) or a point phase.
    pub fn roll(self, dice: DiceRoll) -> ComeOutResult {
        if dice.is_natural() || dice.is_craps() {
            ComeOutResult::Resolved(GameResolved {
                bankrolls: self.bankrolls,
                seat_bets: self.seat_bets,
                point: None,
                roll_history: vec![dice],
                shooter_idx: self.shooter_idx,
                max_odds: self.max_odds,
            })
        } else {
            let point = dice
                .as_point()
                .expect("non-natural, non-craps roll is always a point value");
            ComeOutResult::PointSet(GamePointPhase {
                bankrolls: self.bankrolls,
                seat_bets: self.seat_bets,
                point,
                roll_history: vec![dice],
                max_odds: self.max_odds,
                shooter_idx: self.shooter_idx,
            })
        }
    }
}

/// Result of the come-out roll.
#[derive(Debug, Clone)]
pub enum ComeOutResult {
    /// Point was established — game continues.
    PointSet(GamePointPhase),
    /// Natural or craps — round resolved immediately.
    Resolved(GameResolved),
}

// ─────────────────────────────────────────────────────────────
//  Point Phase
// ─────────────────────────────────────────────────────────────

/// Point phase — a point has been established, shooter keeps rolling.
///
/// The point is immutable once set (enforced by private field).
/// Rolls continue until the point is hit or a seven-out occurs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
#[cfg_attr(kani, derive(elicitation::KaniCompose))]
pub struct GamePointPhase {
    /// Current bankrolls.
    bankrolls: Vec<u64>,
    /// Active bets per seat.
    seat_bets: Vec<Vec<ActiveBet>>,
    /// The established point (immutable).
    point: Point,
    /// All rolls this round.
    roll_history: Vec<DiceRoll>,
    /// Maximum odds multiple.
    max_odds: u8,
    /// Current shooter.
    shooter_idx: usize,
}

impl GamePointPhase {
    /// Returns the established point.
    pub fn point(&self) -> Point {
        self.point
    }

    /// Returns the bankrolls.
    pub fn bankrolls(&self) -> &[u64] {
        &self.bankrolls
    }

    /// Returns the active bets per seat.
    pub fn seat_bets(&self) -> &[Vec<ActiveBet>] {
        &self.seat_bets
    }

    /// Returns mutable access to bets (for adding odds/come bets mid-round).
    pub fn seat_bets_mut(&mut self) -> &mut Vec<Vec<ActiveBet>> {
        &mut self.seat_bets
    }

    /// Returns mutable access to bankrolls (for mid-round bet deductions).
    pub fn bankrolls_mut(&mut self) -> &mut Vec<u64> {
        &mut self.bankrolls
    }

    /// Returns the roll history.
    pub fn roll_history(&self) -> &[DiceRoll] {
        &self.roll_history
    }

    /// Returns the maximum odds multiple.
    pub fn max_odds(&self) -> u8 {
        self.max_odds
    }

    /// Returns the shooter index.
    pub fn shooter_idx(&self) -> usize {
        self.shooter_idx
    }

    /// Processes a point-phase roll.
    ///
    /// Returns either a continued point phase or a resolved game.
    pub fn roll(mut self, dice: DiceRoll) -> PointRollResult {
        self.roll_history.push(dice);

        if dice.sum() == self.point.value() || dice.is_seven() {
            PointRollResult::Resolved(GameResolved {
                bankrolls: self.bankrolls,
                seat_bets: self.seat_bets,
                point: Some(self.point),
                roll_history: self.roll_history,
                shooter_idx: self.shooter_idx,
                max_odds: self.max_odds,
            })
        } else {
            PointRollResult::Continue(self)
        }
    }
}

/// Result of a point-phase roll.
#[derive(Debug, Clone)]
pub enum PointRollResult {
    /// Roll did not resolve the round — continue rolling.
    Continue(GamePointPhase),
    /// Point made or seven-out — round resolved.
    Resolved(GameResolved),
}

// ─────────────────────────────────────────────────────────────
//  Resolved Phase
// ─────────────────────────────────────────────────────────────

/// Round resolved — all bets can be settled.
///
/// Contains the complete state needed to compute payouts and transition
/// to the next round.
#[derive(Debug, Clone, PartialEq, Elicit, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(kani, derive(elicitation::KaniCompose))]
pub struct GameResolved {
    /// Bankrolls before settlement.
    bankrolls: Vec<u64>,
    /// Active bets per seat at resolution time.
    seat_bets: Vec<Vec<ActiveBet>>,
    /// The point that was established (None if resolved on come-out).
    point: Option<Point>,
    /// Complete roll history for this round.
    roll_history: Vec<DiceRoll>,
    /// The shooter for this round.
    shooter_idx: usize,
    /// Max odds (carried for next round).
    max_odds: u8,
}

impl GameResolved {
    /// Returns the bankrolls.
    pub fn bankrolls(&self) -> &[u64] {
        &self.bankrolls
    }

    /// Returns the active bets per seat.
    pub fn seat_bets(&self) -> &[Vec<ActiveBet>] {
        &self.seat_bets
    }

    /// Returns the established point (None if come-out resolution).
    pub fn point(&self) -> Option<Point> {
        self.point
    }

    /// Returns the complete roll history.
    pub fn roll_history(&self) -> &[DiceRoll] {
        &self.roll_history
    }

    /// Returns the final roll that resolved the round.
    pub fn final_roll(&self) -> DiceRoll {
        *self
            .roll_history
            .last()
            .expect("resolved game always has at least one roll")
    }

    /// Returns the shooter index.
    pub fn shooter_idx(&self) -> usize {
        self.shooter_idx
    }

    /// Returns whether the pass line won this round.
    pub fn pass_line_won(&self) -> bool {
        let final_sum = self.final_roll().sum();
        match self.point {
            None => {
                // Come-out resolution: natural wins
                final_sum == 7 || final_sum == 11
            }
            Some(point) => {
                // Point phase: point made wins
                final_sum == point.value()
            }
        }
    }

    /// Transitions to the next round's betting phase with updated bankrolls.
    ///
    /// Rotates the shooter to the next seat.
    pub fn next_round(self, updated_bankrolls: Vec<u64>) -> GameBetting {
        let num_seats = updated_bankrolls.len();
        let next_shooter = if num_seats > 0 {
            (self.shooter_idx + 1) % num_seats
        } else {
            0
        };
        GameBetting {
            bankrolls: updated_bankrolls,
            max_odds: self.max_odds,
            shooter_idx: next_shooter,
        }
    }
}
