//! Verus proofs for BankrollLedger financial typestate.
//!
//! ⚠️ **MIRROR PATTERN**: Financial types are duplicated here from `strictly_blackjack`.
//!
//! **Maintenance contract:**
//! - When `Outcome`, `BankrollLedger`, or `ActionError` change in
//!   `strictly_blackjack`, the mirror definitions MUST be updated manually.
//! - Arithmetic constants must match exactly.
//! - Run `just verify-verus-tracked` after ANY `strictly_blackjack` ledger changes.
//!
//! **Why this pattern:**
//! Verus cannot resolve workspace dependencies when run with
//! `verus --crate-type=lib`.  Types are mirrored and their key properties are
//! re-stated as specification functions.
//!
//! # Properties proven
//!
//! 1. Debit arithmetic: `post_bet_balance = bankroll − bet`
//! 2. Push identity: `debit |> settle(Push) == bankroll`
//! 3. Win gain: `debit |> settle(Win) == bankroll + bet`
//! 4. Loss deduction: `debit |> settle(Loss) == bankroll − bet`
//! 5. Settlement additive: `final ≥ post_bet_balance` for all outcomes
//! 6. Blackjack 3:2 payout arithmetic

use verus_builtin::*;
use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

// ============================================================================
// MIRRORED TYPE DEFINITIONS
// Source: strictly_blackjack/src/{types.rs, ledger.rs}
// Last synced: 2026-02-23
// ============================================================================

/// Mirror of `strictly_blackjack::Outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Blackjack,
    Push,
    Loss,
    Surrender,
}

impl Outcome {
    /// Chips returned to player for this outcome (gross, not net).
    ///
    /// - `Loss`      → 0       (forfeit)
    /// - `Surrender` → bet/2   (half back)
    /// - `Push`      → bet     (breakeven)
    /// - `Win`       → bet*2   (1:1 payout)
    /// - `Blackjack` → bet + (bet*3)/2  (3:2 payout)
    pub open spec fn gross_return(self, bet: u64) -> u64 {
        match self {
            Outcome::Win       => bet * 2,
            Outcome::Blackjack => bet + (bet * 3) / 2,
            Outcome::Push      => bet,
            Outcome::Loss      => 0,
            Outcome::Surrender => bet / 2,
        }
    }
}

// ── Ledger specification ──────────────────────────────────────────────────────

/// Specification model of `BankrollLedger`.
///
/// In actual code the fields are private; here we make them pub for reasoning.
pub struct Ledger {
    pub post_bet_balance: u64,
    pub bet: u64,
}

impl Ledger {
    /// Construct a ledger from a valid debit.
    pub open spec fn debit(bankroll: u64, bet: u64) -> Option<Ledger> {
        if bet == 0 || bet > bankroll {
            None
        } else {
            Some(Ledger { post_bet_balance: bankroll - bet, bet })
        }
    }

    /// Settle and return the final balance.
    pub open spec fn settle(self, outcome: Outcome) -> u64 {
        self.post_bet_balance + outcome.gross_return(self.bet)
    }
}

// ============================================================================
// VERIFICATION PROOFS
// ============================================================================

/// Debit arithmetic: post-bet balance is exactly bankroll minus bet.
pub proof fn verify_debit_arithmetic(bankroll: u64, bet: u64)
    requires
        bet > 0,
        bet <= bankroll,
    ensures
        Ledger::debit(bankroll, bet).is_some(),
        Ledger::debit(bankroll, bet).unwrap().post_bet_balance == bankroll - bet,
        Ledger::debit(bankroll, bet).unwrap().bet == bet,
{
    assert(Ledger::debit(bankroll, bet) == Some(Ledger {
        post_bet_balance: bankroll - bet,
        bet,
    }));
}

