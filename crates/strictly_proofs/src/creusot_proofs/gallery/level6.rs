//! Gallery level SGC6: cross-crate enum in `#[logic]` + inlined arithmetic proofs.
//!
//! **Hypothesis**: The C25c "inlined body" pattern (from elicitation gallery
//! C25) solves the `strictly_blackjack` cross-crate contract problem.
//!
//! ## The problem (diagnosed via elicitation C25)
//!
//! `cargo creusot prove -- -p strictly_proofs` compiles `strictly_blackjack`
//! **without** Creusot injection — no `creusot_contracts` bindings are
//! available in the dep crate.  As a result, every call to
//! `BankrollLedger::debit`, `settle`, or `Outcome::gross_return` appears as an
//! opaque function in the COMA output: `{false} any`.  The VC is unprovable.
//!
//! ## The fix (C25c pattern)
//!
//! Don't call the `strictly_blackjack` functions from proof code.  Instead:
//! - Inline the arithmetic directly in the `strictly_proofs` proof functions.
//! - Use `Outcome` variants from `strictly_blackjack` as *values* in `match` —
//!   ADT definitions (enum arms) **are** visible cross-crate; function bodies
//!   are not.
//!
//! Kani already proves the implementations match the arithmetic.
//! Creusot proves the mathematical properties hold for the inlined arithmetic.
//! Together they form a sound two-layer verification stack.
//!
//! ## Experiment table
//!
//! | ID     | What                                                     | Expected |
//! |--------|----------------------------------------------------------|----------|
//! | SGC6a  | `#[logic]` over cross-crate `Outcome` enum via `match`   | ✓        |
//! | SGC6b  | Inline `gross_return` arithmetic, no real fn call        | ✓        |
//! | SGC6c  | Inline `debit` arithmetic: `post_bet = bankroll − bet`   | ✓        |
//! | SGC6d  | Inline `settle`: `final = post_bet + gross_return`       | ✓        |
//! | SGC6e  | Push identity pipeline (all inline): result == bankroll  | ✓        |
//! | SGC6f  | Win gain pipeline (all inline): result == bankroll + bet | ✓        |
//! | SGC6g  | Loss deduction pipeline: result == bankroll − bet        | ✓        |
//!
//! ## Run
//!
//! ```bash
//! just verify-gallery-creusot
//! ```

use creusot_std::prelude::*;
use strictly_blackjack::Outcome;

// ── SGC6a: #[logic] predicate over cross-crate Outcome ───────────────────────

/// SGC6a: the `gross_return` specification as a Pearlite `#[logic]` predicate.
///
/// Uses `Outcome` from `strictly_blackjack` as a value in `match` — ADT
/// variant names are visible cross-crate.  No function call to
/// `gross_return`; the arithmetic is defined here as pure logic.
#[logic]
pub fn sgc6_gross_return_spec(outcome: Outcome, bet: u64) -> Int {
    pearlite! {
        match outcome {
            Outcome::Push      => bet@,
            Outcome::Loss      => 0,
            Outcome::Win       => bet@ * 2,
            Outcome::Surrender => bet@ / 2,
            Outcome::Blackjack => bet@ * 2 + bet@ / 2,
        }
    }
}

// ── SGC6b: inline gross_return arithmetic ────────────────────────────────────

/// SGC6b: inline `gross_return` arithmetic — no call to `strictly_blackjack`.
///
/// Creusot sees the full `match` body and can prove the ensures directly.
/// The precondition guards against overflow in the `Win` and `Blackjack` arms.
#[requires(sgc6_gross_return_spec(outcome, bet) <= u64::MAX@)]
#[ensures(result@ == sgc6_gross_return_spec(outcome, bet))]
pub fn sgc6_gross_return(outcome: Outcome, bet: u64) -> u64 {
    match outcome {
        Outcome::Push => bet,
        Outcome::Loss => 0,
        Outcome::Win => bet * 2,
        Outcome::Surrender => bet / 2,
        Outcome::Blackjack => bet * 2 + bet / 2,
    }
}

// ── SGC6c: inline debit arithmetic ───────────────────────────────────────────

/// SGC6c: inline `BankrollLedger::debit` arithmetic.
///
/// The real `debit` is opaque to Creusot (dep-crate, no injection).
/// We inline: `post_bet_balance = bankroll − bet`.
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@ - bet@)]
pub fn sgc6_debit(bankroll: u64, bet: u64) -> u64 {
    bankroll - bet
}

// ── SGC6d: inline settle arithmetic ──────────────────────────────────────────

/// SGC6d: inline `BankrollLedger::settle` arithmetic.
///
/// Given `post_bet_balance` and an outcome, computes the final balance.
/// The real `settle` is opaque; we inline the single-addition formula.
#[requires(post_bet@ + sgc6_gross_return_spec(outcome, bet) <= u64::MAX@)]
#[ensures(result@ == post_bet@ + sgc6_gross_return_spec(outcome, bet))]
pub fn sgc6_settle(post_bet: u64, outcome: Outcome, bet: u64) -> u64 {
    post_bet + sgc6_gross_return(outcome, bet)
}

// ── SGC6e: push identity pipeline ────────────────────────────────────────────

/// SGC6e: push is net-zero — final balance equals original bankroll.
///
/// Pipeline: debit → settle(Push) → result == bankroll.
/// Why3 goal: `(bankroll − bet) + bet == bankroll`.
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@)]
pub fn sgc6_push_identity(bankroll: u64, bet: u64) -> u64 {
    let post_bet = sgc6_debit(bankroll, bet);
    sgc6_settle(post_bet, Outcome::Push, bet)
}

// ── SGC6f: win gain pipeline ──────────────────────────────────────────────────

/// SGC6f: win is net-positive — final balance equals bankroll plus bet.
///
/// Why3 goal: `(bankroll − bet) + 2·bet == bankroll + bet`.
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[requires(bankroll@ <= u64::MAX@ - bet@)]
#[requires(sgc6_gross_return_spec(Outcome::Win, bet) <= u64::MAX@)]
#[ensures(result@ == bankroll@ + bet@)]
pub fn sgc6_win_gain(bankroll: u64, bet: u64) -> u64 {
    let post_bet = sgc6_debit(bankroll, bet);
    sgc6_settle(post_bet, Outcome::Win, bet)
}

// ── SGC6g: loss deduction pipeline ───────────────────────────────────────────

/// SGC6g: loss is net-negative — final balance equals bankroll minus bet.
///
/// Why3 goal: `(bankroll − bet) + 0 == bankroll − bet`.
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@ - bet@)]
pub fn sgc6_loss_deduction(bankroll: u64, bet: u64) -> u64 {
    let post_bet = sgc6_debit(bankroll, bet);
    sgc6_settle(post_bet, Outcome::Loss, bet)
}
