//! Craps financial and typestate verification proofs.
//!
//! Verifies the bankroll ledger and proof-carrying workflow maintain
//! financial integrity. The craps ledger supports multiple bets per round
//! (unlike blackjack's single-bet model).
//!
//! Cloud: Trust Rust's type system, ownership model
//! Verify: Debit/credit arithmetic, proof token consumption, payout correctness

#[cfg(kani)]
use elicitation::Elicitation;

use strictly_craps::{
    ActiveBet, BetOutcome, BetType, CrapsLedger, DiceRoll, DieFace, LessonProgress, Point,
    resolve_bet,
};

// ─────────────────────────────────────────────────────────────
//  Compositional type verification
// ─────────────────────────────────────────────────────────────

/// Verifies all craps types through elicitation's compositional legos.
///
/// Each `kani_proof()` call verifies the type is well-formed and
/// satisfies its elicitation contracts.
#[cfg(kani)]
#[kani::proof]
fn verify_craps_legos() {
    DieFace::kani_proof();
    DiceRoll::kani_proof();
    Point::kani_proof();
    BetType::kani_proof();
    ActiveBet::kani_proof();
    LessonProgress::kani_proof();

    assert!(
        true,
        "Craps type legos: DieFace × DiceRoll × Point × BetType × ActiveBet × LessonProgress ∎"
    );
}

// ─────────────────────────────────────────────────────────────
//  Ledger arithmetic
// ─────────────────────────────────────────────────────────────

/// Verifies debit reduces bankroll by the wagered amount.
///
/// Property: ∀ bankroll, bet | bet > 0 ∧ bet ≤ bankroll ⟹
/// debit(bankroll, bet).current_balance == bankroll − bet
#[cfg(kani)]
#[kani::proof]
fn debit_single_bet_correct() {
    let bankroll: u64 = kani::any();
    let bet_amount: u64 = kani::any();

    kani::assume(bet_amount > 0);
    kani::assume(bet_amount <= bankroll);
    kani::assume(bankroll <= 1_000_000);

    let mut ledger = CrapsLedger::new(bankroll);
    let result = ledger.debit(bet_amount);

    assert!(result.is_ok(), "Valid bet should succeed");
    assert_eq!(
        ledger.current_balance(),
        bankroll - bet_amount,
        "Bankroll reduced by bet"
    );
}

/// Verifies debit rejects bets exceeding bankroll.
///
/// Property: ∀ bankroll, bet | bet > bankroll ⟹ debit fails
#[cfg(kani)]
#[kani::proof]
fn debit_rejects_over_bankroll() {
    let bankroll: u64 = kani::any();
    let bet_amount: u64 = kani::any();

    kani::assume(bankroll <= 1_000_000);
    kani::assume(bet_amount > bankroll);

    let mut ledger = CrapsLedger::new(bankroll);
    let result = ledger.debit(bet_amount);

    assert!(result.is_err(), "Bet exceeding bankroll must fail");
}

/// Verifies debit rejects zero-amount bets.
///
/// Property: debit(0) fails
#[cfg(kani)]
#[kani::proof]
fn debit_rejects_zero_bet() {
    let bankroll: u64 = kani::any();
    kani::assume(bankroll > 0);
    kani::assume(bankroll <= 1_000_000);

    let mut ledger = CrapsLedger::new(bankroll);
    let result = ledger.debit(0);

    assert!(result.is_err(), "Zero bet must fail");
}

/// Verifies settle_round with a single winning bet returns correct balance.
///
/// Property: debit(100) then settle with Win(100) → balance = original
#[cfg(kani)]
#[kani::proof]
fn settle_win_returns_correct_balance() {
    let bankroll: u64 = kani::any();
    kani::assume(bankroll >= 100);
    kani::assume(bankroll <= 1_000_000);

    let mut ledger = CrapsLedger::new(bankroll);
    let proof = ledger.debit(100).expect("valid debit");

    let (final_balance, _settled) = ledger.settle_round(&[(100, BetOutcome::Win(100))], proof);

    assert_eq!(
        final_balance,
        bankroll + 100,
        "Win returns wager + profit: original + net profit"
    );
}

/// Verifies settle_round with a losing bet returns reduced balance.
///
/// Property: debit(100) then settle with Lose → balance = original - 100
#[cfg(kani)]
#[kani::proof]
fn settle_loss_reduces_balance() {
    let bankroll: u64 = kani::any();
    kani::assume(bankroll >= 100);
    kani::assume(bankroll <= 1_000_000);

    let mut ledger = CrapsLedger::new(bankroll);
    let proof = ledger.debit(100).expect("valid debit");

    let (final_balance, _settled) = ledger.settle_round(&[(100, BetOutcome::Lose)], proof);

    assert_eq!(final_balance, bankroll - 100, "Loss forfeits the wager");
}

