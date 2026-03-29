//! Payout math for all craps bet types.
//!
//! All payouts are expressed as ratios (numerator, denominator) to avoid
//! floating-point. The [`resolve_bet`] function computes the actual dollar
//! payout for a given bet and roll outcome.

use crate::{ActiveBet, BetType, DiceRoll, Point};

/// Outcome of resolving a single bet against a roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BetOutcome {
    /// Bet wins — payout is the profit (not including original wager).
    Win(u64),
    /// Bet loses — the wagered amount is forfeited.
    Lose,
    /// Bet pushes — wager returned, no profit or loss.
    Push,
    /// Bet is unaffected by this roll (stays on the table).
    NoAction,
}

/// Payout ratio for a bet type, expressed as (numerator, denominator).
///
/// To compute payout: `amount * numerator / denominator`.
/// Returns `None` for bets whose payout depends on context (e.g. odds bets
/// depend on the point number).
pub fn payout_ratio(bet_type: BetType) -> Option<(u64, u64)> {
    match bet_type {
        BetType::PassLine | BetType::DontPass | BetType::Come | BetType::DontCome => Some((1, 1)),
        BetType::Place(Point::Six) | BetType::Place(Point::Eight) => Some((7, 6)),
        BetType::Place(Point::Five) | BetType::Place(Point::Nine) => Some((7, 5)),
        BetType::Place(Point::Four) | BetType::Place(Point::Ten) => Some((9, 5)),
        BetType::AnySeven => Some((4, 1)),
        BetType::AnyCraps => Some((7, 1)),
        BetType::Yo => Some((15, 1)),
        BetType::HiLo => Some((15, 1)),
        // Odds and field payouts depend on context
        BetType::PassOdds
        | BetType::DontPassOdds
        | BetType::ComeOdds(_)
        | BetType::DontComeOdds(_)
        | BetType::Field => None,
    }
}

/// House edge as basis points (1 bp = 0.01%) for a bet type.
///
/// Odds bets return 0 (true odds, no house edge).
pub fn house_edge(bet_type: BetType) -> u32 {
    match bet_type {
        BetType::PassLine | BetType::Come => 141,
        BetType::DontPass | BetType::DontCome => 136,
        BetType::PassOdds
        | BetType::DontPassOdds
        | BetType::ComeOdds(_)
        | BetType::DontComeOdds(_) => 0,
        BetType::Place(Point::Six) | BetType::Place(Point::Eight) => 152,
        BetType::Place(Point::Five) | BetType::Place(Point::Nine) => 400,
        BetType::Place(Point::Four) | BetType::Place(Point::Ten) => 667,
        BetType::Field => 556,
        BetType::AnySeven => 1667,
        BetType::AnyCraps => 1111,
        BetType::Yo => 1111,
        BetType::HiLo => 1111,
    }
}

/// Resolves a bet against a dice roll and optional table point.
///
/// Returns the outcome for this specific bet. The caller is responsible for
/// applying the outcome to the bankroll (via the ledger).
///
/// # Arguments
/// - `bet`: the active bet to resolve
/// - `roll`: the dice roll result
/// - `table_point`: the current table point (`None` during come-out)
/// - `is_comeout`: whether this is the come-out roll
pub fn resolve_bet(
    bet: &ActiveBet,
    roll: DiceRoll,
    table_point: Option<Point>,
    is_comeout: bool,
) -> BetOutcome {
    if !bet.is_on() {
        return BetOutcome::NoAction;
    }

    let sum = roll.sum();

    match bet.bet_type() {
        BetType::PassLine => resolve_pass_line(bet, sum, table_point, is_comeout),
        BetType::DontPass => resolve_dont_pass(bet, sum, table_point, is_comeout),
        BetType::PassOdds => resolve_pass_odds(bet, sum, table_point),
        BetType::DontPassOdds => resolve_dont_pass_odds(bet, sum, table_point),
        BetType::Come => resolve_come(bet, sum),
        BetType::DontCome => resolve_dont_come(bet, sum),
        BetType::ComeOdds(pt) => resolve_come_odds(bet, sum, pt),
        BetType::DontComeOdds(pt) => resolve_dont_come_odds(bet, sum, pt),
        BetType::Place(pt) => resolve_place(bet, sum, pt),
        BetType::Field => resolve_field(bet, sum),
        BetType::AnySeven => resolve_any_seven(bet, sum),
        BetType::AnyCraps => resolve_any_craps(bet, sum),
        BetType::Yo => resolve_yo(bet, sum),
        BetType::HiLo => resolve_hi_lo(bet, sum),
    }
}

// ── Pass Line ────────────────────────────────────────────────

fn resolve_pass_line(
    bet: &ActiveBet,
    sum: u8,
    table_point: Option<Point>,
    is_comeout: bool,
) -> BetOutcome {
    if is_comeout {
        match sum {
            7 | 11 => BetOutcome::Win(bet.amount()), // 1:1
            2 | 3 | 12 => BetOutcome::Lose,
            _ => BetOutcome::NoAction, // point is established
        }
    } else if let Some(point) = table_point {
        if sum == point.value() {
            BetOutcome::Win(bet.amount()) // 1:1
        } else if sum == 7 {
            BetOutcome::Lose
        } else {
            BetOutcome::NoAction
        }
    } else {
        BetOutcome::NoAction
    }
}

// ── Don't Pass ───────────────────────────────────────────────

