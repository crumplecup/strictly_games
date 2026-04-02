//! Creusot deductive proofs for BankrollLedger financial typestate.
//!
//! These are **real Why3 goals** — no `#[trusted]`.
//!
//! # Building blocks
//!
//! `Outcome` and `BankrollLedger` both `#[derive(Elicit)]`, which means their
//! types carry formal invariants through the elicitation framework.  The
//! `#[cfg_attr(creusot, …)]` contracts added to `gross_return`, `debit`, and
//! `settle` in `strictly_blackjack` give Creusot visibility into the arithmetic
//! of each method.  This file composes those contracts into higher-level
//! properties — no duplication, no `#[trusted]`.
//!
//! Pattern: `Outcome::creusot_proof() + BankrollLedger::creusot_proof()` →
//! combined proof obligation discharged by Why3.
//!
//! # Properties proven (real VCs)
//!
//! 1. Push identity:    `debit(b, x) |> settle(Push) == b`
//! 2. Win gain:         `debit(b, x) |> settle(Win) == b + x`
//! 3. Loss deduction:   `debit(b, x) |> settle(Loss) == b − x`
//! 4. Additive settle:  `∀ outcome, final ≥ bankroll − bet`
//! 5. Blackjack gain:   `debit(b, x) |> settle(Blackjack) > b`

use strictly_blackjack::{BankrollLedger, Outcome};

#[cfg(creusot)]
use creusot_std::prelude::*;

// ── Real proof functions (no #[trusted] — Why3 goals generated) ──────────────

/// Push is a net-zero operation: final balance equals original bankroll.
///
/// Builds on:
/// - `Outcome::gross_return` contract: Push → gross_return == bet
/// - `BankrollLedger::debit` contract:  post_bet_balance == bankroll − bet
/// - `BankrollLedger::settle` contract: result == post_bet_balance + gross_return
///
/// Why3 goal: `(bankroll − bet) + bet == bankroll`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@)]
pub fn verify_push_identity(bankroll: u64, bet: u64) -> u64 {
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).expect("valid");
    let (final_balance, _) = ledger.settle(Outcome::Push, token);
    final_balance
}

/// Win is net-positive: final balance equals bankroll plus bet.
///
/// Why3 goal: `(bankroll − bet) + 2·bet == bankroll + bet`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[requires(bankroll@ <= u64::MAX@ - bet@)]
#[ensures(result@ == bankroll@ + bet@)]
pub fn verify_win_gain(bankroll: u64, bet: u64) -> u64 {
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).expect("valid");
    let (final_balance, _) = ledger.settle(Outcome::Win, token);
    final_balance
}

/// Loss is net-negative: final balance equals bankroll minus bet.
///
/// Why3 goal: `(bankroll − bet) + 0 == bankroll − bet`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@ - bet@)]
pub fn verify_loss_deduction(bankroll: u64, bet: u64) -> u64 {
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).expect("valid");
    let (final_balance, _) = ledger.settle(Outcome::Loss, token);
    final_balance
}

/// Settlement is always additive: final ≥ post-bet balance for every outcome.
///
/// Why3 goal: `gross_return(outcome, bet) ≥ 0`, so `post + gross ≥ post`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ >= bankroll@ - bet@)]
pub fn verify_settlement_additive(bankroll: u64, bet: u64, outcome: Outcome) -> u64 {
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).expect("valid");
    let (final_balance, _) = ledger.settle(outcome, token);
    final_balance
}

/// Blackjack yields a strictly larger balance than the original bankroll.
///
/// Why3 goal: `(bankroll − bet) + bet + (bet·3)/2 > bankroll`
///             iff `(bet·3)/2 > 0`, which holds for any `bet ≥ 1`.
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[requires(bankroll@ <= u64::MAX@ - bet@ * 2)]
#[ensures(result@ > bankroll@)]
pub fn verify_blackjack_payout(bankroll: u64, bet: u64) -> u64 {
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).expect("valid");
    let (final_balance, _) = ledger.settle(Outcome::Blackjack, token);
    final_balance
}
