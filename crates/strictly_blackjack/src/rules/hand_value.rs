//! Hand value calculation logic for blackjack.

use super::super::{Card, HandValue};

/// Calculates the value of a hand.
///
/// Returns both hard (all aces as 1) and soft (one ace as 11) totals.
pub fn calculate_value(cards: &[Card]) -> HandValue {
    let mut hard_total = 0u8;
    let mut ace_count = 0;

    // Calculate hard total (all aces as 1)
    for card in cards {
        if card.is_ace() {
            ace_count += 1;
            hard_total = hard_total.saturating_add(1);
        } else {
            hard_total = hard_total.saturating_add(card.value());
        }
    }

    // Try to use one ace as 11 (soft total)
    let soft_total = if ace_count > 0 {
        // Add 10 to convert one ace from 1 to 11
        let soft = hard_total.saturating_add(10);
        if soft <= 21 {
            Some(soft)
        } else {
            None
        }
    } else {
        None
    };

    HandValue::new(hard_total, soft_total)
}

#[cfg(test)]
mod tests {
    use super::super::super::{Card, Rank, Suit};
    use super::*;

    #[test]
    fn test_empty_hand() {
        let value = calculate_value(&[]);
        assert_eq!(value.hard(), 0);
        assert_eq!(value.soft(), None);
    }

    #[test]
    fn test_no_aces() {
        let cards = vec![
            Card::new(Rank::Ten, Suit::Hearts),
            Card::new(Rank::Seven, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert_eq!(value.hard(), 17);
        assert_eq!(value.soft(), None);
    }

    #[test]
    fn test_soft_hand() {
        let cards = vec![
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Six, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert_eq!(value.hard(), 7);
        assert_eq!(value.soft(), Some(17));
    }

    #[test]
    fn test_soft_hand_bust() {
        let cards = vec![
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::King, Suit::Spades),
            Card::new(Rank::Five, Suit::Diamonds),
        ];
        let value = calculate_value(&cards);
        assert_eq!(value.hard(), 16);
        assert_eq!(value.soft(), None); // 26 > 21, so no soft
    }

    #[test]
    fn test_multiple_aces() {
        let cards = vec![
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::Nine, Suit::Diamonds),
        ];
        let value = calculate_value(&cards);
        assert_eq!(value.hard(), 11); // 1 + 1 + 9
        assert_eq!(value.soft(), Some(21)); // 1 + 11 + 9
    }

    #[test]
    fn test_blackjack() {
        let cards = vec![
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::King, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert_eq!(value.hard(), 11);
        assert_eq!(value.soft(), Some(21));
    }
}
