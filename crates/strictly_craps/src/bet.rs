//! Bet types for craps.
//!
//! Craps has many bet types with different house edges and payouts.
//! The [`BetType`] enum covers all standard bets, and [`ActiveBet`]
//! tracks a live bet on the table with its amount and state.

use elicitation::Elicit;
use serde::{Deserialize, Serialize};

use crate::Point;

/// All possible bet types at a craps table.
///
/// Ordered roughly by house edge (best to worst) and grouped by
/// the progressive lesson system.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit, schemars::JsonSchema,
)]
#[cfg_attr(kani, derive(kani::Arbitrary, elicitation::KaniCompose))]
pub enum BetType {
    // ── Lesson 1: Line bets (1.36–1.41%) ──
    /// Pass Line — wins on natural (7/11), loses on craps (2/3/12).
    /// During point phase, wins if point is made, loses on seven-out.
    PassLine,

    /// Don't Pass (bar 12) — opposite of Pass Line.
    /// Wins on craps (2/3), pushes on 12, loses on natural.
    /// During point phase, wins on seven-out, loses if point is made.
    DontPass,

    // ── Lesson 2: Odds bets (0% house edge) ──
    /// Free odds behind Pass Line — pays true odds.
    /// Requires an active Pass Line bet with established point.
    PassOdds,

    /// Free odds behind Don't Pass — lays true odds.
    /// Requires an active Don't Pass bet with established point.
    DontPassOdds,

    // ── Lesson 3: Come bets (1.36–1.41%) ──
    /// Come bet — like Pass Line but placed during point phase.
    /// Next roll is its "come-out": natural wins, craps loses, otherwise
    /// the come bet travels to that point number.
    Come,

    /// Don't Come — opposite of Come bet.
    DontCome,

    /// Odds on a Come bet that has traveled to a point.
    ComeOdds(Point),

    /// Odds on a Don't Come bet that has traveled to a point.
    DontComeOdds(Point),

    // ── Lesson 4: Place bets (1.52–6.67%) ──
    /// Place bet — bet that a specific number hits before 7.
    /// Can be turned on/off at will.
    Place(Point),

    // ── Lesson 5: One-roll bets (2.78–16.67%) ──
    /// Field — one-roll bet on 2, 3, 4, 9, 10, 11, 12.
    /// Pays 2:1 on 2, 2:1 on 12 (some tables 3:1), 1:1 on others.
    Field,

    /// Any Seven — one-roll bet that next roll is 7. 16.67% edge.
    AnySeven,

    /// Any Craps — one-roll bet that next roll is 2, 3, or 12. 11.11% edge.
    AnyCraps,

    /// Yo (Eleven) — one-roll bet on 11. 11.11% edge.
    Yo,

    /// Hi-Lo — one-roll bet on 2 or 12. 11.11% edge.
    HiLo,
}

impl BetType {
    /// Returns the lesson level at which this bet is unlocked (1–5).
    pub fn lesson_level(self) -> u8 {
        match self {
            BetType::PassLine | BetType::DontPass => 1,
            BetType::PassOdds | BetType::DontPassOdds => 2,
            BetType::Come | BetType::DontCome | BetType::ComeOdds(_) | BetType::DontComeOdds(_) => {
                3
            }
            BetType::Place(_) => 4,
            BetType::Field
            | BetType::AnySeven
            | BetType::AnyCraps
            | BetType::Yo
            | BetType::HiLo => 5,
        }
    }

    /// Human-readable label for this bet type.
    pub fn label(self) -> &'static str {
        match self {
            BetType::PassLine => "Pass Line",
            BetType::DontPass => "Don't Pass",
            BetType::PassOdds => "Pass Odds",
            BetType::DontPassOdds => "Don't Pass Odds",
            BetType::Come => "Come",
            BetType::DontCome => "Don't Come",
            BetType::ComeOdds(_) => "Come Odds",
            BetType::DontComeOdds(_) => "Don't Come Odds",
            BetType::Place(_) => "Place",
            BetType::Field => "Field",
            BetType::AnySeven => "Any Seven",
            BetType::AnyCraps => "Any Craps",
            BetType::Yo => "Yo (Eleven)",
            BetType::HiLo => "Hi-Lo",
        }
    }

    /// Returns true if this is a one-roll bet (resolved on the very next roll).
    pub fn is_one_roll(self) -> bool {
        matches!(
            self,
            BetType::Field | BetType::AnySeven | BetType::AnyCraps | BetType::Yo | BetType::HiLo
        )
    }

    /// Returns true if this bet requires an active line bet as prerequisite.
    pub fn requires_line_bet(self) -> bool {
        matches!(
            self,
            BetType::PassOdds
                | BetType::DontPassOdds
                | BetType::ComeOdds(_)
                | BetType::DontComeOdds(_)
        )
    }
}

impl std::fmt::Display for BetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BetType::Place(pt) => write!(f, "Place {}", pt),
            BetType::ComeOdds(pt) => write!(f, "Come Odds ({})", pt),
            BetType::DontComeOdds(pt) => write!(f, "Don't Come Odds ({})", pt),
            other => write!(f, "{}", other.label()),
        }
    }
}

/// A live bet on the table.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit, schemars::JsonSchema,
)]
#[cfg_attr(kani, derive(elicitation::KaniCompose))]
pub struct ActiveBet {
    /// What kind of bet.
    bet_type: BetType,
    /// Wagered amount.
    amount: u64,
    /// For come/don't-come bets that have traveled to a point.
    point: Option<Point>,
    /// Place bets can be toggled on/off without removing them.
    is_on: bool,
}

impl ActiveBet {
    /// Creates a new active bet.
    pub fn new(bet_type: BetType, amount: u64) -> Self {
        Self {
            bet_type,
            amount,
            point: None,
            is_on: true,
        }
    }

    /// Returns the bet type.
    pub fn bet_type(&self) -> BetType {
        self.bet_type
    }

    /// Returns the wagered amount.
    pub fn amount(&self) -> u64 {
        self.amount
    }

    /// Returns the point this bet has traveled to (for come/don't-come).
    pub fn point(&self) -> Option<Point> {
        self.point
    }

    /// Returns whether this bet is currently active.
    pub fn is_on(&self) -> bool {
        self.is_on
    }

    /// Sets the point for a come/don't-come bet that has traveled.
    pub fn with_point(mut self, point: Point) -> Self {
        self.point = Some(point);
        self
    }

    /// Toggles this bet on or off (for place bets).
    pub fn with_on(mut self, on: bool) -> Self {
        self.is_on = on;
        self
    }
}

impl std::fmt::Display for ActiveBet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let on_off = if self.is_on { "" } else { " (OFF)" };
        match self.point {
            Some(pt) => write!(f, "{} on {} ${}{}", self.bet_type, pt, self.amount, on_off),
            None => write!(f, "{} ${}{}", self.bet_type, self.amount, on_off),
        }
    }
}

/// Player action during the betting phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
pub enum BettingAction {
    /// Place a new bet of the given type and amount.
    PlaceBet(BetType, u64),
    /// Finished placing bets — ready to roll.
    Done,
}

impl std::fmt::Display for BettingAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BettingAction::PlaceBet(bt, amt) => write!(f, "Bet {} ${}", bt, amt),
            BettingAction::Done => write!(f, "Done betting"),
        }
    }
}
