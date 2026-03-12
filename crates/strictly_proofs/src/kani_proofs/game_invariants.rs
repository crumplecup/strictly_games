//! Game invariant proofs for tic-tac-toe rules.
//!
//! Uses "cloud of assumptions" pattern:
//! - Trust: Rust's type system (enums, bounds checks)
//! - Verify: Game semantics (opponent, position, winner detection)

use strictly_tictactoe::{Board, Player, Position, Square, rules};

/// Verifies Player::opponent() is an involution.
///
/// Property: opponent(opponent(p)) = p
///
/// Cloud: Trust Rust's exhaustive enum matching
/// Verify: Our implementation of opponent logic
#[cfg(kani)]
#[kani::proof]
fn player_opponent_is_involutive() {
    let player: Player = kani::any();

    // opponent(opponent(x)) = x
    let once = player.opponent();
    let twice = once.opponent();

    assert_eq!(player, twice, "opponent is involutive");
}

/// Verifies opponent returns the other player.
///
/// Property: p ≠ opponent(p)
#[cfg(kani)]
#[kani::proof]
fn opponent_returns_other_player() {
    let player: Player = kani::any();
    let other = player.opponent();

    assert_ne!(player, other, "Opponent is different");
}

/// Verifies Position enum covers valid board indices.
///
/// Property: ∀p ∈ Position, to_index(p) ∈ [0, 8]
///
/// Cloud: Trust Rust's array bounds checking
/// Verify: Our index mapping logic
#[cfg(kani)]
#[kani::proof]
fn position_to_index_is_always_valid() {
    let pos: Position = kani::any();
    let idx = pos.to_index();

    // Index must be within 3x3 board
    assert!(idx < 9, "Index in bounds");
}

/// Verifies Position round-trip: from_index(to_index(p)) = Some(p)
///
/// Property: from_index is left-inverse of to_index
#[cfg(kani)]
#[kani::proof]
fn position_roundtrip() {
    let pos: Position = kani::any();
    let idx = pos.to_index();
    let back = Position::from_index(idx);

    assert_eq!(back, Some(pos), "Round-trip succeeds");
}

/// Verifies new board is empty.
///
/// Property: ∀p, Board::new().is_empty(p) = true
///
/// Cloud: Trust Vec initialization
/// Verify: Our board initialization logic
#[cfg(kani)]
#[kani::proof]
fn new_board_is_empty() {
    let board = Board::new();
    let pos: Position = kani::any();

    assert!(board.is_empty(pos), "New board is empty everywhere");
}

/// Verifies setting a square marks it occupied.
///
/// Property: set(b, p, Occupied(player)) ⟹ get(b', p) = Occupied(player)
#[cfg(kani)]
#[kani::proof]
fn set_marks_occupied() {
    let player: Player = kani::any();
    let pos: Position = kani::any();

    let mut board = Board::new();

    // Should be empty before
    assert!(board.is_empty(pos), "Empty before placement");

    // Set and verify
    board.set(pos, Square::Occupied(player));

    assert!(!board.is_empty(pos), "Occupied after placement");
    assert_eq!(board.get(pos), Square::Occupied(player), "Correct player");
}

/// Verifies get/set round-trip.
///
/// Property: set(b, p, s); get(b, p) = s
#[cfg(kani)]
#[kani::proof]
fn get_set_roundtrip() {
    let square: Square = kani::any();
    let pos: Position = kani::any();

    let mut board = Board::new();
    board.set(pos, square);
    let retrieved = board.get(pos);

    assert_eq!(retrieved, square, "get/set round-trip");
}

/// Verifies winner detection for rows.
///
/// Cloud: Trust our check_winner implementation
/// Verify: Specific winning configuration is recognized
#[cfg(kani)]
#[kani::proof]
fn winner_detects_row() {
    let player: Player = kani::any();

    let mut board = Board::new();

    // Fill top row
    board.set(Position::TopLeft, Square::Occupied(player));
    board.set(Position::TopCenter, Square::Occupied(player));
    board.set(Position::TopRight, Square::Occupied(player));

    let winner = rules::check_winner(&board);

    assert!(winner.is_some(), "Winner detected for complete row");
    assert_eq!(winner, Some(player), "Correct winner");
}

/// Verifies winner detection for columns.
#[cfg(kani)]
#[kani::proof]
fn winner_detects_column() {
    let player: Player = kani::any();

    let mut board = Board::new();

    // Fill left column
    board.set(Position::TopLeft, Square::Occupied(player));
    board.set(Position::MiddleLeft, Square::Occupied(player));
    board.set(Position::BottomLeft, Square::Occupied(player));

    let winner = rules::check_winner(&board);

    assert!(winner.is_some(), "Winner detected for complete column");
    assert_eq!(winner, Some(player), "Correct winner");
}

/// Verifies winner detection for diagonals.
#[cfg(kani)]
#[kani::proof]
fn winner_detects_diagonal() {
    let player: Player = kani::any();

    let mut board = Board::new();

    // Fill main diagonal
    board.set(Position::TopLeft, Square::Occupied(player));
    board.set(Position::Center, Square::Occupied(player));
    board.set(Position::BottomRight, Square::Occupied(player));

    let winner = rules::check_winner(&board);

    assert!(winner.is_some(), "Winner detected for complete diagonal");
    assert_eq!(winner, Some(player), "Correct winner");
}

/// Verifies no winner on empty board.
///
/// Property: check_winner(Board::new()) = None
#[cfg(kani)]
#[kani::proof]
fn no_winner_on_empty_board() {
    let board = Board::new();
    let winner = rules::check_winner(&board);

    assert_eq!(winner, None, "Empty board has no winner");
}

/// Verifies Square equality is well-defined.
///
/// Property: Square implements PartialEq correctly
#[cfg(kani)]
#[kani::proof]
fn square_equality() {
    let sq1: Square = kani::any();
    let sq2: Square = kani::any();

    // Equality is reflexive
    assert_eq!(sq1, sq1);

    // Equality is symmetric
    if sq1 == sq2 {
        assert_eq!(sq2, sq1);
    }
}
