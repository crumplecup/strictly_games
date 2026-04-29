//! Card shoe — a stateful generator that deals cards without replacement.
//!
//! Unlike dice (i.i.d. — each roll is independent), card draws are *sampling
//! without replacement*: the pool shrinks as cards enter play. [`Shoe`]
//! implements [`Generator`] with interior mutability via [`AtomicUsize`], matching
//! the elicitation framework's `generate(&self)` contract while tracking
//! which cards have been dealt.
//!
//! # Random Generation
//!
//! When the `shuffle` feature is enabled, [`Shoe::new`] builds and shuffles
//! a multi-deck shoe using a seeded RNG from `elicitation_rand`:
//!
//! ```rust,ignore
//! use elicitation::Generator;
//! use strictly_blackjack::Shoe;
//!
//! // Create a seeded single-deck shoe
//! let shoe = Shoe::new(42, 1);
//!
//! // Each call deals the next card (returns None when exhausted)
//! let card = shoe.generate();
//! assert!(card.is_some());
//! println!("Dealt: {}", card.unwrap());
//! ```
//!
//! For testing and formal verification, [`Shoe::from_ordered`] creates a
//! deterministic shoe with exact card order:
//!
//! ```rust,ignore
//! use elicitation::Generator;
//! use strictly_blackjack::{Card, Rank, Suit, Shoe};
//!
//! let shoe = Shoe::from_ordered(&[
//!     Card::new(Rank::Ace, Suit::Spades),
//!     Card::new(Rank::King, Suit::Spades),
//! ]);
//! assert_eq!(shoe.remaining(), 2);
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};

use elicitation::{Elicit, Generator};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{Card, Rank, Suit};

/// A card shoe that implements [`Generator`] for dealing without replacement.
///
/// The shoe holds 1–8 standard 52-card decks. Each [`generate`](Generator::generate)
/// call returns the next card from the shuffled pool, or `None` when exhausted.
/// Interior mutability via [`AtomicUsize`] allows `generate(&self)` without
/// requiring `&mut self` — a significant ergonomic win for multi-player code
/// where multiple seats share the same shoe.
///
/// # Stateful Generation
///
/// Unlike the craps dice generator (stateless, i.i.d.), the shoe is *stateful*:
/// each draw reduces the remaining pool. This showcases how to build
/// sampling-without-replacement generators on top of the elicitation framework.
///
/// # Determinism
///
/// Same seed + same number of decks → same card sequence. Ideal for replays,
/// testing, and formal verification.
#[derive(Debug, Elicit, schemars::JsonSchema)]
pub struct Shoe {
    /// Cards in deal order (shuffled at construction).
    cards: Vec<Card>,
    /// Number of cards dealt so far (interior mutability for `&self` generate).
    #[skip]
    dealt: AtomicUsize,
}

impl Clone for Shoe {
    fn clone(&self) -> Self {
        Self {
            cards: self.cards.clone(),
            dealt: AtomicUsize::new(self.dealt.load(Ordering::Relaxed)),
        }
    }
}

impl Generator for Shoe {
    type Target = Option<Card>;

    /// Deals the next card from the shoe.
    ///
    /// Returns `None` when all cards have been dealt. Uses interior mutability
    /// via [`AtomicUsize`] so dealing only requires `&self`.
    fn generate(&self) -> Option<Card> {
        let d = self.dealt.fetch_add(1, Ordering::Relaxed);
        if d < self.cards.len() {
            Some(self.cards[d])
        } else {
            self.dealt.store(self.cards.len(), Ordering::Relaxed);
            None
        }
    }
}

impl Shoe {
    /// Creates a new shuffled shoe with the given number of standard 52-card decks.
    ///
    /// # Panics
    ///
    /// Panics if `num_decks` is 0 or greater than 8.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use elicitation::Generator;
    /// use strictly_blackjack::Shoe;
    ///
    /// let shoe = Shoe::new(42, 1); // Single deck, seed 42
    /// let card = shoe.generate().unwrap();
    /// ```
    #[cfg(feature = "shuffle")]
    #[instrument(skip_all, fields(seed, num_decks))]
    pub fn new(seed: u64, num_decks: u8) -> Self {
        assert!(num_decks > 0 && num_decks <= 8, "num_decks must be 1..=8");

        let total = num_decks as usize * 52;
        let mut cards = Vec::with_capacity(total);

        for _ in 0..num_decks {
            for suit in Suit::ALL {
                for rank in Rank::ALL {
                    cards.push(Card::new(rank, suit));
                }
            }
        }

        // Shuffle using Fisher-Yates with elicitation_rand's seeded RNG
        use elicitation_rand::SeedableRng;
        let rng = elicitation_rand::StdRng::seed_from_u64(seed);
        let shuffle_gen = elicitation_rand::RandomGenerator::<u64>::new(rng);
        for i in (1..cards.len()).rev() {
            let j = shuffle_gen.generate() as usize % (i + 1);
            cards.swap(i, j);
        }

        tracing::debug!(total = cards.len(), "Shoe created and shuffled");

        Self {
            cards,
            dealt: AtomicUsize::new(0),
        }
    }

