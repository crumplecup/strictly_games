//! Hand representation and value calculation for blackjack.

use super::card::Card;
use elicitation::Elicit;
use serde::{Deserialize, Serialize};

/// Value of a blackjack hand (hard and soft totals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct HandValue {
    hard: u8,
    soft: Option<u8>,
}

impl HandValue {
    /// Creates a new HandValue.
    pub fn new(hard: u8, soft: Option<u8>) -> Self {
        Self { hard, soft }
    }

    /// Returns the hard total (all aces count as 1).
    pub fn hard(&self) -> u8 {
        self.hard
    }

    /// Returns the soft total (one ace counts as 11, if total ≤ 21).
    pub fn soft(&self) -> Option<u8> {
        self.soft
    }

    /// Returns the best value to use (soft if available, otherwise hard).
    pub fn best(self) -> u8 {
        self.soft.unwrap_or(self.hard)
    }

    /// Returns true if this hand is bust (hard total > 21).
    pub fn is_bust(self) -> bool {
        self.hard > 21
    }

    /// Returns true if this is a soft hand (has usable ace).
    pub fn is_soft(self) -> bool {
        self.soft.is_some()
    }
}

impl std::fmt::Display for HandValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(soft) = self.soft {
            write!(f, "{}/{}", self.hard, soft)
        } else {
            write!(f, "{}", self.hard)
        }
    }
}

/// A hand of cards in blackjack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elicit)]
pub struct Hand {
    cards: Vec<Card>,
}

impl Hand {
    /// Creates a new empty hand.
    pub fn new(cards: Vec<Card>) -> Self {
        Self { cards }
    }

    /// Adds a card to the hand.
    pub fn add_card(&mut self, card: Card) {
        self.cards.push(card);
    }

    /// Returns the cards in this hand.
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// Returns the number of cards in this hand.
    pub fn card_count(&self) -> usize {
        self.cards.len()
    }

    /// Returns true if the hand is empty.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Calculates the value of this hand.
    ///
    /// Returns both hard (all aces as 1) and soft (one ace as 11) totals.
    /// Soft total is only returned if it's ≤ 21.
    pub fn value(&self) -> HandValue {
        let mut hard_total = 0u8;
        let mut ace_count = 0;

        // Calculate hard total (all aces as 1)
        for card in &self.cards {
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

    /// Returns true if this hand is bust (value > 21).
    pub fn is_bust(&self) -> bool {
        self.value().is_bust()
    }

    /// Returns true if this is a blackjack (natural 21 with 2 cards).
    pub fn is_blackjack(&self) -> bool {
        self.cards.len() == 2 && self.value().best() == 21
    }

    /// Returns true if this hand can be split (2 cards with same rank).
    pub fn can_split(&self) -> bool {
        self.cards.len() == 2 && self.cards[0].rank() == self.cards[1].rank()
    }

    /// Formats the hand as a readable string showing all cards.
    pub fn display(&self) -> String {
        if self.cards.is_empty() {
            return "Empty hand".to_string();
        }

        let cards_str: Vec<String> = self.cards.iter().map(|c| c.to_string()).collect();
        let value = self.value();

        format!("{} ({})", cards_str.join(" "), value)
    }
}

impl Default for Hand {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl std::fmt::Display for Hand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}
