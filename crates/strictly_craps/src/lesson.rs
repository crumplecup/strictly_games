//! Progressive lesson system for craps.
//!
//! Craps has many bet types, which is overwhelming for beginners. The lesson
//! system gates bet types behind progression levels, introducing concepts
//! gradually as the player gains experience.

use elicitation::Elicit;
use serde::{Deserialize, Serialize};

use crate::BetType;

/// Tracks a player's progression through craps lessons.
///
/// Level 1 starts with just Pass/Don't Pass. Each level unlocks more bet
/// types, building understanding incrementally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elicit)]
pub struct LessonProgress {
    /// Current lesson level (1–5).
    level: u8,
    /// Total rounds played across all levels.
    rounds_played: u32,
    /// Rounds played at the current level.
    rounds_at_level: u32,
}

impl LessonProgress {
    /// Maximum lesson level.
    pub const MAX_LEVEL: u8 = 5;

    /// Creates a new progress tracker starting at lesson 1.
    pub fn new() -> Self {
        Self {
            level: 1,
            rounds_played: 0,
            rounds_at_level: 0,
        }
    }

    /// Creates a progress tracker at a specific level (for testing/skipping).
    pub fn at_level(level: u8) -> Self {
        let clamped = level.clamp(1, Self::MAX_LEVEL);
        Self {
            level: clamped,
            rounds_played: 0,
            rounds_at_level: 0,
        }
    }

    /// Returns the current lesson level.
    pub fn level(&self) -> u8 {
        self.level
    }

    /// Returns total rounds played.
    pub fn rounds_played(&self) -> u32 {
        self.rounds_played
    }

    /// Returns rounds played at the current level.
    pub fn rounds_at_level(&self) -> u32 {
        self.rounds_at_level
    }

    /// Records that a round has been completed.
    pub fn record_round(&mut self) {
        self.rounds_played += 1;
        self.rounds_at_level += 1;
    }

    /// Returns the number of rounds needed to advance from the current level.
    pub fn advance_threshold(&self) -> u32 {
        match self.level {
            1 => 5,        // Learn pass/don't pass basics
            2 => 5,        // Learn odds bets
            3 => 8,        // Learn come bets (more complex)
            4 => 8,        // Learn place bets
            _ => u32::MAX, // Level 5 is the final level
        }
    }

    /// Returns true if the player has played enough rounds to advance.
    pub fn can_advance(&self) -> bool {
        self.level < Self::MAX_LEVEL && self.rounds_at_level >= self.advance_threshold()
    }

    /// Advances to the next level if eligible. Returns true if advanced.
    pub fn try_advance(&mut self) -> bool {
        if self.can_advance() {
            self.level += 1;
            self.rounds_at_level = 0;
            true
        } else {
            false
        }
    }

    /// Returns all bet types available at the current lesson level.
    pub fn available_bets(&self) -> Vec<BetType> {
        let mut bets = Vec::new();

        // Level 1: Line bets
        if self.level >= 1 {
            bets.push(BetType::PassLine);
            bets.push(BetType::DontPass);
        }

        // Level 2: Odds bets
        if self.level >= 2 {
            bets.push(BetType::PassOdds);
            bets.push(BetType::DontPassOdds);
        }

        // Level 3: Come bets
        if self.level >= 3 {
            bets.push(BetType::Come);
            bets.push(BetType::DontCome);
            // ComeOdds/DontComeOdds are contextual (need a point)
        }

        // Level 4: Place bets
        if self.level >= 4 {
            for &pt in &crate::Point::ALL {
                bets.push(BetType::Place(pt));
            }
        }

        // Level 5: One-roll bets
        if self.level >= 5 {
            bets.push(BetType::Field);
            bets.push(BetType::AnySeven);
            bets.push(BetType::AnyCraps);
            bets.push(BetType::Yo);
            bets.push(BetType::HiLo);
        }

        bets
    }

    /// Returns true if a specific bet type is unlocked.
    pub fn is_unlocked(&self, bet: BetType) -> bool {
        bet.lesson_level() <= self.level
    }

    /// Returns educational text for the current level.
    pub fn lesson_text(&self) -> &'static str {
        match self.level {
            1 => concat!(
                "Lesson 1: Pass Line & Don't Pass\n",
                "The Pass Line is the most fundamental craps bet. On the come-out roll:\n",
                "• 7 or 11 → you win (\"natural\")\n",
                "• 2, 3, or 12 → you lose (\"craps\")\n",
                "• Any other number becomes \"the point\"\n",
                "Once a point is set, you win if the point is rolled again before a 7.\n",
                "\n",
                "Don't Pass is the opposite — you're betting with the house.\n",
                "It has a slightly better edge (1.36% vs 1.41%)."
            ),
            2 => concat!(
                "Lesson 2: Odds Bets — The Best Bet in the Casino\n",
                "After a point is established, you can place \"odds\" behind your line bet.\n",
                "Odds bets pay at TRUE ODDS — zero house edge!\n",
                "• Point 4 or 10: pays 2:1\n",
                "• Point 5 or 9: pays 3:2\n",
                "• Point 6 or 8: pays 6:5\n",
                "\n",
                "Always take maximum odds — it's the best bet on the table."
            ),
            3 => concat!(
                "Lesson 3: Come & Don't Come Bets\n",
                "Come bets work like Pass Line but are placed during the point phase.\n",
                "The next roll acts as a \"come-out\" for your Come bet:\n",
                "• 7 or 11 → Come wins\n",
                "• 2, 3, 12 → Come loses\n",
                "• Other numbers → your Come bet travels to that point\n",
                "\n",
                "You can have multiple points working at once with Come + odds!"
            ),
            4 => concat!(
                "Lesson 4: Place Bets\n",
                "Place bets let you bet directly on a number (4, 5, 6, 8, 9, 10).\n",
                "You win if your number hits before a 7.\n",
                "• Place 6 or 8: 1.52% edge (best place bets)\n",
                "• Place 5 or 9: 4.0% edge\n",
                "• Place 4 or 10: 6.67% edge\n",
                "\n",
                "Place bets can be turned on/off at any time — they're flexible."
            ),
            5 => concat!(
                "Lesson 5: Field & Proposition Bets — Know the Sucker Bets\n",
                "The Field bet wins on 2, 3, 4, 9, 10, 11, 12 (one roll).\n",
                "Looks great (7 numbers vs 4), but 5, 6, 7, 8 are rolled MORE often.\n",
                "House edge: ~5.56%\n",
                "\n",
                "Proposition bets (Any Seven, Any Craps, Yo, Hi-Lo) have edges of 11-17%.\n",
                "These are fun once in a while, but the math is heavily against you."
            ),
            _ => "You've mastered the craps table! Play smart and have fun.",
        }
    }

    /// Short title for the current level.
    pub fn lesson_title(&self) -> &'static str {
        match self.level {
            1 => "Pass Line & Don't Pass",
            2 => "Odds Bets",
            3 => "Come & Don't Come",
            4 => "Place Bets",
            5 => "Field & Propositions",
            _ => "Master",
        }
    }
}

impl Default for LessonProgress {
    fn default() -> Self {
        Self::new()
    }
}
