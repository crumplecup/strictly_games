//! Blackjack invariant proofs.
//!
//! Uses "cloud of assumptions" pattern:
//! - Trust: Rust's type system (enums, bounds checks, Vec)
//! - Verify: Game semantics (deck integrity, hand values, game rules)

use strictly_blackjack::{Card, Deck, Hand, HandValue, Rank, Suit};

/// Verifies a new deck has exactly 52 cards.
///
/// Property: |Deck::new_shuffled().remaining()| = 52
///
/// Cloud: Trust Vec::len() and RNG
/// Verify: Our deck initialization creates 52 cards
#[cfg(kani)]
#[kani::proof]
fn deck_has_52_cards() {
    let deck = Deck::new_shuffled();
    assert_eq!(deck.remaining(), 52, "New deck has 52 cards");
}

/// Verifies dealing reduces remaining card count.
///
/// Property: deal(deck) ⟹ remaining(deck') = remaining(deck) - 1
///
/// Cloud: Trust Vec::pop()
/// Verify: Our deal tracking logic
#[cfg(kani)]
#[kani::proof]
fn deal_reduces_remaining() {
    let mut deck = Deck::new_shuffled();
    let initial = deck.remaining();

    if let Some(_card) = deck.deal() {
        let after = deck.remaining();
        assert_eq!(after, initial - 1, "Dealing reduces count by 1");
    }
}

/// Verifies dealing from exhausted deck returns None.
///
/// Property: remaining(deck) = 0 ⟹ deal(deck) = None
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(53)]  // Need 52 deals + 1 final check
fn exhausted_deck_returns_none() {
    let mut deck = Deck::new_shuffled();

    // Deal all 52 cards
    for _ in 0..52 {
        let card = deck.deal();
        assert!(card.is_some(), "Cards available while remaining > 0");
    }

    assert_eq!(deck.remaining(), 0, "Deck exhausted");

    // Try to deal from exhausted deck
    let card = deck.deal();
    assert!(card.is_none(), "Exhausted deck returns None");
}

/// Verifies Card::value() returns correct range.
///
/// Property: ∀c ∈ Card, value(c) ∈ {1, 2, ..., 11}
///
/// Cloud: Trust Rust's exhaustive enum matching
/// Verify: Our card value mapping
#[cfg(kani)]
#[kani::proof]
fn card_value_in_range() {
    let rank: Rank = kani::any();
    let suit: Suit = kani::any();
    let card = Card::new(rank, suit);

    let value = card.value();
    assert!(value >= 1 && value <= 11, "Card value in valid range");
}

/// Verifies ace detection.
///
/// Property: is_ace(Card(Ace, _)) = true
/// Property: is_ace(Card(other, _)) = false
#[cfg(kani)]
#[kani::proof]
fn ace_detection() {
    let suit: Suit = kani::any();

    let ace = Card::new(Rank::Ace, suit);
    assert!(ace.is_ace(), "Ace detected");

    let king = Card::new(Rank::King, suit);
    assert!(!king.is_ace(), "Non-ace not detected as ace");
}

/// Verifies empty hand has value 0.
///
/// Property: value(Hand::new()) = HandValue { hard: 0, soft: None }
#[cfg(kani)]
#[kani::proof]
fn empty_hand_zero_value() {
    let hand = Hand::new(vec![]);
    let value = hand.value();

    assert_eq!(value.hard(), 0, "Empty hand has hard value 0");
    assert_eq!(value.soft(), None, "Empty hand has no soft value");
}

/// Verifies hand value calculation without aces.
///
/// Property: value([2♠, 3♥]) = HandValue { hard: 5, soft: None }
#[cfg(kani)]
#[kani::proof]
fn hand_value_no_aces() {
    let hand = Hand::new(vec![
        Card::new(Rank::Two, Suit::Spades),
        Card::new(Rank::Three, Suit::Hearts),
    ]);

    let value = hand.value();

    assert_eq!(value.hard(), 5, "Hard total is sum of cards");
    assert_eq!(value.soft(), None, "No aces means no soft value");
}

