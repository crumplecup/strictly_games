//! Compositional capstone harness for blackjack types.
//!
//! ## Verification layer structure
//!
//! 1. **Generated foundation** (`generated/blackjack_foundation.rs`)
//!    Produced by `build.rs` calling `Type::kani_proof()` on Rank, Suit, Card, Outcome.
//!    Proves structural well-formedness: each type is constructible.
//!
//! 2. **Domain invariants** (`blackjack_invariants.rs`, `bankroll_financial.rs`)
//!    Prove game-logic and financial properties.
//!
//! 3. **Capstone** (this file)
//!    Cross-type property: any (Rank, Suit) pair produces a Card with a
//!    valid blackjack value — connecting structural and semantic layers.

use strictly_blackjack::{Card, Rank, Suit};

/// Capstone: any Card built from any Rank × Suit has a value in [1, 11].
///
/// This cross-type property witnesses that the structural foundation
/// (Rank constructible, Suit constructible, Card composed from both)
/// connects to the semantic blackjack value invariant.
#[cfg(kani)]
#[kani::proof]
fn verify_blackjack_legos() {
    let rank: Rank = kani::any();
    let suit: Suit = kani::any();
    let card = Card::new(rank, suit);
    let v = card.value();
    assert!(
        v >= 1 && v <= 11,
        "Any Card(Rank, Suit) has value in 1..=11 ∎"
    );
}
