//! Blackjack (natural 21) detection logic.

use super::super::{Card, HandValue};

/// Checks if a hand is a blackjack (natural 21 with 2 cards).
///
/// Returns true only if:
/// - Hand has exactly 2 cards
/// - Total value is 21
pub fn is_blackjack(cards: &[Card], value: &HandValue) -> bool {
    cards.len() == 2 && value.best() == 21
}

#[cfg(test)]
mod tests {
    use super::super::super::{Card, Rank, Suit};
    use super::super::hand_value::calculate_value;
    use super::*;

    #[test]
    fn test_blackjack_ace_king() {
        let cards = vec![
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::King, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert!(is_blackjack(&cards, &value));
    }

    #[test]
    fn test_blackjack_ace_ten() {
        let cards = vec![
            Card::new(Rank::Ace, Suit::Hearts),
            Card::new(Rank::Ten, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert!(is_blackjack(&cards, &value));
    }

    #[test]
    fn test_not_blackjack_three_cards() {
        let cards = vec![
            Card::new(Rank::Seven, Suit::Hearts),
            Card::new(Rank::Seven, Suit::Spades),
            Card::new(Rank::Seven, Suit::Diamonds),
        ];
        let value = calculate_value(&cards);
        assert!(!is_blackjack(&cards, &value));
    }

    #[test]
    fn test_not_blackjack_wrong_total() {
        let cards = vec![
            Card::new(Rank::Ten, Suit::Hearts),
            Card::new(Rank::Nine, Suit::Spades),
        ];
        let value = calculate_value(&cards);
        assert!(!is_blackjack(&cards, &value));
    }

    #[test]
    fn test_not_blackjack_empty() {
        let cards = vec![];
        let value = calculate_value(&cards);
        assert!(!is_blackjack(&cards, &value));
    }
}
