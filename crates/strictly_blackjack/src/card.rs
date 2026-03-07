//! Card types for blackjack.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};

/// Rank of a playing card.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Elicit,
)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Rank {
    /// Ace (value 1 or 11).
    Ace,
    /// Two (value 2).
    Two,
    /// Three (value 3).
    Three,
    /// Four (value 4).
    Four,
    /// Five (value 5).
    Five,
    /// Six (value 6).
    Six,
    /// Seven (value 7).
    Seven,
    /// Eight (value 8).
    Eight,
    /// Nine (value 9).
    Nine,
    /// Ten (value 10).
    Ten,
    /// Jack (value 10).
    Jack,
    /// Queen (value 10).
    Queen,
    /// King (value 10).
    King,
}

impl Rank {
    /// Returns the base value of this rank.
    ///
    /// Aces return 11 (soft value) - adjusted to 1 in Hand calculations.
    /// Face cards (J, Q, K) return 10.
    pub fn value(self) -> u8 {
        match self {
            Rank::Ace => 11,
            Rank::Two => 2,
            Rank::Three => 3,
            Rank::Four => 4,
            Rank::Five => 5,
            Rank::Six => 6,
            Rank::Seven => 7,
            Rank::Eight => 8,
            Rank::Nine => 9,
            Rank::Ten | Rank::Jack | Rank::Queen | Rank::King => 10,
        }
    }

    /// Returns true if this is an ace.
    pub fn is_ace(self) -> bool {
        matches!(self, Rank::Ace)
    }

    /// Returns all 13 ranks in order.
    pub const ALL: [Rank; 13] = [
        Rank::Ace,
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
    ];
}

impl std::fmt::Display for Rank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rank::Ace => write!(f, "A"),
            Rank::Two => write!(f, "2"),
            Rank::Three => write!(f, "3"),
            Rank::Four => write!(f, "4"),
            Rank::Five => write!(f, "5"),
            Rank::Six => write!(f, "6"),
            Rank::Seven => write!(f, "7"),
            Rank::Eight => write!(f, "8"),
            Rank::Nine => write!(f, "9"),
            Rank::Ten => write!(f, "10"),
            Rank::Jack => write!(f, "J"),
            Rank::Queen => write!(f, "Q"),
            Rank::King => write!(f, "K"),
        }
    }
}

/// Suit of a playing card.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Elicit,
)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Suit {
    /// Hearts (♥).
    Hearts,
    /// Diamonds (♦).
    Diamonds,
    /// Clubs (♣).
    Clubs,
    /// Spades (♠).
    Spades,
}

impl Suit {
    /// Returns all 4 suits in order.
    pub const ALL: [Suit; 4] = [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades];

    /// Returns the Unicode symbol for this suit.
    pub fn symbol(self) -> &'static str {
        match self {
            Suit::Hearts => "♥",
            Suit::Diamonds => "♦",
            Suit::Clubs => "♣",
            Suit::Spades => "♠",
        }
    }
}

impl std::fmt::Display for Suit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

/// A playing card with rank and suit.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Elicit,
)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Card {
    rank: Rank,
    suit: Suit,
}

impl Card {
    /// Creates a new card with the given rank and suit.
    pub fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }

    /// Returns the rank of this card.
    pub fn rank(self) -> Rank {
        self.rank
    }

    /// Returns the suit of this card.
    pub fn suit(self) -> Suit {
        self.suit
    }

    /// Returns the base value of this card.
    pub fn value(self) -> u8 {
        self.rank.value()
    }

    /// Returns true if this card is an ace.
    pub fn is_ace(self) -> bool {
        self.rank.is_ace()
    }
}

impl std::fmt::Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.rank, self.suit)
    }
}
