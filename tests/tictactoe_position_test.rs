//! Tests for tic-tac-toe position enum.

use strictly_games::{Board, Position, Square, TicTacToePlayer as Player};

#[test]
fn test_position_to_index() {
    assert_eq!(Position::TopLeft.to_index(), 0);
    assert_eq!(Position::Center.to_index(), 4);
    assert_eq!(Position::BottomRight.to_index(), 8);
}

#[test]
fn test_position_from_index() {
    assert_eq!(Position::from_index(0), Some(Position::TopLeft));
    assert_eq!(Position::from_index(4), Some(Position::Center));
    assert_eq!(Position::from_index(8), Some(Position::BottomRight));
    assert_eq!(Position::from_index(9), None);
}

#[test]
fn test_valid_moves_empty_board() {
    let board = Board::new();
    let valid = Position::valid_moves(&board);
    assert_eq!(valid.len(), 9); // All positions valid on empty board
}

#[test]
fn test_valid_moves_filters_occupied() {
    let mut board = Board::new();
    board.set(0, Square::Occupied(Player::X)).unwrap();
    board.set(4, Square::Occupied(Player::O)).unwrap();

    let valid = Position::valid_moves(&board);
    assert_eq!(valid.len(), 7); // 2 occupied, 7 free
    assert!(!valid.contains(&Position::TopLeft));
    assert!(!valid.contains(&Position::Center));
    assert!(valid.contains(&Position::BottomRight));
}
