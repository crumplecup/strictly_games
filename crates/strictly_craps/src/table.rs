//! Table state management for multi-seat craps.
//!
//! Provides the high-level [`CrapsTable`] that ties together seats,
//! bankrolls, lesson progress, and bet settlement logic. The table
//! orchestrates rounds by feeding data into the typestate machine
//! and interpreting results.
//!
//! # Seat lifecycle
//!
//! ```text
//! CrapsSeat (persistent)
//!   │
//!   ├─ place bets → Vec<ActiveBet>
//!   │
//!   ├─ round resolves → SeatRoundResult
//!   │
//!   └─ bankroll updated, lesson advanced
//! ```

use derive_getters::Getters;
use elicitation::Elicit;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::lesson::LessonProgress;
use crate::payout::BetOutcome;
use crate::{ActiveBet, BetType, CrapsError, CrapsErrorKind, DiceRoll, Point};

// ─────────────────────────────────────────────────────────────
//  CrapsSeat — persistent player at the table
// ─────────────────────────────────────────────────────────────

/// A player seated at the craps table.
///
/// Persists across rounds — tracks bankroll, lesson progress, and
/// whether this player is the current shooter.
#[derive(Debug, Clone, Serialize, Deserialize, Getters, Elicit, schemars::JsonSchema)]
pub struct CrapsSeat {
    /// Player display name.
    name: String,
    /// Current bankroll.
    bankroll: u64,
    /// Progressive lesson tracker.
    lesson: LessonProgress,
    /// Whether this seat is the current shooter.
    is_shooter: bool,
    /// Bets that persist across rounds (come bets on established points).
    persistent_bets: Vec<ActiveBet>,
}

impl CrapsSeat {
    /// Creates a new seat with the given name and bankroll.
    #[instrument(skip_all, fields(bankroll))]
    pub fn new(name: impl Into<String>, bankroll: u64) -> Self {
        Self {
            name: name.into(),
            bankroll,
            lesson: LessonProgress::new(),
            is_shooter: false,
            persistent_bets: Vec::new(),
        }
    }

    /// Sets this seat as the shooter.
    pub fn with_shooter(mut self, is_shooter: bool) -> Self {
        self.is_shooter = is_shooter;
        self
    }

    /// Sets the lesson progress for this seat.
    pub fn with_lesson(mut self, lesson: LessonProgress) -> Self {
        self.lesson = lesson;
        self
    }

    /// Deducts wagers from the bankroll.
    ///
    /// Returns an error if total wagers exceed bankroll.
    #[instrument(skip(self), fields(seat = %self.name))]
    pub fn deduct_wagers(&mut self, total: u64) -> Result<(), CrapsError> {
        if total > self.bankroll {
            return Err(CrapsErrorKind::InsufficientFunds {
                need: total,
                have: self.bankroll,
            }
            .into());
        }
        self.bankroll -= total;
        Ok(())
    }

    /// Credits winnings to the bankroll.
    #[instrument(skip(self), fields(seat = %self.name, amount))]
    pub fn credit_winnings(&mut self, amount: u64) {
        self.bankroll += amount;
    }

    /// Advances lesson progress by one round.
    #[instrument(skip(self), fields(seat = %self.name))]
    pub fn advance_round(&mut self) -> bool {
        self.lesson.try_advance()
    }

    /// Records a round played for lesson progress.
    #[instrument(skip(self), fields(seat = %self.name))]
    pub fn record_round(&mut self) {
        self.lesson.record_round();
    }

    /// Toggles the shooter flag.
    pub fn set_shooter(&mut self, shooter: bool) {
        self.is_shooter = shooter;
    }

    /// Adds a persistent bet (come bets that travel to a point).
    pub fn add_persistent_bet(&mut self, bet: ActiveBet) {
        self.persistent_bets.push(bet);
    }

    /// Takes and clears all persistent bets (consumed at settlement).
    pub fn take_persistent_bets(&mut self) -> Vec<ActiveBet> {
        std::mem::take(&mut self.persistent_bets)
    }
}

impl std::fmt::Display for CrapsSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shooter = if self.is_shooter { " 🎲" } else { "" };
        write!(
            f,
            "{}{} (${}, L{})",
            self.name,
            shooter,
            self.bankroll,
            self.lesson.level()
        )
    }
}

// ─────────────────────────────────────────────────────────────
//  SeatRoundResult — outcome for a single seat after settlement
// ─────────────────────────────────────────────────────────────

