//! Draw detection logic for tic-tac-toe.

use super::super::{Board, Square};
use tracing::instrument;

/// Checks if the board is full (all squares occupied).
///
/// A full board with no winner indicates a draw.
#[instrument]
pub fn is_full(board: &Board) -> bool {
    board.squares().iter().all(|s| *s != Square::Empty)
}

#[cfg(test)]
mod tests {
    use super::super::super::{Player, Position};
    use super::super::win::check_winner;
    use super::*;

    fn is_draw(board: &Board) -> bool {
        is_full(board) && check_winner(board).is_none()
    }

    #[test]
    fn test_empty_board_not_full() {
        let board = Board::new();
        assert!(!is_full(&board));
    }

    #[test]
    fn test_partial_board_not_full() {
        let mut board = Board::new();
        board.set(Position::Center, Square::Occupied(Player::X));
        assert!(!is_full(&board));
    }

    #[test]
    fn test_full_board() {
        let mut board = Board::new();
        for pos in [
            Position::TopLeft,
            Position::TopCenter,
            Position::TopRight,
            Position::MiddleLeft,
            Position::Center,
            Position::MiddleRight,
            Position::BottomLeft,
            Position::BottomCenter,
            Position::BottomRight,
        ] {
            board.set(pos, Square::Occupied(Player::X));
        }
        assert!(is_full(&board));
    }

    #[test]
    fn test_draw_detection() {
        let mut board = Board::new();
        // Create draw scenario: X O X / O X X / O X O
        board.set(Position::TopLeft, Square::Occupied(Player::X));
        board.set(Position::TopCenter, Square::Occupied(Player::O));
        board.set(Position::TopRight, Square::Occupied(Player::X));
        board.set(Position::MiddleLeft, Square::Occupied(Player::O));
        board.set(Position::Center, Square::Occupied(Player::X));
        board.set(Position::MiddleRight, Square::Occupied(Player::X));
        board.set(Position::BottomLeft, Square::Occupied(Player::O));
        board.set(Position::BottomCenter, Square::Occupied(Player::X));
        board.set(Position::BottomRight, Square::Occupied(Player::O));

        assert!(is_draw(&board));
    }

    #[test]
    fn test_not_draw_if_winner() {
        let mut board = Board::new();
        // X wins top row
        board.set(Position::TopLeft, Square::Occupied(Player::X));
        board.set(Position::TopCenter, Square::Occupied(Player::X));
        board.set(Position::TopRight, Square::Occupied(Player::X));
        board.set(Position::MiddleLeft, Square::Occupied(Player::O));
        board.set(Position::Center, Square::Occupied(Player::O));

        assert!(!is_draw(&board));
    }
}
