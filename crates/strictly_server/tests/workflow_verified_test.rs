//! [`VerifiedWorkflow`] validation tests for tic-tac-toe propositions
//! (defined in `strictly_server::games::tictactoe::contracts`).
//!
//! Verifies two guarantees for every proposition type:
//!
//! 1. **Non-emptiness** — each proposition's proof methods return a non-empty
//!    `TokenStream`. An empty stream means a proof is present in name only.
//!
//! 2. **Constituent delegation** — `LegalMove = And<SquareEmpty, PlayerTurn>`
//!    contains both constituents' proofs, proving the composite is not hollow.

use elicitation::VerifiedWorkflow;
use elicitation::contracts::And;
use strictly_server::{PlayerTurn, SquareEmpty};

type LegalMove = And<SquareEmpty, PlayerTurn>;

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
fn tictactoe_props_non_empty() {
    assert_verified::<SquareEmpty>("SquareEmpty");
    assert_verified::<PlayerTurn>("PlayerTurn");
}

// ── Composite proof containment ───────────────────────────────────────────────

#[test]
fn legal_move_contains_constituents() {
    assert!(
        LegalMove::kani_proof_contains::<SquareEmpty>(),
        "LegalMove Kani proof must contain SquareEmpty's proof"
    );
    assert!(
        LegalMove::kani_proof_contains::<PlayerTurn>(),
        "LegalMove Kani proof must contain PlayerTurn's proof"
    );
    assert!(
        LegalMove::verus_proof_contains::<SquareEmpty>(),
        "LegalMove Verus proof must contain SquareEmpty's proof"
    );
    assert!(
        LegalMove::verus_proof_contains::<PlayerTurn>(),
        "LegalMove Verus proof must contain PlayerTurn's proof"
    );
    assert!(
        LegalMove::creusot_proof_contains::<SquareEmpty>(),
        "LegalMove Creusot proof must contain SquareEmpty's proof"
    );
    assert!(
        LegalMove::creusot_proof_contains::<PlayerTurn>(),
        "LegalMove Creusot proof must contain PlayerTurn's proof"
    );
}