/// Push is a net-zero operation: bankroll is fully restored.
///
/// **Property:** `debit(b, x) |> settle(Push) == b`
pub proof fn verify_push_identity(bankroll: u64, bet: u64)
    requires
        bet > 0,
        bet <= bankroll,
    ensures
        Ledger::debit(bankroll, bet).is_some(),
        Ledger::debit(bankroll, bet).unwrap().settle(Outcome::Push) == bankroll,
{
    let ledger = Ledger::debit(bankroll, bet).unwrap();
    assert(ledger.settle(Outcome::Push) == ledger.post_bet_balance + bet);
    assert(ledger.post_bet_balance == bankroll - bet);
}

/// Win gives net gain equal to the bet.
///
/// **Property:** `debit(b, x) |> settle(Win) == b + x`
pub proof fn verify_win_gain(bankroll: u64, bet: u64)
    requires
        bet > 0,
        bet <= bankroll,
        bankroll <= u64::MAX - bet,
    ensures
        Ledger::debit(bankroll, bet).is_some(),
        Ledger::debit(bankroll, bet).unwrap().settle(Outcome::Win) == bankroll + bet,
{
    let ledger = Ledger::debit(bankroll, bet).unwrap();
    assert(ledger.settle(Outcome::Win) == ledger.post_bet_balance + bet * 2);
    assert(ledger.post_bet_balance == bankroll - bet);
    // (bankroll - bet) + (bet * 2) == bankroll + bet
}

/// Loss gives net loss equal to the bet.
///
/// **Property:** `debit(b, x) |> settle(Loss) == b − x`
pub proof fn verify_loss_deduction(bankroll: u64, bet: u64)
    requires
        bet > 0,
        bet <= bankroll,
    ensures
        Ledger::debit(bankroll, bet).is_some(),
        Ledger::debit(bankroll, bet).unwrap().settle(Outcome::Loss) == bankroll - bet,
{
    let ledger = Ledger::debit(bankroll, bet).unwrap();
    assert(ledger.settle(Outcome::Loss) == ledger.post_bet_balance);
    assert(ledger.post_bet_balance == bankroll - bet);
}

/// Settlement is always additive: final ≥ post-bet balance.
///
/// **Property:** `∀ outcome, settle(outcome).final ≥ post_bet_balance`
pub proof fn verify_settlement_additive(bankroll: u64, bet: u64, outcome: Outcome)
    requires
        bet > 0,
        bet <= bankroll,
    ensures
        Ledger::debit(bankroll, bet).is_some(),
        Ledger::debit(bankroll, bet).unwrap().settle(outcome) >=
            Ledger::debit(bankroll, bet).unwrap().post_bet_balance,
{
    let ledger = Ledger::debit(bankroll, bet).unwrap();
    // gross_return ≥ 0 for all outcomes (u64 ≥ 0 by type)
    match outcome {
        Outcome::Win       => assert(ledger.settle(outcome) == ledger.post_bet_balance + bet * 2),
        Outcome::Blackjack => assert(ledger.settle(outcome) == ledger.post_bet_balance + bet + (bet * 3) / 2),
        Outcome::Push      => assert(ledger.settle(outcome) == ledger.post_bet_balance + bet),
        Outcome::Loss      => assert(ledger.settle(outcome) == ledger.post_bet_balance),
        Outcome::Surrender => assert(ledger.settle(outcome) == ledger.post_bet_balance + bet / 2),
    }
}

/// Blackjack uses 3:2 integer payout arithmetic.
///
/// **Property:** `settle(Blackjack) == post_bet + bet + (bet*3)/2`
pub proof fn verify_blackjack_payout(bankroll: u64, bet: u64)
    requires
        bet > 0,
        bet <= bankroll,
        bet <= u64::MAX / 3,
    ensures
        Ledger::debit(bankroll, bet).is_some(),
        Ledger::debit(bankroll, bet).unwrap().settle(Outcome::Blackjack) ==
            (bankroll - bet) + bet + (bet * 3) / 2,
{
    let ledger = Ledger::debit(bankroll, bet).unwrap();
    assert(ledger.settle(Outcome::Blackjack) == ledger.post_bet_balance + bet + (bet * 3) / 2);
    assert(ledger.post_bet_balance == bankroll - bet);
}

} // verus!
