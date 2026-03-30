//! Dice types for craps.
//!
//! A craps roll uses two standard six-sided dice. [`DieFace`] represents a
//! single die (1–6) and [`DiceRoll`] bundles two dice together, providing
//! the sum and classification helpers.
//!
//! # Random Generation
//!
//! When the `roll` feature is enabled, both types integrate with the
//! elicitation framework's [`Generator`] system via `#[derive(Rand)]`.
//! This provides seeded, deterministic dice generation:
//!
//! ```rust,ignore
//! use elicitation::Generator;
//! use strictly_craps::{DieFace, DiceRoll};
//!
//! // Create a deterministic dice generator
//! let dice = DiceRoll::random_generator(42);
//!
//! // Each call produces the next roll in the sequence
//! let roll = dice.generate();
//! println!("Rolled {} + {} = {}", roll.die1(), roll.die2(), roll.sum());
//! ```

use elicitation::{Elicit, Generator, Prompt, Select};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// A single die face (1–6).
///
/// Using an enum guarantees at the type level that a face can never be 0 or 7+.
///
/// With the `roll` feature, derives `Rand` for uniform random face selection
/// via `DieFace::random_generator(seed)`.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Elicit,
    strum::EnumIter,
)]
#[cfg_attr(feature = "roll", derive(elicitation_derive::Rand))]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum DieFace {
    /// Face showing 1.
    #[default]
    One = 1,
    /// Face showing 2.
    Two = 2,
    /// Face showing 3.
    Three = 3,
    /// Face showing 4.
    Four = 4,
    /// Face showing 5.
    Five = 5,
    /// Face showing 6.
    Six = 6,
}

impl DieFace {
    /// Returns the numeric value (1–6).
    pub fn value(self) -> u8 {
        self as u8
    }

    /// All six faces in order.
    pub const ALL: [DieFace; 6] = [
        DieFace::One,
        DieFace::Two,
        DieFace::Three,
        DieFace::Four,
        DieFace::Five,
        DieFace::Six,
    ];

    /// Creates a [`DieFace`] from a numeric value (1–6).
    ///
    /// Returns `None` for values outside 1..=6.
    #[instrument]
    pub fn from_value(v: u8) -> Option<Self> {
        match v {
            1 => Some(DieFace::One),
            2 => Some(DieFace::Two),
            3 => Some(DieFace::Three),
            4 => Some(DieFace::Four),
            5 => Some(DieFace::Five),
            6 => Some(DieFace::Six),
            _ => None,
        }
    }
}

impl std::fmt::Display for DieFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value())
    }
}

/// A roll of two dice.
///
/// The sum is always in 2..=12. Classification methods identify naturals,
/// craps, and potential point values.
///
/// # Random Generation
///
/// With the `roll` feature, use `DiceRoll::random_generator(seed)` to create
/// a deterministic dice generator. Each `generate()` call produces an
/// independent roll by composing two [`DieFace`] generators with split seeds:
///
/// ```rust,ignore
/// use elicitation::Generator;
/// use strictly_craps::DiceRoll;
///
/// let dice = DiceRoll::random_generator(42);
/// let roll = dice.generate(); // Independent, seeded roll
/// assert!(roll.sum() >= 2 && roll.sum() <= 12);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct DiceRoll {
    /// First die.
    die1: DieFace,
    /// Second die.
    die2: DieFace,
}

impl DiceRoll {
    /// Creates a new dice roll from two faces.
    pub fn new(die1: DieFace, die2: DieFace) -> Self {
        Self { die1, die2 }
    }

    /// Returns the first die.
    pub fn die1(self) -> DieFace {
        self.die1
    }

    /// Returns the second die.
    pub fn die2(self) -> DieFace {
        self.die2
    }

    /// Returns the sum of both dice (2–12).
    pub fn sum(self) -> u8 {
        self.die1.value() + self.die2.value()
    }

    /// Returns true for a natural (7 or 11) — pass line wins on come-out.
    pub fn is_natural(self) -> bool {
        let s = self.sum();
        s == 7 || s == 11
    }

    /// Returns true for craps (2, 3, or 12) — pass line loses on come-out.
    pub fn is_craps(self) -> bool {
        let s = self.sum();
        s == 2 || s == 3 || s == 12
    }

    /// Returns true if the sum establishes a point (4, 5, 6, 8, 9, 10).
    pub fn is_point_value(self) -> bool {
        crate::Point::from_sum(self.sum()).is_some()
    }

    /// Returns true if both dice show the same face ("hardway").
    pub fn is_hard(self) -> bool {
        self.die1 == self.die2
    }

    /// Returns true if this roll is a seven.
    pub fn is_seven(self) -> bool {
        self.sum() == 7
    }

    /// Returns the roll as a [`Point`] if the sum is a valid point value.
    pub fn as_point(self) -> Option<crate::Point> {
        crate::Point::from_sum(self.sum())
    }

    /// Creates a seeded dice generator using the elicitation framework.
    ///
    /// The generator composes two independent [`DieFace`] generators with
    /// split seeds, ensuring die1 and die2 are uncorrelated. Same seed
    /// always produces the same sequence — ideal for replays and testing.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use elicitation::Generator;
    /// use strictly_craps::DiceRoll;
    ///
    /// let dice = DiceRoll::random_generator(42);
    /// let roll1 = dice.generate();
    /// let roll2 = dice.generate();
    /// // Deterministic: same seed → same sequence
    /// ```
    #[cfg(feature = "roll")]
    pub fn random_generator(seed: u64) -> impl elicitation::Generator<Target = Self> {
        elicitation_rand::generators::MapGenerator::new(
            elicitation_rand::generators::RandomGenerator::<u64>::with_seed(seed),
            |inner_seed: u64| {
                // Split seed for independent dice
                let gen1 = DieFace::random_generator(inner_seed);
                let gen2 = DieFace::random_generator(inner_seed.wrapping_add(1));
                DiceRoll::new(gen1.generate(), gen2.generate())
            },
        )
    }

    /// All 36 possible dice roll combinations.
    pub fn all_combinations() -> impl Iterator<Item = DiceRoll> {
        DieFace::ALL
            .iter()
            .flat_map(|&d1| DieFace::ALL.iter().map(move |&d2| DiceRoll::new(d1, d2)))
    }
}

impl std::fmt::Display for DiceRoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}+{}={}", self.die1, self.die2, self.sum())
    }
}
