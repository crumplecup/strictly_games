//! Position enum with Select paradigm for tic-tac-toe moves.

use super::types::Board;
use elicitation::{Prompt, Select};
use serde::{Deserialize, Serialize};

/// A position on the tic-tac-toe board (0-8).
///
/// This enum uses the Select paradigm - agents choose from
/// a finite set of options. The game server filters which
/// positions are valid (unoccupied) before elicitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, elicitation::Elicit)]
pub enum Position {
    /// Top-left (position 0)
    TopLeft,
    /// Top-center (position 1)
    TopCenter,
    /// Top-right (position 2)
    TopRight,
    /// Middle-left (position 3)
    MiddleLeft,
    /// Center (position 4)
    Center,
    /// Middle-right (position 5)
    MiddleRight,
    /// Bottom-left (position 6)
    BottomLeft,
    /// Bottom-center (position 7)
    BottomCenter,
    /// Bottom-right (position 8)
    BottomRight,
}

impl Position {
    /// Converts position to board index (0-8).
    pub fn to_index(self) -> usize {
        match self {
            Position::TopLeft => 0,
            Position::TopCenter => 1,
            Position::TopRight => 2,
            Position::MiddleLeft => 3,
            Position::Center => 4,
            Position::MiddleRight => 5,
            Position::BottomLeft => 6,
            Position::BottomCenter => 7,
            Position::BottomRight => 8,
        }
    }

    /// Converts position to u8 (0-8).
    pub fn to_u8(self) -> u8 {
        self.to_index() as u8
    }

    /// Creates position from board index.
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Position::TopLeft),
            1 => Some(Position::TopCenter),
            2 => Some(Position::TopRight),
            3 => Some(Position::MiddleLeft),
            4 => Some(Position::Center),
            5 => Some(Position::MiddleRight),
            6 => Some(Position::BottomLeft),
            7 => Some(Position::BottomCenter),
            8 => Some(Position::BottomRight),
            _ => None,
        }
    }

    /// All 9 positions.
    pub const ALL: [Position; 9] = [
        Position::TopLeft,
        Position::TopCenter,
        Position::TopRight,
        Position::MiddleLeft,
        Position::Center,
        Position::MiddleRight,
        Position::BottomLeft,
        Position::BottomCenter,
        Position::BottomRight,
    ];

    /// Filters positions by board state - returns only empty squares.
    ///
    /// This is the key method for dynamic selection: we have a static
    /// enum with all positions, but filter which ones to present based
    /// on runtime board state.
    pub fn valid_moves(board: &Board) -> Vec<Position> {
        Self::ALL
            .iter()
            .copied()
            .filter(|pos| board.is_empty(pos.to_index()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{Player, Square};

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
        assert!(valid.contains(&Position::TopCenter));
    }
}
