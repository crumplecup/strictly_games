//! Craps scenario proofs — end-to-end game flow verification.
//!
//! Each harness constructs a specific craps situation and verifies the
//! typestate machine + payout logic produces the correct outcome.
//!
//! Cloud: Trust Rust's type system, enum exhaustiveness
//! Verify: Workflow transitions, payout correctness, phase ordering

use strictly_craps::{
    ActiveBet, BetOutcome, BetType, ComeOutOutput, DiceRoll, DieFace, GameSetup, Point,
    PointRollOutput, execute_comeout_roll, execute_place_bets, execute_point_roll, resolve_bet,
};

/// Helper: construct a DiceRoll from two face values.
fn roll(d1: u8, d2: u8) -> DiceRoll {
    DiceRoll::new(
        DieFace::from_value(d1).unwrap(),
        DieFace::from_value(d2).unwrap(),
    )
}

// ─────────────────────────────────────────────────────────────
//  Come-out scenarios
// ─────────────────────────────────────────────────────────────

/// Verifies come-out 7 resolves as natural (Pass Line wins).
///
/// Property: roll(3,4) on come-out → Resolved ∧ pass_line_won ∎
#[cfg(kani)]
#[kani::proof]
fn comeout_natural_seven() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    let dice = roll(3, 4); // sum = 7
    match execute_comeout_roll(comeout, dice, proof) {
        ComeOutOutput::Resolved(resolved, _settled) => {
            assert!(resolved.pass_line_won(), "Natural 7 wins Pass Line");
        }
        ComeOutOutput::PointSet(..) => {
            panic!("7 must not establish a point");
        }
    }
}

/// Verifies come-out 11 resolves as natural (Pass Line wins).
#[cfg(kani)]
#[kani::proof]
fn comeout_natural_eleven() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    let dice = roll(5, 6); // sum = 11
    match execute_comeout_roll(comeout, dice, proof) {
        ComeOutOutput::Resolved(resolved, _settled) => {
            assert!(resolved.pass_line_won(), "Natural 11 wins Pass Line");
        }
        ComeOutOutput::PointSet(..) => {
            panic!("11 must not establish a point");
        }
    }
}

/// Verifies come-out 2 (Snake Eyes) resolves as craps (Pass Line loses).
#[cfg(kani)]
#[kani::proof]
fn comeout_craps_two() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    let dice = roll(1, 1); // sum = 2
    match execute_comeout_roll(comeout, dice, proof) {
        ComeOutOutput::Resolved(resolved, _settled) => {
            assert!(!resolved.pass_line_won(), "Craps 2 loses Pass Line");
        }
        ComeOutOutput::PointSet(..) => {
            panic!("2 must not establish a point");
        }
    }
}

/// Verifies come-out 3 (Ace-Deuce) resolves as craps.
#[cfg(kani)]
#[kani::proof]
fn comeout_craps_three() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    let dice = roll(1, 2); // sum = 3
    match execute_comeout_roll(comeout, dice, proof) {
        ComeOutOutput::Resolved(resolved, _settled) => {
            assert!(!resolved.pass_line_won(), "Craps 3 loses Pass Line");
        }
        ComeOutOutput::PointSet(..) => {
            panic!("3 must not establish a point");
        }
    }
}

