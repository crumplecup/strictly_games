//! [`ElicitComplete`] validation tests for tic-tac-toe types.
//!
//! `ElicitComplete` is the compiler-enforced stamp that a type satisfies every
//! framework obligation. These runtime tests verify the proof methods are
//! non-empty — something the type system cannot check since emptiness is a
//! value property, not a type property.
//!
//! Each call to `assert_proofs_non_empty::<T>` catches any manual `impl Elicitation`
//! (or field type) that accidentally returns `TokenStream::new()`.
//!
//! Composition is also verified: `Board` contains `Square`, and `Square`
//! contains `Player` — so `Board`'s proof must include all constituents.

#![cfg(feature = "verification")]

use elicitation::ElicitComplete;
use strictly_tictactoe::{Board, Player, Position, Square};

#[track_caller]
fn assert_proofs_non_empty<T: ElicitComplete>(label: &str) {
    assert!(
        T::validate_proofs_non_empty(),
        "{label}: proof methods returned an empty TokenStream — \
         check the #[derive(Elicit)] or manual impl Elicitation"
    );
}

// ── Non-emptiness ─────────────────────────────────────────────────────────────

#[test]
fn tictactoe_types_proofs_non_empty() {
    assert_proofs_non_empty::<Player>("Player");
    assert_proofs_non_empty::<Square>("Square");
    assert_proofs_non_empty::<Board>("Board");
    assert_proofs_non_empty::<Position>("Position");
}

// ── Composition: aggregate proofs contain constituent proofs ─────────────────

#[test]
fn square_proof_contains_player() {
    assert!(
        Square::kani_proof_contains::<Player>(),
        "Square Kani proof must contain Player's proof (Square::Occupied carries Player)"
    );
    assert!(
        Square::verus_proof_contains::<Player>(),
        "Square Verus proof must contain Player's proof"
    );
    assert!(
        Square::creusot_proof_contains::<Player>(),
        "Square Creusot proof must contain Player's proof"
    );
}

#[test]
fn board_proof_contains_square() {
    assert!(
        Board::kani_proof_contains::<Square>(),
        "Board Kani proof must contain Square's proof"
    );
    assert!(
        Board::verus_proof_contains::<Square>(),
        "Board Verus proof must contain Square's proof"
    );
    assert!(
        Board::creusot_proof_contains::<Square>(),
        "Board Creusot proof must contain Square's proof"
    );
}
