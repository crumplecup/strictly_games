//! Win detection logic for tic-tac-toe.

use super::super::{Board, Player, Position, Square};
use tracing::instrument;

/// Checks if there is a winner on the board.
///
/// Returns `Some(player)` if the player has three in a row,
/// `None` otherwise.
#[instrument]
pub fn check_winner(board: &Board) -> Option<Player> {
    const LINES: [[Position; 3]; 8] = [
        // Rows
        [Position::TopLeft, Position::TopCenter, Position::TopRight],
        [
            Position::MiddleLeft,
            Position::Center,
            Position::MiddleRight,
        ],
        [
            Position::BottomLeft,
            Position::BottomCenter,
            Position::BottomRight,
        ],
        // Columns
        [
            Position::TopLeft,
            Position::MiddleLeft,
            Position::BottomLeft,
        ],
        [
            Position::TopCenter,
            Position::Center,
            Position::BottomCenter,
        ],
        [
            Position::TopRight,
            Position::MiddleRight,
            Position::BottomRight,
        ],
        // Diagonals
        [Position::TopLeft, Position::Center, Position::BottomRight],
        [Position::TopRight, Position::Center, Position::BottomLeft],
    ];

    for [a, b, c] in LINES {
        let sq = board.get(a);
        if sq != Square::Empty && sq == board.get(b) && sq == board.get(c) {
            return match sq {
                Square::Occupied(player) => Some(player),
                Square::Empty => None,
            };
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_winner_empty_board() {
        let board = Board::new();
        assert_eq!(check_winner(&board), None);
    }

    #[test]
    fn test_winner_top_row() {
        let mut board = Board::new();
        board.set(Position::TopLeft, Square::Occupied(Player::X));
        board.set(Position::TopCenter, Square::Occupied(Player::X));
        board.set(Position::TopRight, Square::Occupied(Player::X));
        assert_eq!(check_winner(&board), Some(Player::X));
    }

    #[test]
    fn test_winner_diagonal() {
        let mut board = Board::new();
        board.set(Position::TopLeft, Square::Occupied(Player::O));
        board.set(Position::Center, Square::Occupied(Player::O));
        board.set(Position::BottomRight, Square::Occupied(Player::O));
        assert_eq!(check_winner(&board), Some(Player::O));
    }

    #[test]
    fn test_no_winner_incomplete() {
        let mut board = Board::new();
        board.set(Position::TopLeft, Square::Occupied(Player::X));
        board.set(Position::TopCenter, Square::Occupied(Player::X));
        assert_eq!(check_winner(&board), None);
    }
}
