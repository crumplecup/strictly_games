//! Simple AI that picks the first available square.

use crate::games::tictactoe::{types::Board, Position};

/// Returns the first empty position on the board.
pub fn pick_move(board: &Board) -> Option<Position> {
    Position::ALL.iter().copied().find(|&pos| board.is_empty(pos))
}
