//! Craps dice and game-rule invariant proofs.
//!
//! Uses "cloud of assumptions" pattern:
//! - Trust: Rust's type system (enums, bounds checks)
//! - Verify: Dice ranges, point classification, comeout exhaustiveness, payout ratios

use strictly_craps::{
    ActiveBet, BetOutcome, BetType, DiceRoll, DieFace, Point, house_edge, payout_ratio, resolve_bet,
};

// ─────────────────────────────────────────────────────────────
//  Dice invariants
// ─────────────────────────────────────────────────────────────

/// Verifies DieFace::value() is always in 1..=6.
///
/// Property: ∀f ∈ DieFace, value(f) ∈ {1, 2, 3, 4, 5, 6}
///
/// Cloud: Trust Rust's exhaustive enum matching
/// Verify: Our face-to-value mapping
#[cfg(kani)]
#[kani::proof]
fn die_face_value_bounded() {
    let face: DieFace = kani::any();
    let v = face.value();
    assert!(v >= 1 && v <= 6, "DieFace value in 1..=6");
}

/// Verifies DiceRoll::sum() is always in 2..=12.
///
/// Property: ∀r ∈ DiceRoll, sum(r) ∈ {2, 3, ..., 12}
///
/// Cloud: Trust enum bounds
/// Verify: Our sum calculation
#[cfg(kani)]
#[kani::proof]
fn dice_roll_sum_bounded() {
    let roll: DiceRoll = kani::any();
    let s = roll.sum();
    assert!(s >= 2 && s <= 12, "Dice sum in 2..=12");
}

/// Verifies DieFace round-trip: from_value(value(f)) = Some(f).
///
/// Property: from_value is left-inverse of value
#[cfg(kani)]
#[kani::proof]
fn die_face_roundtrip() {
    let face: DieFace = kani::any();
    let v = face.value();
    let back = DieFace::from_value(v);
    assert_eq!(back, Some(face), "DieFace round-trip");
}

// ─────────────────────────────────────────────────────────────
//  Point invariants
// ─────────────────────────────────────────────────────────────

/// Verifies Point enum only contains valid point values.
///
/// Property: ∀p ∈ Point, value(p) ∈ {4, 5, 6, 8, 9, 10}
///
/// Cloud: Trust enum definition
/// Verify: Our value mapping has no gaps or invalid entries
#[cfg(kani)]
#[kani::proof]
fn point_values_are_valid() {
    let point: Point = kani::any();
    let v = point.value();
    assert!(
        v == 4 || v == 5 || v == 6 || v == 8 || v == 9 || v == 10,
        "Point value is a valid craps point"
    );
}

/// Verifies Point round-trip: from_sum(value(p)) = Some(p).
#[cfg(kani)]
#[kani::proof]
fn point_roundtrip() {
    let point: Point = kani::any();
    let v = point.value();
    let back = Point::from_sum(v);
    assert_eq!(back, Some(point), "Point round-trip");
}

/// Verifies from_sum returns None for non-point sums.
///
/// Property: 7 is never a point
#[cfg(kani)]
#[kani::proof]
fn seven_is_not_a_point() {
    assert!(Point::from_sum(7).is_none(), "7 is not a point");
}

/// Verifies from_sum returns None for craps numbers.
#[cfg(kani)]
#[kani::proof]
fn craps_numbers_are_not_points() {
    assert!(Point::from_sum(2).is_none(), "2 is not a point");
    assert!(Point::from_sum(3).is_none(), "3 is not a point");
    assert!(Point::from_sum(11).is_none(), "11 is not a point");
    assert!(Point::from_sum(12).is_none(), "12 is not a point");
}

// ─────────────────────────────────────────────────────────────
//  Come-out roll classification exhaustiveness
// ─────────────────────────────────────────────────────────────

/// Verifies every possible dice roll classifies as natural, craps, or point.
///
/// Property: ∀r ∈ DiceRoll, is_natural(r) ∨ is_craps(r) ∨ is_point(r)
///
/// Cloud: Trust enum exhaustiveness
/// Verify: Our classification covers all outcomes
#[cfg(kani)]
#[kani::proof]
fn comeout_classification_exhaustive() {
    let roll: DiceRoll = kani::any();
    let natural = roll.is_natural();
    let craps = roll.is_craps();
    let point = Point::from_sum(roll.sum()).is_some();

    assert!(
        natural || craps || point,
        "Every roll is natural, craps, or point"
    );
}

