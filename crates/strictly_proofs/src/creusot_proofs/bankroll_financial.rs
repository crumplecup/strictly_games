//! Creusot deductive proofs for BankrollLedger financial typestate.
//!
//! These are **real Why3 goals** — no `#[trusted]`.
//!
//! # Architecture (C25c inlined-body pattern)
//!
//! `cargo creusot prove -- -p strictly_proofs` compiles `strictly_blackjack`
//! without Creusot injection — `debit`, `settle`, and `gross_return` are
//! opaque to the prover.  Calling them yields `{false} any` in the COMA,
//! making VCs unprovable.
//!
//! Fix (documented in elicitation gallery C25): **inline the arithmetic**.
//! Do not call `strictly_blackjack` functions.  Instead express each step
//! directly in `strictly_proofs` proof functions.  Kani proves the real
//! implementations match this arithmetic; Creusot proves the math properties.
//!
//! # Proof structure
//!
//! Each proof uses two helpers that mirror the real functions:
//! - `gross_return_spec(outcome, bet)`: Pearlite `#[logic]` match — the spec
//! - `inline_gross_return(outcome, bet)`: executable version proved against spec
//!
//! Then every financial proof is a two-step pipeline:
//!   1. `post_bet = bankroll − bet`  (mirrors `debit`)
//!   2. `final = post_bet + gross_return(outcome, bet)` (mirrors `settle`)
//!
//! # Properties proven (real VCs)
//!
//! 1. Push identity:    `debit(b, x) |> settle(Push)      == b`
//! 2. Win gain:         `debit(b, x) |> settle(Win)       == b + x`
//! 3. Loss deduction:   `debit(b, x) |> settle(Loss)      == b − x`
//! 4. Additive settle:  `∀ outcome, final ≥ bankroll − bet`
//! 5. Blackjack gain:   `debit(b, x) |> settle(Blackjack) > b`

use strictly_blackjack::Outcome;

#[cfg(creusot)]
use creusot_std::prelude::*;

// ── Inline specs (mirrors strictly_blackjack, visible to Creusot) ─────────────

/// The `gross_return` specification as a Pearlite `#[logic]` predicate.
///
/// ADT variant names from `strictly_blackjack` are visible cross-crate; only
/// function *bodies* are opaque.  This logic function lives in `strictly_proofs`
/// so its body is always visible to the prover.
#[cfg(creusot)]
#[logic]
pub fn gross_return_spec(outcome: Outcome, bet: u64) -> Int {
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

/// Inline executable `gross_return`, proved against `gross_return_spec`.
#[cfg(creusot)]
#[requires(gross_return_spec(outcome, bet) <= u64::MAX@)]
#[ensures(result@ == gross_return_spec(outcome, bet))]
fn inline_gross_return(outcome: Outcome, bet: u64) -> u64 {
    match outcome {
        Outcome::Push => bet,
        Outcome::Loss => 0,
        Outcome::Win => bet * 2,
        Outcome::Surrender => bet / 2,
        Outcome::Blackjack => bet * 2 + bet / 2,
    }
}

// ── Real proof functions (no #[trusted] — Why3 goals generated) ──────────────

/// Push is a net-zero operation: final balance equals original bankroll.
///
/// Why3 goal: `(bankroll − bet) + bet == bankroll`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@)]
pub fn verify_push_identity(bankroll: u64, bet: u64) -> u64 {
    let post_bet = bankroll - bet;
    post_bet + inline_gross_return(Outcome::Push, bet)
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
    let post_bet = bankroll - bet;
    post_bet + inline_gross_return(Outcome::Win, bet)
}

/// Loss is net-negative: final balance equals bankroll minus bet.
///
/// Why3 goal: `(bankroll − bet) + 0 == bankroll − bet`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@ - bet@)]
pub fn verify_loss_deduction(bankroll: u64, bet: u64) -> u64 {
    let post_bet = bankroll - bet;
    post_bet + inline_gross_return(Outcome::Loss, bet)
}

/// Settlement is always additive: final ≥ post-bet balance for every outcome.
///
/// Why3 goal: `gross_return(outcome, bet) ≥ 0`, so `post + gross ≥ post`
#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[requires(gross_return_spec(outcome, bet) <= u64::MAX@)]
#[requires(bankroll@ + gross_return_spec(outcome, bet) <= u64::MAX@)]
#[ensures(result@ >= bankroll@ - bet@)]
pub fn verify_settlement_additive(bankroll: u64, bet: u64, outcome: Outcome) -> u64 {
    let post_bet = bankroll - bet;
    post_bet + inline_gross_return(outcome, bet)
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
    let post_bet = bankroll - bet;
    post_bet + inline_gross_return(Outcome::Blackjack, bet)
}
