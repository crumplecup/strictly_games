//! [`VerifiedWorkflow`] validation tests for `strictly_blackjack` propositions.
//!
//! Verifies two guarantees for every proposition type:
//!
//! 1. **Non-emptiness** — each proposition's proof methods return a non-empty
//!    `TokenStream`. An empty stream would mean `#[derive(Prop)]` failed silently
//!    or a manual impl returned `TokenStream::new()`.
//!
//! 2. **Constituent delegation** — compound propositions built with `And<P, Q>`
//!    contain both `P`'s and `Q`'s proof streams. This guards against a future
//!    refactor accidentally dropping a constituent from a composite proof.

#![cfg(feature = "proofs")]

use elicitation::VerifiedWorkflow;
use elicitation::contracts::And;
use strictly_blackjack::{
    BetDeducted, BetPlaced, NotBust, PayoutSettled, PlayerTurnComplete, ValidAction,
};

// Type alias matching the one in contracts.rs
type LegalAction = And<ValidAction, NotBust>;

#[track_caller]
fn assert_verified<T: VerifiedWorkflow>(label: &str) {
    assert!(
        T::validate_proofs_non_empty(),
        "{label}: proof methods returned an empty TokenStream — \
         check the #[derive(Prop)] or manual impl Prop"
    );
}

// ── Individual proposition non-emptiness ─────────────────────────────────────

#[test]
fn blackjack_props_non_empty() {
    assert_verified::<ValidAction>("ValidAction");
    assert_verified::<NotBust>("NotBust");
    assert_verified::<BetDeducted>("BetDeducted");
    assert_verified::<PayoutSettled>("PayoutSettled");
    assert_verified::<BetPlaced>("BetPlaced");
    assert_verified::<PlayerTurnComplete>("PlayerTurnComplete");
}

// ── Composite proof containment ───────────────────────────────────────────────

#[test]
fn legal_action_contains_constituents() {
    assert!(
        LegalAction::kani_proof_contains::<ValidAction>(),
        "LegalAction Kani proof must contain ValidAction's proof"
    );
    assert!(
        LegalAction::kani_proof_contains::<NotBust>(),
        "LegalAction Kani proof must contain NotBust's proof"
    );
    assert!(
        LegalAction::verus_proof_contains::<ValidAction>(),
        "LegalAction Verus proof must contain ValidAction's proof"
    );
    assert!(
        LegalAction::verus_proof_contains::<NotBust>(),
        "LegalAction Verus proof must contain NotBust's proof"
    );
    assert!(
        LegalAction::creusot_proof_contains::<NotBust>(),
        "LegalAction Creusot proof must contain NotBust's proof"
    );
    assert!(
        LegalAction::creusot_proof_contains::<ValidAction>(),
        "LegalAction Creusot proof must contain ValidAction's proof"
    );
}
