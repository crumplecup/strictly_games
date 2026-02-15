//! Kani arbitrary implementations for tic-tac-toe types.
//!
//! These implementations allow Kani to explore all possible values of our types
//! during model checking.

#[cfg(kani)]
use super::{Board, GameInProgress, Move, Player, Position, Square};

#[cfg(kani)]
impl kani::Arbitrary for Player {
    fn any() -> Self {
        if kani::any() {
            Player::X
        } else {
            Player::O
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Position {
    fn any() -> Self {
        let index: u8 = kani::any();
        kani::assume(index < 9);
        match index {
            0 => Position::TopLeft,
            1 => Position::TopCenter,
            2 => Position::TopRight,
            3 => Position::MiddleLeft,
            4 => Position::MiddleCenter,
            5 => Position::MiddleRight,
            6 => Position::BottomLeft,
            7 => Position::BottomCenter,
            8 => Position::BottomRight,
            _ => unreachable!(),
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Square {
    fn any() -> Self {
        if kani::any() {
            Square::Empty
        } else {
            Square::Occupied(kani::any())
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Move {
    fn any() -> Self {
        Move::new(kani::any(), kani::any())
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Board {
    fn any() -> Self {
        let squares: [Square; 9] = kani::any();
        Board::from_squares(squares)
    }
}

#[cfg(kani)]
impl kani::Arbitrary for GameInProgress {
    fn any() -> Self {
        let board: Board = kani::any();
        let to_move: Player = kani::any();
        
        // Generate history of moves
        let history_len: usize = kani::any();
        kani::assume(history_len <= 9);
        
        let mut history = Vec::with_capacity(history_len);
        for _ in 0..history_len {
            history.push(kani::any());
        }
        
        // Create game with arbitrary data
        // Note: This bypasses normal construction, allowing Kani to explore invalid states
        GameInProgress::from_parts(board, history, to_move)
    }
}