/// Settlement result for one seat after a round.
#[derive(Debug, Clone, Getters)]
pub struct SeatRoundResult {
    /// Seat index.
    seat_idx: usize,
    /// Player name.
    name: String,
    /// Each bet and its outcome.
    outcomes: Vec<(ActiveBet, BetOutcome)>,
    /// Net gain or loss for this round.
    net: i64,
    /// Bankroll after settlement.
    final_bankroll: u64,
}

impl std::fmt::Display for SeatRoundResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.net >= 0 { "+" } else { "" };
        write!(
            f,
            "{}: {}{} (bankroll: ${})",
            self.name, sign, self.net, self.final_bankroll
        )
    }
}

// ─────────────────────────────────────────────────────────────
//  CrapsTable — multi-seat game orchestrator
// ─────────────────────────────────────────────────────────────

/// Multi-seat craps table that orchestrates rounds.
///
/// Manages seats, shooter rotation, and bet validation. Delegates
/// phase transitions to the typestate machine.
#[derive(Debug, Clone, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
pub struct CrapsTable {
    /// All players at the table.
    seats: Vec<CrapsSeat>,
    /// Index of the current shooter.
    shooter_idx: usize,
    /// Maximum odds multiple allowed.
    max_odds: u8,
    /// Table minimum bet.
    table_min: u64,
    /// Table maximum bet.
    table_max: u64,
}

impl CrapsTable {
    /// Creates a new table with the given odds limit and bet range.
    #[instrument(skip_all, fields(max_odds, table_min, table_max))]
    pub fn new(max_odds: u8, table_min: u64, table_max: u64) -> Self {
        Self {
            seats: Vec::new(),
            shooter_idx: 0,
            max_odds,
            table_min,
            table_max,
        }
    }

    /// Returns all seats.
    pub fn seats(&self) -> &[CrapsSeat] {
        &self.seats
    }

    /// Returns the maximum odds multiple.
    pub fn max_odds(&self) -> u8 {
        self.max_odds
    }

    /// Returns the table minimum.
    pub fn table_min(&self) -> u64 {
        self.table_min
    }

    /// Returns the table maximum.
    pub fn table_max(&self) -> u64 {
        self.table_max
    }

    /// Returns the current shooter index.
    pub fn shooter_idx(&self) -> usize {
        self.shooter_idx
    }

    /// Adds a player to the table.
    #[instrument(skip(self, seat), fields(name = %seat.name))]
    pub fn add_seat(&mut self, seat: CrapsSeat) {
        self.seats.push(seat);
    }

    /// Returns the current shooter's seat.
    pub fn shooter(&self) -> Option<&CrapsSeat> {
        self.seats.get(self.shooter_idx)
    }

    /// Returns a mutable reference to a seat by index.
    pub fn seat_mut(&mut self, idx: usize) -> Option<&mut CrapsSeat> {
        self.seats.get_mut(idx)
    }

    /// Returns bankrolls as a vector (feeds into typestate).
    #[instrument(skip(self))]
    pub fn bankroll_vec(&self) -> Vec<u64> {
        self.seats.iter().map(|s| *s.bankroll()).collect()
    }

