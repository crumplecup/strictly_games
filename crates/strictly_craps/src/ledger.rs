//! Proof-carrying bankroll ledger for craps financial transactions.
//!
//! The [`CrapsLedger`] ensures that bets are deducted exactly once and
//! settlements happen exactly once per bet. Unlike blackjack's single-bet
//! ledger, craps supports multiple simultaneous bets per seat.
//!
//! # Financial contract chain
//!
//! ```text
//! bankroll, bet
//!     │
//!     ▼
//! CrapsLedger::debit()  ──────────► Established<BetDeducted>
//!     │                                       │
//!     │  (carried through game phases         │
//!     │   as proof fields)                    │
//!     ▼                                       ▼
//! CrapsLedger::settle_round(outcomes, Established<BetDeducted>)
//!     │
//!     ▼
//! (final_bankroll, Established<RoundSettled>)
//! ```

use elicitation::Elicit;
use elicitation::VerifiedWorkflow;
use elicitation::contracts::Established;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::CrapsError;
use crate::error::CrapsErrorKind;
use crate::payout::BetOutcome;

// ── Financial propositions ────────────────────────────────────────────────────

/// Proposition: at least one bet has been deducted from the player's bankroll.
///
/// Established exclusively by [`CrapsLedger::debit`].
/// Required by [`CrapsLedger::settle_round`].
#[derive(Debug, Clone, Serialize, Deserialize, Elicit, elicitation::Prop, schemars::JsonSchema)]
pub struct BetDeducted;
impl VerifiedWorkflow for BetDeducted {}

/// Proposition: all bets for the round have been correctly settled.
///
/// Established exclusively by [`CrapsLedger::settle_round`].
/// Consuming `Established<BetDeducted>` guarantees settlement happened
/// exactly once.
#[derive(Debug, Clone, Serialize, Deserialize, Elicit, elicitation::Prop, schemars::JsonSchema)]
pub struct RoundSettled;
impl VerifiedWorkflow for RoundSettled {}

// ── Ledger ────────────────────────────────────────────────────────────────────

/// Carries the financial invariant through a craps round.
///
/// Tracks cumulative deductions for multiple bets and settles them all
/// at round end.
#[derive(Debug, Clone, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
pub struct CrapsLedger {
    /// Original bankroll before any bets this round.
    original_bankroll: u64,
    /// Current balance after deductions.
    current_balance: u64,
    /// Total amount wagered across all bets this round.
    total_wagered: u64,
}

impl CrapsLedger {
    /// Creates a new ledger for a round with the given starting bankroll.
    pub fn new(bankroll: u64) -> Self {
        Self {
            original_bankroll: bankroll,
            current_balance: bankroll,
            total_wagered: 0,
        }
    }

    /// Deducts a bet amount from the bankroll.
    ///
    /// Returns the debit proof on the first deduction. Subsequent deductions
    /// reuse the existing proof (the round has bets placed).
    ///
    /// # Errors
    ///
    /// Returns [`CrapsErrorKind::InvalidBet`] if `amount` is zero, or
    /// [`CrapsErrorKind::InsufficientFunds`] if `amount` exceeds balance.
    #[instrument]
    #[track_caller]
    pub fn debit(&mut self, amount: u64) -> Result<Established<BetDeducted>, CrapsError> {
        if amount == 0 {
            return Err(CrapsErrorKind::InvalidBet(amount).into());
        }
        if amount > self.current_balance {
            return Err(CrapsErrorKind::InsufficientFunds {
                need: amount,
                have: self.current_balance,
            }
            .into());
        }
        self.current_balance -= amount;
        self.total_wagered += amount;
        tracing::debug!(
            amount,
            remaining = self.current_balance,
            total_wagered = self.total_wagered,
            "Bet deducted"
        );
        Ok(Established::assert())
    }

    /// Settles all bets for the round by applying outcomes.
    ///
    /// Each outcome adds back the appropriate amount:
    /// - `Win(profit)`: returns wager + profit
    /// - `Lose`: wager already deducted, nothing returned
    /// - `Push`: returns the original wager
    /// - `NoAction`: returns the original wager (bet stays but round ends)
    ///
    /// Consumes the debit proof — settlement happens exactly once.
    #[instrument(skip(_pre))]
    #[track_caller]
    pub fn settle_round(
        self,
        outcomes: &[(u64, BetOutcome)],
        _pre: Established<BetDeducted>,
    ) -> (u64, Established<RoundSettled>) {
        let mut balance = self.current_balance;

        for &(wager, ref outcome) in outcomes {
            match outcome {
                BetOutcome::Win(profit) => {
                    balance += wager + profit;
                }
                BetOutcome::Lose => {
                    // Already deducted — nothing to return
                }
                BetOutcome::Push | BetOutcome::NoAction => {
                    balance += wager;
                }
            }
        }

        tracing::debug!(
            original = self.original_bankroll,
            final_balance = balance,
            total_wagered = self.total_wagered,
            "Round settled"
        );

        (balance, Established::assert())
    }

    /// Returns the current balance after deductions.
    pub fn current_balance(&self) -> u64 {
        self.current_balance
    }

    /// Returns the total amount wagered this round.
    pub fn total_wagered(&self) -> u64 {
        self.total_wagered
    }

    /// Returns the original bankroll before any bets.
    pub fn original_bankroll(&self) -> u64 {
        self.original_bankroll
    }
}