/// Verifies come-out 12 (Boxcars) resolves as craps.
#[cfg(kani)]
#[kani::proof]
fn comeout_craps_twelve() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    let dice = roll(6, 6); // sum = 12
    match execute_comeout_roll(comeout, dice, proof) {
        ComeOutOutput::Resolved(resolved, _settled) => {
            assert!(!resolved.pass_line_won(), "Craps 12 loses Pass Line");
        }
        ComeOutOutput::PointSet(..) => {
            panic!("12 must not establish a point");
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Point scenarios
// ─────────────────────────────────────────────────────────────

/// Verifies come-out 8 establishes point, then rolling 8 resolves as win.
///
/// Property: ComeOut(8) → PointPhase(8) → Roll(8) → Resolved ∧ pass_won ∎
#[cfg(kani)]
#[kani::proof]
fn point_made_eight() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    // Come-out: establish point 8
    let comeout_dice = roll(3, 5); // sum = 8
    let (point_phase, point_proof) = match execute_comeout_roll(comeout, comeout_dice, proof) {
        ComeOutOutput::PointSet(pp, pr) => (pp, pr),
        ComeOutOutput::Resolved(..) => panic!("8 must establish a point"),
    };

    assert_eq!(point_phase.point(), Point::Eight, "Point is 8");

    // Point phase: hit the point
    let point_dice = roll(2, 6); // sum = 8
    match execute_point_roll(point_phase, point_dice, point_proof) {
        PointRollOutput::Resolved(resolved, _settled) => {
            assert!(resolved.pass_line_won(), "Point 8 made → Pass Line wins");
        }
        PointRollOutput::Continue(..) => {
            panic!("Rolling the point must resolve");
        }
    }
}

/// Verifies seven-out during point phase (Pass Line loses).
///
/// Property: ComeOut(6) → PointPhase(6) → Roll(7) → Resolved ∧ !pass_won ∎
#[cfg(kani)]
#[kani::proof]
fn seven_out() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    // Come-out: establish point 6
    let comeout_dice = roll(1, 5); // sum = 6
    let (point_phase, point_proof) = match execute_comeout_roll(comeout, comeout_dice, proof) {
        ComeOutOutput::PointSet(pp, pr) => (pp, pr),
        ComeOutOutput::Resolved(..) => panic!("6 must establish a point"),
    };

    // Point phase: seven-out
    let seven_dice = roll(3, 4); // sum = 7
    match execute_point_roll(point_phase, seven_dice, point_proof) {
        PointRollOutput::Resolved(resolved, _settled) => {
            assert!(!resolved.pass_line_won(), "Seven-out → Pass Line loses");
        }
        PointRollOutput::Continue(..) => {
            panic!("Rolling 7 during point phase must resolve");
        }
    }
}

/// Verifies non-decision rolls during point phase continue.
///
/// Property: ComeOut(4) → PointPhase(4) → Roll(9) → Continue ∎
#[cfg(kani)]
#[kani::proof]
fn point_phase_no_decision() {
    let setup = GameSetup::new(1, 3);
    let betting = setup.start_betting(vec![1000]);
    let bets = vec![vec![ActiveBet::new(BetType::PassLine, 100)]];
    let (comeout, proof) = execute_place_bets(betting, bets).unwrap();

    // Come-out: establish point 4
    let comeout_dice = roll(1, 3); // sum = 4
    let (point_phase, point_proof) = match execute_comeout_roll(comeout, comeout_dice, proof) {
        ComeOutOutput::PointSet(pp, pr) => (pp, pr),
        ComeOutOutput::Resolved(..) => panic!("4 must establish a point"),
    };

    // Roll something that isn't the point or 7
    let other_dice = roll(4, 5); // sum = 9
    match execute_point_roll(point_phase, other_dice, point_proof) {
        PointRollOutput::Continue(next, _proof) => {
            assert_eq!(next.point(), Point::Four, "Point unchanged");
        }
        PointRollOutput::Resolved(..) => {
            panic!("9 is not a decision roll when point is 4");
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Bet resolution scenarios
// ─────────────────────────────────────────────────────────────

/// Verifies Pass Line wins on come-out natural.
///
/// Property: PassLine($100) + natural 7 → Win($100) ∎
#[cfg(kani)]
#[kani::proof]
fn pass_line_wins_on_natural() {
    let bet = ActiveBet::new(BetType::PassLine, 100);
    let dice = roll(3, 4); // sum = 7
    let outcome = resolve_bet(&bet, dice, None, true);
    assert_eq!(outcome, BetOutcome::Win(100), "Pass Line wins $100 on 7");
}

/// Verifies Don't Pass wins on come-out 2.
///
/// Property: DontPass($50) + craps 2 → Win($50) ∎
#[cfg(kani)]
#[kani::proof]
fn dont_pass_wins_on_craps_two() {
    let bet = ActiveBet::new(BetType::DontPass, 50);
    let dice = roll(1, 1); // sum = 2
    let outcome = resolve_bet(&bet, dice, None, true);
    assert_eq!(outcome, BetOutcome::Win(50), "Don't Pass wins on 2");
}

/// Verifies Don't Pass pushes on 12 (bar 12 rule).
///
/// Property: DontPass + craps 12 → Push ∎
#[cfg(kani)]
#[kani::proof]
fn dont_pass_pushes_on_twelve() {
    let bet = ActiveBet::new(BetType::DontPass, 50);
    let dice = roll(6, 6); // sum = 12
    let outcome = resolve_bet(&bet, dice, None, true);
    assert_eq!(
        outcome,
        BetOutcome::Push,
        "Don't Pass pushes on 12 (bar 12)"
    );
}