    /// Validates a bet for a specific seat.
    ///
    /// Checks lesson level, table limits, line-bet prerequisites,
    /// and bankroll sufficiency.
    #[instrument(skip(self), fields(seat_idx, bet_type = %bet_type, amount))]
    pub fn validate_bet(
        &self,
        seat_idx: usize,
        bet_type: BetType,
        amount: u64,
        current_bets: &[ActiveBet],
    ) -> Result<(), CrapsError> {
        let seat = self
            .seats
            .get(seat_idx)
            .ok_or_else(|| CrapsErrorKind::InvalidPhase("invalid seat index".to_string()))?;

        // Check lesson level
        if !seat.lesson().is_unlocked(bet_type) {
            return Err(CrapsErrorKind::BetNotUnlocked {
                bet: bet_type.to_string(),
                required: bet_type.lesson_level(),
                current: seat.lesson().level(),
            }
            .into());
        }

        // Check table limits
        if amount < self.table_min || amount > self.table_max {
            return Err(CrapsErrorKind::InvalidBet(amount).into());
        }

        // Check odds prerequisite
        if bet_type.requires_line_bet() {
            let has_line = current_bets.iter().any(|b| match bet_type {
                BetType::PassOdds => b.bet_type() == BetType::PassLine,
                BetType::DontPassOdds => b.bet_type() == BetType::DontPass,
                BetType::ComeOdds(_) => b.bet_type() == BetType::Come,
                BetType::DontComeOdds(_) => b.bet_type() == BetType::DontCome,
                _ => false,
            });
            if !has_line {
                return Err(CrapsErrorKind::MissingLineBet {
                    line_bet: match bet_type {
                        BetType::PassOdds => "Pass Line".to_string(),
                        BetType::DontPassOdds => "Don't Pass".to_string(),
                        BetType::ComeOdds(_) => "Come".to_string(),
                        BetType::DontComeOdds(_) => "Don't Come".to_string(),
                        _ => "Unknown".to_string(),
                    },
                }
                .into());
            }

            // Check odds amount against max multiple
            let line_amount = current_bets
                .iter()
                .find(|b| match bet_type {
                    BetType::PassOdds => b.bet_type() == BetType::PassLine,
                    BetType::DontPassOdds => b.bet_type() == BetType::DontPass,
                    _ => false,
                })
                .map(|b| b.amount())
                .unwrap_or(0);

            if line_amount > 0 {
                let max_odds_amount = line_amount * self.max_odds as u64;
                if amount > max_odds_amount {
                    return Err(CrapsErrorKind::OddsExceedMax {
                        amount,
                        multiple: self.max_odds,
                        max: max_odds_amount,
                    }
                    .into());
                }
            }
        }

        // Check bankroll sufficiency
        let current_wagered: u64 = current_bets.iter().map(|b| b.amount()).sum();
        let total = current_wagered + amount;
        if total > *seat.bankroll() {
            return Err(CrapsErrorKind::InsufficientFunds {
                need: total,
                have: *seat.bankroll(),
            }
            .into());
        }

        Ok(())
    }

    /// Settles all bets after round resolution.
    ///
    /// Computes outcomes per seat, updates bankrolls, records rounds.
    #[instrument(skip(self, seat_bets, last_roll))]
    pub fn settle_round(
        &mut self,
        seat_bets: &[Vec<ActiveBet>],
        last_roll: DiceRoll,
        table_point: Option<Point>,
        is_comeout: bool,
    ) -> Vec<SeatRoundResult> {
        let mut results = Vec::with_capacity(self.seats.len());

        for (idx, bets) in seat_bets.iter().enumerate() {
            let Some(seat) = self.seats.get_mut(idx) else {
                continue;
            };

            let mut outcomes = Vec::with_capacity(bets.len());
            let mut net: i64 = 0;

            for bet in bets {
                let outcome = crate::payout::resolve_bet(bet, last_roll, table_point, is_comeout);

                match outcome {
                    BetOutcome::Win(profit) => {
                        // Return wager + profit
                        seat.credit_winnings(bet.amount() + profit);
                        net += profit as i64;
                    }
                    BetOutcome::Lose => {
                        // Wager already deducted
                        net -= bet.amount() as i64;
                    }
                    BetOutcome::Push => {
                        // Return wager
                        seat.credit_winnings(bet.amount());
                    }
                    BetOutcome::NoAction => {
                        // Bet stays — return wager for now (re-placed next phase)
                        seat.credit_winnings(bet.amount());
                    }
                }

                outcomes.push((bet.clone(), outcome));
            }

            seat.record_round();

            results.push(SeatRoundResult {
                seat_idx: idx,
                name: seat.name.clone(),
                outcomes,
                net,
                final_bankroll: *seat.bankroll(),
            });
        }

        results
    }

    /// Rotates the shooter to the next seat.
    #[instrument(skip(self))]
    pub fn rotate_shooter(&mut self) {
        if !self.seats.is_empty() {
            // Clear old shooter
            if let Some(old) = self.seats.get_mut(self.shooter_idx) {
                old.set_shooter(false);
            }
            // Advance
            self.shooter_idx = (self.shooter_idx + 1) % self.seats.len();
            // Set new shooter
            if let Some(new) = self.seats.get_mut(self.shooter_idx) {
                new.set_shooter(true);
            }
        }
    }

    /// Returns the number of seats with non-zero bankrolls.
    pub fn active_seat_count(&self) -> usize {
        self.seats.iter().filter(|s| *s.bankroll() > 0).count()
    }
}

impl Default for CrapsTable {
    fn default() -> Self {
        Self::new(3, 5, 500)
    }
}