    /// Creates a shoe with a specific card order (for testing and formal verification).
    ///
    /// Cards are placed in deal order: first card in the slice is dealt first.
    /// No shuffling occurs regardless of feature flags.
    #[instrument(skip_all, fields(num_cards = cards.len()))]
    pub fn from_ordered(cards: &[Card]) -> Self {
        tracing::debug!(num_cards = cards.len(), "Shoe created from ordered cards");
        Self {
            cards: cards.to_vec(),
            dealt: AtomicUsize::new(0),
        }
    }

    /// Returns the number of cards remaining in the shoe.
    pub fn remaining(&self) -> usize {
        self.cards
            .len()
            .saturating_sub(self.dealt.load(Ordering::Relaxed))
    }

    /// Returns the total number of cards in the shoe.
    pub fn total(&self) -> usize {
        self.cards.len()
    }

    /// Returns the number of cards dealt so far.
    pub fn dealt_count(&self) -> usize {
        self.dealt.load(Ordering::Relaxed)
    }

    /// Returns true if no cards remain.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Resets the shoe to deal from the beginning again.
    ///
    /// Does not reshuffle — cards maintain their current order.
    pub fn reset(&self) {
        self.dealt.store(0, Ordering::Relaxed);
    }

    /// Reshuffles the shoe and resets the deal counter.
    #[cfg(feature = "shuffle")]
    #[instrument(skip_all, fields(seed))]
    pub fn reshuffle(&mut self, seed: u64) {
        use elicitation_rand::SeedableRng;
        let rng = elicitation_rand::StdRng::seed_from_u64(seed);
        let shuffle_gen = elicitation_rand::RandomGenerator::<u64>::new(rng);
        for i in (1..self.cards.len()).rev() {
            let j = shuffle_gen.generate() as usize % (i + 1);
            self.cards.swap(i, j);
        }
        self.dealt.store(0, Ordering::Relaxed);
        tracing::debug!(total = self.cards.len(), "Shoe reshuffled");
    }
}

impl Default for Shoe {
    /// Creates a default single-deck shoe.
    ///
    /// With the `shuffle` feature, uses seed 0 for determinism.
    /// Without it, returns an unshuffled ordered shoe.
    fn default() -> Self {
        #[cfg(feature = "shuffle")]
        {
            Self::new(0, 1)
        }
        #[cfg(not(feature = "shuffle"))]
        {
            let mut cards = Vec::with_capacity(52);
            for suit in Suit::ALL {
                for rank in Rank::ALL {
                    cards.push(Card::new(rank, suit));
                }
            }
            Self {
                cards,
                dealt: AtomicUsize::new(0),
            }
        }
    }
}

impl Serialize for Shoe {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let remaining_cards = &self.cards[self.dealt.load(Ordering::Relaxed)..];
        let mut s = serializer.serialize_struct("Shoe", 1)?;
        s.serialize_field("cards", remaining_cards)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Shoe {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct ShoeHelper {
            cards: Vec<Card>,
        }
        let helper = ShoeHelper::deserialize(deserializer)?;
        Ok(Self {
            cards: helper.cards,
            dealt: AtomicUsize::new(0),
        })
    }
}

impl PartialEq for Shoe {
    fn eq(&self, other: &Self) -> bool {
        self.cards == other.cards
            && self.dealt.load(Ordering::Relaxed) == other.dealt.load(Ordering::Relaxed)
    }
}

/// Manual `KaniCompose` for `Shoe` — `AtomicUsize` prevents derive.
///
/// TODO: remove once a blanket `KaniCompose` impl covers `AtomicUsize` or
/// once `Shoe` migrates to a `Cell<usize>` that implements the trait.
#[cfg(kani)]
impl elicitation::KaniCompose for Shoe {
    fn kani_depth0() -> Self {
        Self {
            cards: Vec::<Card>::kani_depth0(),
            dealt: AtomicUsize::new(0),
        }
    }

    fn kani_depth1() -> Self {
        Self {
            cards: Vec::<Card>::kani_depth1(),
            dealt: AtomicUsize::new(0),
        }
    }

    fn kani_depth2() -> Self {
        Self {
            cards: Vec::<Card>::kani_depth2(),
            dealt: AtomicUsize::new(0),
        }
    }
}
