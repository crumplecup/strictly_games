//! Blackjack invariant proofs.
//!
//! Uses "cloud of assumptions" pattern:
//! - Trust: Rust's type system (enums, bounds checks, fixed arrays)
//! - Verify: Game semantics (deck integrity, hand values, game rules)

use strictly_blackjack::{Card, Deck, Hand, HandValue, MAX_DECK_CARDS, Rank, Suit};

/// Verifies a new deck has exactly 52 cards.
///
/// Property: |Deck::new_shuffled().remaining()| = 52
///
/// Cloud: Trust fixed-array indexing and RNG
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
/// Cloud: Trust fixed-array indexing and `dealt` counter
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
fn exhausted_deck_returns_none() {
    let mut deck = Deck::new_shuffled();

    // Deal all MAX_DECK_CARDS cards — concrete constant, Kani auto-determines bound.
    for _ in 0..MAX_DECK_CARDS {
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
    let hand = Hand::new(&[]);
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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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
fn hand_value_bounds() {
    // Symbolic array of 7 cards — no fill loop needed, Kani generates the array
    // symbolically without unwinding.  card_count bounds the active slice.
    let card_count: usize = kani::any();
    kani::assume(card_count <= 7);

    let cards: [Card; 7] = kani::any();

    let hand = Hand::new(&cards[..card_count]);
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
fn blackjack_requires_two_cards() {
    // Symbolic array of 7 cards — no fill loop needed.
    let card_count: usize = kani::any();
    kani::assume(card_count <= 7);

    let cards: [Card; 7] = kani::any();

    let hand = Hand::new(&cards[..card_count]);

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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[
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

    let hand = Hand::new(&[Card::new(rank, Suit::Spades), Card::new(rank, Suit::Hearts)]);

    assert!(hand.can_split(), "Matching ranks can split");
}

/// Verifies can_split fails with different ranks.
///
/// Property: rank(h[0]) ≠ rank(h[1]) ⟹ ¬can_split(h)
#[cfg(kani)]
#[kani::proof]
fn cannot_split_different_ranks() {
    let hand = Hand::new(&[
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
    let hand = Hand::new(&[Card::new(Rank::King, Suit::Spades)]);

    assert!(!hand.can_split(), "Single card cannot split");

    let hand3 = Hand::new(&[
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::King, Suit::Diamonds),
    ]);

    assert!(!hand3.can_split(), "Three cards cannot split");
}

/// Verifies face cards (Ten, Jack, Queen, King) all map to value 10.
///
/// Property: ∀s ∈ Suit, value(Card(Ten|Jack|Queen|King, s)) = 10
///
/// This is parametric: proven for all four suits, all four face-card ranks.
/// Closing the gap identified in the proof critique — the original range
/// check (1..=11) did not assert the specific mapping.
#[cfg(kani)]
#[kani::proof]
fn face_card_values_are_ten() {
    let suit: Suit = kani::any();

    // All four face-card ranks must map to exactly 10
    assert_eq!(Card::new(Rank::Ten, suit).value(), 10, "Ten = 10");
    assert_eq!(Card::new(Rank::Jack, suit).value(), 10, "Jack = 10");
    assert_eq!(Card::new(Rank::Queen, suit).value(), 10, "Queen = 10");
    assert_eq!(Card::new(Rank::King, suit).value(), 10, "King = 10");
}

/// Verifies ace hard value is always 1 (the adjusted value used in hand totals).
///
/// Property: ∀s ∈ Suit, value(Card(Ace, s)) = 11  (raw; adjusted to 1 in Hand calc)
///
/// Note: Rank::value() returns 11 for Ace (the "soft" value), but
/// calculate_value() counts aces as 1 in the hard total.  This harness
/// documents that contract explicitly.
#[cfg(kani)]
#[kani::proof]
fn ace_raw_value_is_eleven() {
    let suit: Suit = kani::any();
    // Rank::value() returns 11 for Ace — calculate_value treats it as 1 hard.
    assert_eq!(
        Card::new(Rank::Ace, suit).value(),
        11,
        "Ace raw rank value is 11"
    );
    // In a single-ace hand, hard=1 and soft=11
    let hand = Hand::new(&[Card::new(Rank::Ace, suit)]);
    assert_eq!(hand.value().hard(), 1, "Ace counts as 1 in hard total");
    assert_eq!(
        hand.value().soft(),
        Some(11),
        "Ace counts as 11 in soft total"
    );
}

/// Verifies the exact soft/hard relationship: soft == hard + 10.
///
/// Property: ∀h ∈ Hand, soft(h) = Some(s) ⟹ s = hard(h) + 10
///
/// This is the critical mathematical invariant: when a soft total exists,
/// it is exactly the hard total plus 10 (the ace promotion bonus).
/// Only one ace can be counted as 11 at a time.
#[cfg(kani)]
#[kani::proof]
fn soft_hard_exact_relation() {
    // Symbolic array of 7 cards — no fill loop needed.
    let card_count: usize = kani::any();
    kani::assume(card_count <= 7);

    let cards: [Card; 7] = kani::any();

    let hand = Hand::new(&cards[..card_count]);
    let value = hand.value();

    if let Some(soft) = value.soft() {
        // The exact relation: soft is always hard + 10 (one ace promoted from 1 to 11).
        assert_eq!(
            soft,
            value.hard() + 10,
            "soft == hard + 10 whenever soft total exists"
        );
        // Soft must be ≤ 21 (invariant from calculate_value)
        assert!(soft <= 21, "soft total ≤ 21");
    }
}

/// Verifies the deck (unshuffled) contains no duplicate cards.
///
/// Property: ∀i ≠ j in 0..52, deal_i(deck) ≠ deal_j(deck)
///
/// In Kani, the shuffle feature is disabled so `new_shuffled()` produces
/// a deterministic ordered deck.  Each of the 52 (Rank × Suit) pairs
/// appears exactly once — proven by exhaustive pairwise comparison.
#[cfg(kani)]
#[kani::proof]
fn deck_all_cards_unique() {
    let mut deck = Deck::new_shuffled();

    // Deal all cards into a fixed-size array — MAX_DECK_CARDS is a constant so
    // Kani auto-determines the loop bound without an unwind annotation.
    let mut cards = [Card::default(); MAX_DECK_CARDS];
    for i in 0..MAX_DECK_CARDS {
        let card = deck.deal();
        assert!(card.is_some(), "All 52 cards available on fresh deck");
        cards[i] = card.unwrap();
    }

    assert_eq!(deck.remaining(), 0, "Deck has exactly MAX_DECK_CARDS cards");

    // Verify no two positions hold the same card.
    // Both loop bounds are the constant MAX_DECK_CARDS — Kani unrolls them automatically.
    for i in 0..MAX_DECK_CARDS {
        for j in 0..MAX_DECK_CARDS {
            if i != j {
                assert_ne!(cards[i], cards[j], "No duplicate cards in deck");
            }
        }
    }
}

/// Verifies the biconditional: 2-card hand with value 21 IS blackjack.
///
/// Property: |h| = 2 ∧ best_value(h) = 21 ⟹ is_blackjack(h)  (converse)
///
/// The existing `blackjack_requires_two_cards` proves the forward direction.
/// This harness closes the proof critique's gap on the biconditional.
///
/// In practice the only 2-card 21 is Ace + {Ten, J, Q, K}.
#[cfg(kani)]
#[kani::proof]
fn blackjack_biconditional_converse() {
    let rank1: Rank = kani::any();
    let suit1: Suit = kani::any();
    let rank2: Rank = kani::any();
    let suit2: Suit = kani::any();

    let hand = Hand::new(&[Card::new(rank1, suit1), Card::new(rank2, suit2)]);

    // Converse: if it's a 2-card hand totaling 21, it must be blackjack.
    if hand.value().best() == 21 && hand.card_count() == 2 {
        assert!(
            hand.is_blackjack(),
            "2-card hand totaling 21 must be blackjack"
        );
    }
}

/// Verifies ace/ace double-ace hand: both aces, one promoted.
///
/// Property: value([A, A]) = HandValue { hard: 2, soft: Some(12) }
///
/// Two aces: hard=2 (both as 1), soft=12 (one promoted to 11, other stays 1).
/// 1+1+10=12 ≤ 21 so soft exists.
#[cfg(kani)]
#[kani::proof]
fn double_ace_value() {
    let hand = Hand::new(&[
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Ace, Suit::Hearts),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 2, "Two aces: hard = 2");
    assert_eq!(value.soft(), Some(12), "Two aces: soft = 12 (one promoted)");
    // Confirm exact relation holds
    assert_eq!(value.soft().unwrap(), value.hard() + 10);
}

/// Verifies ace/ace/nine: soft collapses when it would bust.
///
/// Property: value([A, A, 9]) = HandValue { hard: 11, soft: Some(21) }
///
/// 1+1+9=11 (hard), 11+1+9=21 (soft — one ace promoted, still ≤ 21).
#[cfg(kani)]
#[kani::proof]
fn ace_ace_nine_value() {
    let hand = Hand::new(&[
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Ace, Suit::Hearts),
        Card::new(Rank::Nine, Suit::Diamonds),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 11, "A,A,9: hard = 11");
    assert_eq!(
        value.soft(),
        Some(21),
        "A,A,9: soft = 21 (one ace promoted)"
    );
}

/// Verifies ace/ace/ten: soft collapses to hard only.
///
/// Property: value([A, A, 10]) = HandValue { hard: 12, soft: None }
///
/// 1+1+10=12 (hard), 11+1+10=22 (bust → no soft).
#[cfg(kani)]
#[kani::proof]
fn ace_ace_ten_soft_collapses() {
    let hand = Hand::new(&[
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::Ace, Suit::Hearts),
        Card::new(Rank::Ten, Suit::Diamonds),
    ]);

    let value = hand.value();
    assert_eq!(value.hard(), 12, "A,A,10: hard = 12");
    assert_eq!(value.soft(), None, "A,A,10: soft would bust (22), so None");
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