/// Verifies hand value with single ace (soft).
///
/// Property: value([A♠, 6♥]) = HandValue { hard: 7, soft: Some(17) }
#[cfg(kani)]
#[kani::proof]
fn hand_value_single_ace_soft() {
    let hand = Hand::new(vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Six, Suit::Hearts),
    ]);

    let value = hand.value();

    assert_eq!(value.hard(), 7, "Ace counts as 1 in hard total");
    assert_eq!(value.soft(), Some(17), "Ace counts as 11 in soft total");
}

/// Verifies hand value with ace that would bust if soft.
///
/// Property: value([A♠, 10♥, 5♦]) = HandValue { hard: 16, soft: None }
///
/// Ace + 10 + 5 = 16 (hard), or 11 + 10 + 5 = 26 (bust, so no soft)
#[cfg(kani)]
#[kani::proof]
fn hand_value_ace_busts_soft() {
    let hand = Hand::new(vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Ten, Suit::Hearts),
        Card::new(Rank::Five, Suit::Diamonds),
    ]);

    let value = hand.value();

    assert_eq!(value.hard(), 16, "Hard total with ace as 1");
    assert_eq!(value.soft(), None, "Soft total would bust, so None");
}

/// Verifies hand value never exceeds maximum possible.
///
/// Property: ∀h ∈ Hand, hard(h) ≤ 127 ∧ (soft(h) = Some(s) ⟹ s ≤ 21)
///
/// Max hard value: Many aces counting as 1 each (bounded by u8)
/// Soft value only exists if ≤ 21
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(8)]
fn hand_value_bounds() {
    // Create hand with bounded number of cards (max 7 for practical verification)
    let card_count: usize = kani::any();
    kani::assume(card_count <= 7);

    let mut cards = Vec::new();
    for _ in 0..card_count {
        cards.push(kani::any());
    }

    let hand = Hand::new(cards);
    let value = hand.value();

    // Hard total can't overflow u8 (saturating_add prevents it)
    assert!(value.hard() <= 127, "Hard total within u8 bounds");

    // Soft value only exists if ≤ 21
    if let Some(soft) = value.soft() {
        assert!(soft <= 21, "Soft value ≤ 21");
        assert!(soft >= value.hard(), "Soft ≥ hard (ace adds 10)");
    }
}

/// Verifies blackjack detection requires exactly 2 cards totaling 21.
///
/// Property: is_blackjack(h) ⟹ |h| = 2 ∧ value(h) = 21
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(8)]
fn blackjack_requires_two_cards() {
    // Create hand with bounded number of cards
    let card_count: usize = kani::any();
    kani::assume(card_count <= 7);

    let mut cards = Vec::new();
    for _ in 0..card_count {
        cards.push(kani::any());
    }

    let hand = Hand::new(cards);

    if hand.is_blackjack() {
        assert_eq!(hand.card_count(), 2, "Blackjack requires exactly 2 cards");

        let value = hand.value();
        assert_eq!(value.best(), 21, "Blackjack best value is 21");
    }
}

/// Verifies blackjack is detected for ace + ten-value card.
///
/// Property: is_blackjack([A, 10]) = true
#[cfg(kani)]
#[kani::proof]
fn blackjack_ace_ten() {
    let hand = Hand::new(vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Ten, Suit::Hearts),
    ]);

    assert!(hand.is_blackjack(), "Ace + 10 is blackjack");
}

/// Verifies blackjack is detected for ace + face card.
///
/// Property: is_blackjack([A, K]) = true
#[cfg(kani)]
#[kani::proof]
fn blackjack_ace_king() {
    let hand = Hand::new(vec![
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
    ]);

    assert!(hand.is_blackjack(), "Ace + King is blackjack");
}