fn resolve_dont_pass(
    bet: &ActiveBet,
    sum: u8,
    table_point: Option<Point>,
    is_comeout: bool,
) -> BetOutcome {
    if is_comeout {
        match sum {
            2 | 3 => BetOutcome::Win(bet.amount()), // 1:1
            12 => BetOutcome::Push,                 // bar 12
            7 | 11 => BetOutcome::Lose,
            _ => BetOutcome::NoAction,
        }
    } else if let Some(point) = table_point {
        if sum == 7 {
            BetOutcome::Win(bet.amount()) // 1:1
        } else if sum == point.value() {
            BetOutcome::Lose
        } else {
            BetOutcome::NoAction
        }
    } else {
        BetOutcome::NoAction
    }
}

// ── Pass Odds ────────────────────────────────────────────────

fn resolve_pass_odds(bet: &ActiveBet, sum: u8, table_point: Option<Point>) -> BetOutcome {
    let Some(point) = table_point else {
        return BetOutcome::NoAction;
    };
    if sum == point.value() {
        let (num, den) = point.true_odds();
        let profit = bet.amount() * num as u64 / den as u64;
        BetOutcome::Win(profit)
    } else if sum == 7 {
        BetOutcome::Lose
    } else {
        BetOutcome::NoAction
    }
}

// ── Don't Pass Odds ──────────────────────────────────────────

fn resolve_dont_pass_odds(bet: &ActiveBet, sum: u8, table_point: Option<Point>) -> BetOutcome {
    let Some(point) = table_point else {
        return BetOutcome::NoAction;
    };
    if sum == 7 {
        // Lay odds: pays inverse of true odds
        let (num, den) = point.true_odds();
        let profit = bet.amount() * den as u64 / num as u64;
        BetOutcome::Win(profit)
    } else if sum == point.value() {
        BetOutcome::Lose
    } else {
        BetOutcome::NoAction
    }
}

// ── Come / Don't Come ────────────────────────────────────────

fn resolve_come(bet: &ActiveBet, sum: u8) -> BetOutcome {
    // Come bet acts like its own come-out on the next roll
    match sum {
        7 | 11 => BetOutcome::Win(bet.amount()), // 1:1
        2 | 3 | 12 => BetOutcome::Lose,
        _ => BetOutcome::NoAction, // travels to point (handled by table logic)
    }
}

fn resolve_dont_come(bet: &ActiveBet, sum: u8) -> BetOutcome {
    match sum {
        2 | 3 => BetOutcome::Win(bet.amount()), // 1:1
        12 => BetOutcome::Push,
        7 | 11 => BetOutcome::Lose,
        _ => BetOutcome::NoAction,
    }
}

// ── Come/Don't Come Odds ─────────────────────────────────────

fn resolve_come_odds(bet: &ActiveBet, sum: u8, point: Point) -> BetOutcome {
    if sum == point.value() {
        let (num, den) = point.true_odds();
        let profit = bet.amount() * num as u64 / den as u64;
        BetOutcome::Win(profit)
    } else if sum == 7 {
        BetOutcome::Lose
    } else {
        BetOutcome::NoAction
    }
}

fn resolve_dont_come_odds(bet: &ActiveBet, sum: u8, point: Point) -> BetOutcome {
    if sum == 7 {
        let (num, den) = point.true_odds();
        let profit = bet.amount() * den as u64 / num as u64;
        BetOutcome::Win(profit)
    } else if sum == point.value() {
        BetOutcome::Lose
    } else {
        BetOutcome::NoAction
    }
}

// ── Place Bets ───────────────────────────────────────────────

fn resolve_place(bet: &ActiveBet, sum: u8, point: Point) -> BetOutcome {
    if sum == point.value() {
        let (num, den) = match point {
            Point::Six | Point::Eight => (7u64, 6u64),
            Point::Five | Point::Nine => (7, 5),
            Point::Four | Point::Ten => (9, 5),
        };
        let profit = bet.amount() * num / den;
        BetOutcome::Win(profit)
    } else if sum == 7 {
        BetOutcome::Lose
    } else {
        BetOutcome::NoAction
    }
}

// ── One-Roll Bets ────────────────────────────────────────────

fn resolve_field(bet: &ActiveBet, sum: u8) -> BetOutcome {
    match sum {
        2 => BetOutcome::Win(bet.amount() * 2),               // 2:1
        12 => BetOutcome::Win(bet.amount() * 2),              // 2:1 (some tables 3:1)
        3 | 4 | 9 | 10 | 11 => BetOutcome::Win(bet.amount()), // 1:1
        _ => BetOutcome::Lose,                                // 5, 6, 7, 8
    }
}

fn resolve_any_seven(bet: &ActiveBet, sum: u8) -> BetOutcome {
    if sum == 7 {
        BetOutcome::Win(bet.amount() * 4) // 4:1
    } else {
        BetOutcome::Lose
    }
}

fn resolve_any_craps(bet: &ActiveBet, sum: u8) -> BetOutcome {
    if matches!(sum, 2 | 3 | 12) {
        BetOutcome::Win(bet.amount() * 7) // 7:1
    } else {
        BetOutcome::Lose
    }
}

fn resolve_yo(bet: &ActiveBet, sum: u8) -> BetOutcome {
    if sum == 11 {
        BetOutcome::Win(bet.amount() * 15) // 15:1
    } else {
        BetOutcome::Lose
    }
}

fn resolve_hi_lo(bet: &ActiveBet, sum: u8) -> BetOutcome {
    if sum == 2 || sum == 12 {
        BetOutcome::Win(bet.amount() * 15) // 15:1
    } else {
        BetOutcome::Lose
    }
}