/// Verifies natural, craps, and point are mutually exclusive.
///
/// Property: at most one classification is true
#[cfg(kani)]
#[kani::proof]
fn comeout_classification_exclusive() {
    let roll: DiceRoll = kani::any();
    let n = roll.is_natural() as u8;
    let c = roll.is_craps() as u8;
    let p = Point::from_sum(roll.sum()).is_some() as u8;

    assert_eq!(n + c + p, 1, "Exactly one classification");
}

/// Verifies is_natural returns true only for 7 and 11.
///
/// Property: is_natural(r) ⟺ sum(r) ∈ {7, 11}
#[cfg(kani)]
#[kani::proof]
fn natural_values_correct() {
    let roll: DiceRoll = kani::any();
    let s = roll.sum();
    assert_eq!(
        roll.is_natural(),
        s == 7 || s == 11,
        "Naturals are exactly 7 and 11"
    );
}

/// Verifies is_craps returns true only for 2, 3, and 12.
///
/// Property: is_craps(r) ⟺ sum(r) ∈ {2, 3, 12}
#[cfg(kani)]
#[kani::proof]
fn craps_values_correct() {
    let roll: DiceRoll = kani::any();
    let s = roll.sum();
    assert_eq!(
        roll.is_craps(),
        s == 2 || s == 3 || s == 12,
        "Craps are exactly 2, 3, 12"
    );
}

// ─────────────────────────────────────────────────────────────
//  Payout invariants
// ─────────────────────────────────────────────────────────────

/// Verifies Pass Line pays 1:1.
///
/// Property: payout_ratio(PassLine) = (1, 1)
#[cfg(kani)]
#[kani::proof]
fn pass_line_payout_is_even_money() {
    let ratio = payout_ratio(BetType::PassLine);
    assert_eq!(ratio, Some((1, 1)), "Pass Line pays 1:1");
}

/// Verifies Don't Pass pays 1:1.
#[cfg(kani)]
#[kani::proof]
fn dont_pass_payout_is_even_money() {
    let ratio = payout_ratio(BetType::DontPass);
    assert_eq!(ratio, Some((1, 1)), "Don't Pass pays 1:1");
}

/// Verifies Place 6/8 pay 7:6.
#[cfg(kani)]
#[kani::proof]
fn place_six_eight_payout() {
    let r6 = payout_ratio(BetType::Place(Point::Six));
    let r8 = payout_ratio(BetType::Place(Point::Eight));
    assert_eq!(r6, Some((7, 6)), "Place 6 pays 7:6");
    assert_eq!(r8, Some((7, 6)), "Place 8 pays 7:6");
}

/// Verifies Place 5/9 pay 7:5.
#[cfg(kani)]
#[kani::proof]
fn place_five_nine_payout() {
    let r5 = payout_ratio(BetType::Place(Point::Five));
    let r9 = payout_ratio(BetType::Place(Point::Nine));
    assert_eq!(r5, Some((7, 5)), "Place 5 pays 7:5");
    assert_eq!(r9, Some((7, 5)), "Place 9 pays 7:5");
}

/// Verifies Place 4/10 pay 9:5.
#[cfg(kani)]
#[kani::proof]
fn place_four_ten_payout() {
    let r4 = payout_ratio(BetType::Place(Point::Four));
    let r10 = payout_ratio(BetType::Place(Point::Ten));
    assert_eq!(r4, Some((9, 5)), "Place 4 pays 9:5");
    assert_eq!(r10, Some((9, 5)), "Place 10 pays 9:5");
}

/// Verifies house edge is non-negative for all bet types.
///
/// Property: ∀b ∈ BetType, house_edge(b) >= 0.0
///
/// Cloud: Trust f64 arithmetic
/// Verify: No bet type accidentally has negative edge
#[cfg(kani)]
#[kani::proof]
fn house_edge_non_negative() {
    let bet: BetType = kani::any();
    let edge = house_edge(bet);
    assert!(edge >= 0.0, "House edge is non-negative");
}
