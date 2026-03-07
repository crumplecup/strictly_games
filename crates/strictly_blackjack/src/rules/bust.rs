//! Bust (over 21) detection logic for blackjack.

use super::super::HandValue;

/// Checks if a hand is bust (hard total > 21).
///
/// A hand is bust when its hard total (counting all aces as 1) exceeds 21.
pub fn is_bust(value: &HandValue) -> bool {
    value.hard() > 21
}

#[cfg(test)]
mod tests {
    use super::super::super::{Card, Rank, Suit};
    use super::super::hand_value::calculate_value;
    use super::*;

    #[test]
    fn test_not_bust_under_21() {
        let cards = vec![
            Card::new(Rank::Ten, Suit::Hearts),
            Card::new(Rank::Nine, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert!(!is_bust(&value));
    }

    #[test]
    fn test_not_bust_exactly_21() {
        let cards = vec![
            Card::new(Rank::Ten, Suit::Hearts),
            Card::new(Rank::Five, Suit::Spades),
            Card::new(Rank::Six, Suit::Diamonds),
        ];
        let value = calculate_value(&cards);
        assert!(!is_bust(&value));
    }

    #[test]
    fn test_bust_over_21() {
        let cards = vec![
            Card::new(Rank::Ten, Suit::Hearts),
            Card::new(Rank::Nine, Suit::Spades),
            Card::new(Rank::Five, Suit::Diamonds),
        ];
        let value = calculate_value(&cards);
        assert!(is_bust(&value));
    }

    #[test]
    fn test_bust_with_aces() {
        let cards = vec![
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Queen, Suit::Spades),
            Card::new(Rank::Ace, Suit::Diamonds),
            Card::new(Rank::Ace, Suit::Clubs),
        ];
        let value = calculate_value(&cards);
        // Hard: 10 + 10 + 1 + 1 = 22
        assert!(is_bust(&value));
    }

    #[test]
    fn test_not_bust_empty_hand() {
        let cards = vec![];
        let value = calculate_value(&cards);
        assert!(!is_bust(&value));
    }
}