/// Verifies three-card 21 is not blackjack.
///
/// Property: |h| = 3 ∧ value(h) = 21 ⟹ ¬is_blackjack(h)
#[cfg(kani)]
#[kani::proof]
fn three_card_21_not_blackjack() {
    let hand = Hand::new(vec![
        Card::new(Rank::Seven, Suit::Spades),
        Card::new(Rank::Seven, Suit::Hearts),
        Card::new(Rank::Seven, Suit::Diamonds),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 21, "Hand totals 21");
    assert!(!hand.is_blackjack(), "Three cards is not blackjack");
}

/// Verifies bust detection when hard total > 21.
///
/// Property: hard(h) > 21 ⟹ is_bust(h)
#[cfg(kani)]
#[kani::proof]
fn bust_detection() {
    let hand = Hand::new(vec![
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::Queen, Suit::Hearts),
        Card::new(Rank::Five, Suit::Diamonds),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 25, "Hand totals 25");
    assert!(hand.is_bust(), "Hand > 21 is bust");
}

/// Verifies no bust when hand ≤ 21.
///
/// Property: hard(h) ≤ 21 ⟹ ¬is_bust(h)
#[cfg(kani)]
#[kani::proof]
fn no_bust_under_21() {
    let hand = Hand::new(vec![
        Card::new(Rank::Ten, Suit::Spades),
        Card::new(Rank::Six, Suit::Hearts),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 16, "Hand totals 16");
    assert!(!hand.is_bust(), "Hand ≤ 21 not bust");
}

/// Verifies hand exactly at 21 is not bust.
///
/// Property: hard(h) = 21 ⟹ ¬is_bust(h)
#[cfg(kani)]
#[kani::proof]
fn exactly_21_not_bust() {
    let hand = Hand::new(vec![
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::Queen, Suit::Hearts),
        Card::new(Rank::Ace, Suit::Diamonds),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 21, "Hand totals 21");
    assert!(!hand.is_bust(), "Exactly 21 not bust");
}

/// Verifies can_split requires exactly 2 cards of same rank.
///
/// Property: can_split(h) ⟹ |h| = 2 ∧ rank(h[0]) = rank(h[1])
#[cfg(kani)]
#[kani::proof]
fn can_split_matching_ranks() {
    let rank: Rank = kani::any();

    let hand = Hand::new(vec![
        Card::new(rank, Suit::Spades),
        Card::new(rank, Suit::Hearts),
    ]);

    assert!(hand.can_split(), "Matching ranks can split");
}

/// Verifies can_split fails with different ranks.
///
/// Property: rank(h[0]) ≠ rank(h[1]) ⟹ ¬can_split(h)
#[cfg(kani)]
#[kani::proof]
fn cannot_split_different_ranks() {
    let hand = Hand::new(vec![
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::Queen, Suit::Hearts),
    ]);

    assert!(!hand.can_split(), "Different ranks cannot split");
}

/// Verifies can_split fails with wrong card count.
///
/// Property: |h| ≠ 2 ⟹ ¬can_split(h)
#[cfg(kani)]
#[kani::proof]
fn cannot_split_wrong_count() {
    let hand = Hand::new(vec![
        Card::new(Rank::King, Suit::Spades),
    ]);

    assert!(!hand.can_split(), "Single card cannot split");

    let hand3 = Hand::new(vec![
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::King, Suit::Diamonds),
    ]);

    assert!(!hand3.can_split(), "Three cards cannot split");
}

/// Verifies HandValue equality is well-defined.
///
/// Property: HandValue implements PartialEq correctly
#[cfg(kani)]
#[kani::proof]
fn handvalue_equality() {
    let hv1: HandValue = kani::any();
    let hv2: HandValue = kani::any();

    // Equality is reflexive
    assert_eq!(hv1, hv1);

    // Equality is symmetric
    if hv1 == hv2 {
        assert_eq!(hv2, hv1);
    }
}

/// Verifies Card equality is well-defined.
///
/// Property: Card implements PartialEq correctly
#[cfg(kani)]
#[kani::proof]
fn card_equality() {
    let card1: Card = kani::any();
    let card2: Card = kani::any();

    // Equality is reflexive
    assert_eq!(card1, card1);

    // Equality is symmetric
    if card1 == card2 {
        assert_eq!(card2, card1);
    }
}
