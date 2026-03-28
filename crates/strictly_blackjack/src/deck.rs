//! Deck of playing cards with shuffling and deal tracking.

use super::card::{Card, Rank, Suit};
use elicitation::Elicit;
use serde::{Deserialize, Serialize};

/// Maximum number of cards a deck can hold (standard single deck).
///
/// Fixed-size array lets Kani auto-determine loop bounds in `deal()` and all
/// callers, eliminating the need for per-harness `#[kani::unwind]` annotations.
pub const MAX_DECK_CARDS: usize = 52;

/// A standard 52-card deck.
#[derive(Debug, Clone, PartialEq, Eq, Elicit)]
pub struct Deck {
    cards: [Card; MAX_DECK_CARDS],
    total: usize,
    dealt: usize,
}

impl Deck {
    /// Creates a new shuffled deck with 52 cards.
    ///
    /// Uses elicitation's rand feature for shuffling if available.
    pub fn new_shuffled() -> Self {
        let mut cards = [Card::default(); MAX_DECK_CARDS];
        let mut total = 0;

        for suit in Suit::ALL {
            for rank in Rank::ALL {
                cards[total] = Card::new(rank, suit);
                total += 1;
            }
        }

        #[cfg(feature = "shuffle")]
        {
            use rand::seq::SliceRandom;
            use rand::thread_rng;
            cards[..total].shuffle(&mut thread_rng());
        }

        Self {
            cards,
            total,
            dealt: 0,
        }
    }

    /// Creates a deck with a specific card order (for testing and formal verification).
    ///
    /// Cards are placed in deal order: first card in the slice is dealt first.
    /// Panics if `cards.len() > MAX_DECK_CARDS`.
    pub fn new_ordered(cards: &[Card]) -> Self {
        assert!(
            cards.len() <= MAX_DECK_CARDS,
            "deck cannot hold more than {MAX_DECK_CARDS} cards"
        );
        let mut arr = [Card::default(); MAX_DECK_CARDS];
        arr[..cards.len()].copy_from_slice(cards);
        Self {
            cards: arr,
            total: cards.len(),
            dealt: 0,
        }
    }

    /// Deals one card from the top of the deck.
    ///
    /// Returns None if no cards remain.
    pub fn deal(&mut self) -> Option<Card> {
        if self.dealt < self.total {
            let card = self.cards[self.dealt];
            self.dealt += 1;
            Some(card)
        } else {
            None
        }
    }

    /// Returns the number of cards remaining in the deck.
    pub fn remaining(&self) -> usize {
        self.total.saturating_sub(self.dealt)
    }

    /// Returns the total number of cards in the deck.
    pub fn total(&self) -> usize {
        self.total
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
        self.cards[..self.total].shuffle(&mut thread_rng());
        self.dealt = 0;
    }
}

impl Default for Deck {
    fn default() -> Self {
        Self::new_shuffled()
    }
}

impl Serialize for Deck {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Deck", 2)?;
        s.serialize_field("cards", &self.cards[..self.total])?;
        s.serialize_field("dealt", &self.dealt)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Deck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct DeckHelper {
            cards: Vec<Card>,
            dealt: usize,
        }
        let helper = DeckHelper::deserialize(deserializer)?;
        if helper.cards.len() > MAX_DECK_CARDS {
            return Err(serde::de::Error::custom(format!(
                "deck cannot hold more than {MAX_DECK_CARDS} cards"
            )));
        }
        let mut arr = [Card::default(); MAX_DECK_CARDS];
        arr[..helper.cards.len()].copy_from_slice(&helper.cards);
        Ok(Self {
            cards: arr,
            total: helper.cards.len(),
            dealt: helper.dealt,
        })
    }
}