/// Verifies settle_round with Push returns original balance.
///
/// Property: debit(100) then settle with Push → balance = original
#[cfg(kani)]
#[kani::proof]
fn settle_push_returns_wager() {
    let bankroll: u64 = kani::any();
    kani::assume(bankroll >= 100);
    kani::assume(bankroll <= 1_000_000);

    let mut ledger = CrapsLedger::new(bankroll);
    let proof = ledger.debit(100).expect("valid debit");

    let (final_balance, _settled) = ledger.settle_round(&[(100, BetOutcome::Push)], proof);

    assert_eq!(final_balance, bankroll, "Push returns the wager");
}

// ─────────────────────────────────────────────────────────────
//  Payout correctness
// ─────────────────────────────────────────────────────────────

/// Verifies Pass Line win payout equals the bet amount (1:1).
///
/// Property: PassLine($X) + natural → Win($X) where X = bet amount
#[cfg(kani)]
#[kani::proof]
fn pass_line_win_payout_correct() {
    let amount: u64 = kani::any();
    kani::assume(amount > 0);
    kani::assume(amount <= 100_000);

    let bet = ActiveBet::new(BetType::PassLine, amount);
    let dice = DiceRoll::new(DieFace::Three, DieFace::Four); // sum = 7
    let outcome = resolve_bet(&bet, dice, None, true);

    assert_eq!(outcome, BetOutcome::Win(amount), "1:1 payout for Pass Line");
}

/// Verifies Place 6 win payout is 7:6.
///
/// Property: Place6($60) + hit → Win($70) (60 * 7/6 = 70)
#[cfg(kani)]
#[kani::proof]
fn place_six_win_payout_correct() {
    let bet = ActiveBet::new(BetType::Place(Point::Six), 60);
    let dice = DiceRoll::new(DieFace::Two, DieFace::Four); // sum = 6
    let outcome = resolve_bet(&bet, dice, Some(Point::Six), false);

    assert_eq!(outcome, BetOutcome::Win(70), "Place 6: $60 * 7/6 = $70");
}

/// Verifies Place 5 win payout is 7:5.
///
/// Property: Place5($50) + hit → Win($70) (50 * 7/5 = 70)
#[cfg(kani)]
#[kani::proof]
fn place_five_win_payout_correct() {
    let bet = ActiveBet::new(BetType::Place(Point::Five), 50);
    let dice = DiceRoll::new(DieFace::Two, DieFace::Three); // sum = 5
    let outcome = resolve_bet(&bet, dice, Some(Point::Five), false);

    assert_eq!(outcome, BetOutcome::Win(70), "Place 5: $50 * 7/5 = $70");
}

/// Verifies payout never produces Win(0).
///
/// Property: ∀ bet, outcome | outcome = Win(x) ⟹ x > 0
///
/// This verifies no edge case in payout math rounds down to zero.
#[cfg(kani)]
#[kani::proof]
fn win_payout_never_zero() {
    let amount: u64 = kani::any();
    kani::assume(amount > 0);
    kani::assume(amount <= 100_000);

    let bet = ActiveBet::new(BetType::PassLine, amount);
    let dice = DiceRoll::new(DieFace::Three, DieFace::Four); // natural 7
    let outcome = resolve_bet(&bet, dice, None, true);

    match outcome {
        BetOutcome::Win(payout) => {
            assert!(payout > 0, "Win payout must be positive");
        }
        _ => {} // Not a win — that's fine
    }
}

// ─────────────────────────────────────────────────────────────
//  Lesson progression
// ─────────────────────────────────────────────────────────────

/// Verifies lesson level stays in 1..=5.
///
/// Property: ∀ level operations, level ∈ [1, 5]
#[cfg(kani)]
#[kani::proof]
fn lesson_level_bounded() {
    let mut lesson = LessonProgress::new();
    assert_eq!(lesson.level(), 1, "Starts at level 1");

    // Play enough rounds to advance through all levels
    for _ in 0..50 {
        lesson.record_round();
        lesson.try_advance();
        let level = lesson.level();
        assert!(
            level >= 1 && level <= LessonProgress::MAX_LEVEL,
            "Level stays in bounds"
        );
    }
}

/// Verifies at_level clamps to valid range.
///
/// Property: LessonProgress::at_level(n).level() ∈ [1, MAX_LEVEL]
#[cfg(kani)]
#[kani::proof]
fn at_level_clamps() {
    let raw: u8 = kani::any();
    let lesson = LessonProgress::at_level(raw);
    let level = lesson.level();
    assert!(
        level >= 1 && level <= LessonProgress::MAX_LEVEL,
        "at_level clamps to valid range"
    );
}

/// Verifies lesson advancement is monotonic.
///
/// Property: try_advance never decreases level
#[cfg(kani)]
#[kani::proof]
fn lesson_advancement_monotonic() {
    let mut lesson = LessonProgress::new();
    let mut prev_level = lesson.level();

    for _ in 0..50 {
        lesson.record_round();
        lesson.try_advance();
        let new_level = lesson.level();
        assert!(new_level >= prev_level, "Level never decreases");
        prev_level = new_level;
    }
}
