//! Point values for craps.
//!
//! A point is established on the come-out roll when the shooter rolls
//! 4, 5, 6, 8, 9, or 10. The enum guarantees only valid point values
//! exist — 2, 3, 7, 11, 12 are structurally excluded.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};

/// A valid craps point number.
///
/// Only 4, 5, 6, 8, 9, 10 can be points. The type system prevents
/// constructing invalid points (e.g. 7 or 2).
#[derive(
    Debug,
    Clone,
    Copy,
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
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Point {
    /// Point of 4 (3 ways to roll, true odds 2:1).
    Four = 4,
    /// Point of 5 (4 ways to roll, true odds 3:2).
    Five = 5,
    /// Point of 6 (5 ways to roll, true odds 6:5).
    Six = 6,
    /// Point of 8 (5 ways to roll, true odds 6:5).
    Eight = 8,
    /// Point of 9 (4 ways to roll, true odds 3:2).
    Nine = 9,
    /// Point of 10 (3 ways to roll, true odds 2:1).
    Ten = 10,
}

impl Point {
    /// All valid point values.
    pub const ALL: [Point; 6] = [
        Point::Four,
        Point::Five,
        Point::Six,
        Point::Eight,
        Point::Nine,
        Point::Ten,
    ];

    /// Returns the numeric value.
    pub fn value(self) -> u8 {
        self as u8
    }

    /// Creates a [`Point`] from a dice sum, returning `None` for non-point values.
    pub fn from_sum(sum: u8) -> Option<Self> {
        match sum {
            4 => Some(Point::Four),
            5 => Some(Point::Five),
            6 => Some(Point::Six),
            8 => Some(Point::Eight),
            9 => Some(Point::Nine),
            10 => Some(Point::Ten),
            _ => None,
        }
    }

    /// Number of ways to roll this point with two dice.
    ///
    /// This determines the true odds: fewer ways = harder to hit = higher payout.
    pub fn ways_to_roll(self) -> u8 {
        match self {
            Point::Four | Point::Ten => 3,
            Point::Five | Point::Nine => 4,
            Point::Six | Point::Eight => 5,
        }
    }

    /// Number of ways to roll a 7 (always 6).
    ///
    /// Used for odds calculations: `ways_to_seven / ways_to_roll` = against odds.
    pub const WAYS_TO_SEVEN: u8 = 6;

    /// True odds against making this point (as "X to Y" → returns (x, y)).
    ///
    /// - 4/10: 6:3 = 2:1
    /// - 5/9:  6:4 = 3:2
    /// - 6/8:  6:5
    pub fn true_odds(self) -> (u8, u8) {
        match self {
            Point::Four | Point::Ten => (2, 1),
            Point::Five | Point::Nine => (3, 2),
            Point::Six | Point::Eight => (6, 5),
        }
    }
}

impl std::fmt::Display for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value())
    }
}
