//! Deck of playing cards with shuffling and deal tracking.

use super::card::{Card, Rank, Suit};
use serde::{Deserialize, Serialize};

/// A standard 52-card deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    cards: Vec<Card>,
    dealt: usize,
}

impl Deck {
    /// Creates a new shuffled deck with 52 cards.
    ///
    /// Uses elicitation's rand feature for shuffling if available.
    pub fn new_shuffled() -> Self {
        let mut cards = Vec::with_capacity(52);

        // Generate all 52 cards (13 ranks × 4 suits)
        for suit in Suit::ALL {
            for rank in Rank::ALL {
                cards.push(Card::new(rank, suit));
            }
        }

        // Shuffle using rand if available
        #[cfg(feature = "shuffle")]
        {
            use rand::seq::SliceRandom;
            use rand::thread_rng;
            cards.shuffle(&mut thread_rng());
        }

        Self { cards, dealt: 0 }
    }

    /// Creates a new deck with specific card order (for testing).
    pub fn new_ordered(cards: Vec<Card>) -> Self {
        Self { cards, dealt: 0 }
    }

    /// Deals one card from the top of the deck.
    ///
    /// Returns None if no cards remain.
    pub fn deal(&mut self) -> Option<Card> {
        if self.dealt < self.cards.len() {
            let card = self.cards[self.dealt];
            self.dealt += 1;
            Some(card)
        } else {
            None
        }
    }

    /// Returns the number of cards remaining in the deck.
    pub fn remaining(&self) -> usize {
        self.cards.len().saturating_sub(self.dealt)
    }

    /// Returns the total number of cards in the deck.
    pub fn total(&self) -> usize {
        self.cards.len()
    }

    /// Returns the number of cards dealt.
    pub fn dealt_count(&self) -> usize {
        self.dealt
    }

    /// Returns true if no cards remain.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Resets the deck to deal from the beginning again.
    ///
    /// Does not reshuffle - cards maintain their current order.
    pub fn reset(&mut self) {
        self.dealt = 0;
    }

    /// Reshuffles the deck and resets the deal counter.
    #[cfg(feature = "shuffle")]
    pub fn reshuffle(&mut self) {
        use rand::seq::SliceRandom;
        use rand::thread_rng;
        self.cards.shuffle(&mut thread_rng());
        self.dealt = 0;
    }
}

impl Default for Deck {
    fn default() -> Self {
        Self::new_shuffled()
    }
}
