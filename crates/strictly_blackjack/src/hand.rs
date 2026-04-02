//! Hand representation and value calculation for blackjack.

use super::card::Card;
use elicitation::Elicit;
use serde::{Deserialize, Serialize};

/// Maximum number of cards a hand can hold.
///
/// With a single 52-card shoe, the longest possible non-busted hand is 11 cards:
/// 4 Aces (4) + 4 Twos (8) + 3 Threes (9) = 21.  Any 12th card would bust.
/// Using a fixed-size array lets Kani auto-determine loop bounds without
/// per-harness `#[kani::unwind]` annotations.
pub const MAX_HAND_CARDS: usize = 11;

/// Maximum number of player hands (1 initial + up to 3 splits).
///
/// Standard blackjack allows splitting pairs up to 3 times, producing at most 4 hands.
/// Fixed-size array lets Kani auto-determine loop bounds over player hands.
pub const MAX_PLAYER_HANDS: usize = 4;

/// Value of a blackjack hand (hard and soft totals).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit, schemars::JsonSchema,
)]
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
///
/// Backed by a fixed-size array of [`MAX_HAND_CARDS`] slots so that Kani can
/// auto-determine loop bounds in [`Hand::value`] without manual
/// `#[kani::unwind]` annotations on every verification harness.
///
/// Serializes / deserializes as a variable-length JSON array for wire
/// compatibility with the original `Vec`-backed representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Elicit, schemars::JsonSchema)]
pub struct Hand {
    cards: [Card; MAX_HAND_CARDS],
    len: usize,
}

impl Hand {
    /// Creates a hand pre-populated from a slice.
    ///
    /// # Panics
    ///
    /// Panics if `cards.len() > MAX_HAND_CARDS`.
    pub fn new(cards: &[Card]) -> Self {
        assert!(
            cards.len() <= MAX_HAND_CARDS,
            "Hand::new: {} cards exceeds MAX_HAND_CARDS ({})",
            cards.len(),
            MAX_HAND_CARDS,
        );
        let mut arr = [Card::default(); MAX_HAND_CARDS];
        arr[..cards.len()].copy_from_slice(cards);
        Self {
            cards: arr,
            len: cards.len(),
        }
    }

    /// Creates an empty hand.
    pub fn empty() -> Self {
        Self {
            cards: [Card::default(); MAX_HAND_CARDS],
            len: 0,
        }
    }

    /// Adds a card to the hand.
    ///
    /// # Panics
    ///
    /// Panics if the hand is already at capacity ([`MAX_HAND_CARDS`]).
    pub fn add_card(&mut self, card: Card) {
        assert!(
            self.len < MAX_HAND_CARDS,
            "Hand::add_card: hand is full ({} cards)",
            MAX_HAND_CARDS,
        );
        self.cards[self.len] = card;
        self.len += 1;
    }

    /// Returns the cards in this hand as a slice.
    pub fn cards(&self) -> &[Card] {
        &self.cards[..self.len]
    }

    /// Returns the number of cards in this hand.
    pub fn card_count(&self) -> usize {
        self.len
    }

    /// Returns true if the hand is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Calculates the value of this hand.
    ///
    /// Returns both hard (all aces as 1) and soft (one ace as 11) totals.
    /// Soft total is only returned if it's ≤ 21.
    pub fn value(&self) -> HandValue {
        let mut hard_total = 0u8;
        let mut ace_count = 0;

        // Bounded by MAX_HAND_CARDS constant so Kani auto-determines loop bound.
        for i in 0..MAX_HAND_CARDS {
            if i >= self.len {
                break;
            }
            let card = &self.cards[i];
            if card.is_ace() {
                ace_count += 1;
                hard_total = hard_total.saturating_add(1);
            } else {
                hard_total = hard_total.saturating_add(card.value());
            }
        }

        // Try to use one ace as 11 (soft total)
        let soft_total = if ace_count > 0 {
            let soft = hard_total.saturating_add(10);
            if soft <= 21 { Some(soft) } else { None }
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
        self.len == 2 && self.value().best() == 21
    }

    /// Returns true if this hand can be split (2 cards with same rank).
    pub fn can_split(&self) -> bool {
        self.len == 2 && self.cards[0].rank() == self.cards[1].rank()
    }

    /// Formats the hand as a readable string showing all cards.
    pub fn display(&self) -> String {
        if self.len == 0 {
            return "Empty hand".to_string();
        }

        let cards_str: Vec<String> = self.cards[..self.len]
            .iter()
            .map(|c| c.to_string())
            .collect();
        let value = self.value();

        format!("{} ({})", cards_str.join(" "), value)
    }
}

impl Default for Hand {
    fn default() -> Self {
        Self::empty()
    }
}

impl std::fmt::Display for Hand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

impl Serialize for Hand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len))?;
        for i in 0..MAX_HAND_CARDS {
            if i >= self.len {
                break;
            }
            seq.serialize_element(&self.cards[i])?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Hand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cards: Vec<Card> = Vec::deserialize(deserializer)?;
        if cards.len() > MAX_HAND_CARDS {
            return Err(serde::de::Error::custom(format!(
                "hand has {} cards, exceeds MAX_HAND_CARDS ({})",
                cards.len(),
                MAX_HAND_CARDS,
            )));
        }
        Ok(Hand::new(&cards))
    }
}
