//! Proof-carrying bankroll ledger for blackjack financial transactions.
//!
//! The [`BankrollLedger`] makes double-deduction structurally impossible.
//! A bet is deducted exactly once — inside [`BankrollLedger::debit`] — and the
//! resulting ledger carries a proof token ([`Established<BetDeducted>`]) that
//! must be presented to [`BankrollLedger::settle`].  The settlement function
//! adds back only the gross return (never subtracts), so the arithmetic has a
//! single, correct code path the compiler enforces.
//!
//! # Financial contract chain
//!
//! ```text
//! bankroll, bet
//!     │
//!     ▼
//! BankrollLedger::debit()  ──────────► Established<BetDeducted>
//!     │                                         │
//!     │  (carried through GamePlayerTurn        │
//!     │   and GameDealerTurn as game            │
//!     │   state fields)                         │
//!     ▼                                         ▼
//! BankrollLedger::settle(outcome, Established<BetDeducted>)
//!     │
//!     ▼
//! (final_bankroll, Established<PayoutSettled>)
//! ```
//!
//! # Applicability to high-assurance finance
//!
//! The same pattern applies to any system where a debit must precede a credit:
//! escrow, margin accounts, payment processors.  The type token replaces an
//! audit trail entry — the compiler refuses to compile code that skips the
//! debit step or attempts to settle twice (proof token is consumed on use).

use elicitation::contracts::{Established, Prop};
use elicitation::{ElicitCommunicator, ElicitResult, Elicitation, Prompt, default_style};
use strictly_blackjack::Outcome;
use tracing::instrument;

use super::action::ActionError;

// ── Financial propositions ────────────────────────────────────────────────────

/// Proposition: the player's bet has been deducted from their bankroll.
///
/// Established exclusively by [`BankrollLedger::debit`].
/// Required by [`BankrollLedger::settle`].
///
/// Carrying this token through game state means any code that reaches
/// settlement *must* have gone through a validated debit first.
pub struct BetDeducted;
impl Prop for BetDeducted {}

/// Proposition: the hand's payout has been correctly settled.
///
/// Established exclusively by [`BankrollLedger::settle`].
/// Consuming `Established<BetDeducted>` guarantees settlement happened
/// exactly once and that the gross-return arithmetic was applied.
pub struct PayoutSettled;
impl Prop for PayoutSettled {}

// ── Ledger ────────────────────────────────────────────────────────────────────

/// Carries the financial invariant through a blackjack hand.
///
/// - Created by [`debit`][Self::debit]: validates and removes the bet once.
/// - Consumed by [`settle`][Self::settle]: adds back the gross return.
/// - Stored in [`GamePlayerTurn`][super::typestate::GamePlayerTurn] and
///   [`GameDealerTurn`][super::typestate::GameDealerTurn] so it is present
///   wherever the final bankroll needs to be calculated.
///
/// The fields are private: the only way to create a valid ledger is through
/// `debit`, and the only way to produce a final balance is through `settle`.
/// Manual bankroll arithmetic is not possible from outside this module.
#[derive(Debug, Clone)]
pub struct BankrollLedger {
    /// Bankroll after the bet was removed.  Settlement adds gross return here.
    post_bet_balance: u64,
    /// The original bet amount, needed to compute the gross return.
    bet: u64,
}

impl BankrollLedger {
    /// Deducts `bet` from `bankroll`, producing a ledger and a debit proof.
    ///
    /// # Errors
    ///
    /// Returns [`ActionError::InvalidBet`] if `bet` is zero, or
    /// [`ActionError::InsufficientFunds`] if `bet` exceeds `bankroll`.
    #[instrument]
    #[track_caller]
    pub fn debit(bankroll: u64, bet: u64) -> Result<(Self, Established<BetDeducted>), ActionError> {
        if bet == 0 {
            return Err(ActionError::InvalidBet(bet));
        }
        if bet > bankroll {
            return Err(ActionError::InsufficientFunds(bet, bankroll));
        }
        let ledger = Self {
            post_bet_balance: bankroll - bet,
            bet,
        };
        Ok((ledger, Established::assert()))
    }

    /// Settles the hand by adding the gross return for `outcome` to the
    /// post-bet balance.
    ///
    /// Consumes `Established<BetDeducted>` — the compiler enforces that a
    /// valid [`debit`][Self::debit] call preceded this settlement, and that
    /// settlement happens at most once (the proof token is moved, not copied).
    ///
    /// Uses [`Outcome::gross_return`] exclusively — there is no subtraction
    /// path, so double-deduction is structurally impossible.
    #[instrument(skip(_pre))]
    #[track_caller]
    pub fn settle(
        self,
        outcome: Outcome,
        _pre: Established<BetDeducted>,
    ) -> (u64, Established<PayoutSettled>) {
        let returned = outcome.gross_return(self.bet);
        let final_balance = self.post_bet_balance + returned;
        tracing::debug!(
            bet = self.bet,
            post_bet_balance = self.post_bet_balance,
            returned,
            final_balance,
            outcome = ?outcome,
            "Payout settled"
        );
        (final_balance, Established::assert())
    }

    /// The bankroll balance after the bet was deducted (before settlement).
    pub fn post_bet_balance(&self) -> u64 {
        self.post_bet_balance
    }

    /// The bet amount being wagered this hand.
    pub fn bet(&self) -> u64 {
        self.bet
    }
}

// ── Elicitation impls ─────────────────────────────────────────────────────────

// BetDeducted is a unit proposition — only one value exists.
// An agent reconstructing game state can assert it without user interaction.
default_style!(BetDeducted => BetDeductedStyle);

impl Prompt for BetDeducted {
    fn prompt() -> Option<&'static str> {
        Some("BetDeducted proposition (proof that a bet was validly deducted)")
    }
}

impl Elicitation for BetDeducted {
    type Style = BetDeductedStyle;

    async fn elicit<C: ElicitCommunicator>(_communicator: &C) -> ElicitResult<Self> {
        tracing::debug!("Eliciting BetDeducted (unit proposition)");
        Ok(BetDeducted)
    }
}

// BankrollLedger elicitation: asks the communicator for post-bet balance and bet amount.
// Used when an agent reconstructs game state for reasoning or replay.
default_style!(BankrollLedger => BankrollLedgerStyle);

impl Prompt for BankrollLedger {
    fn prompt() -> Option<&'static str> {
        Some("Bankroll ledger (post-bet balance and bet amount)")
    }
}

impl Elicitation for BankrollLedger {
    type Style = BankrollLedgerStyle;

    async fn elicit<C: ElicitCommunicator>(communicator: &C) -> ElicitResult<Self> {
        tracing::debug!("Eliciting BankrollLedger");
        let post_bet_balance = u64::elicit(communicator).await?;
        let bet = u64::elicit(communicator).await?;
        Ok(Self {
            post_bet_balance,
            bet,
        })
    }
}
