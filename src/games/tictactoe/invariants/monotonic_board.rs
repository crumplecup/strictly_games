//! Monotonic board invariant: squares never change once set.

use super::super::{Board, GameInProgress, Square};
use super::Invariant;

/// Invariant: Board squares are monotonic (never overwritten).
///
/// Once a square transitions from Empty to Occupied, it never changes.
/// This is verified by replaying the move history and comparing.
pub struct MonotonicBoardInvariant;

impl Invariant<GameInProgress> for MonotonicBoardInvariant {
    fn holds(game: &GameInProgress) -> bool {
        // Reconstruct board from history
        let mut reconstructed = Board::new();
        
        for mov in game.history() {
            let pos = mov.position;
            let player = mov.player;
            
            // Square must be empty before placing
            if reconstructed.get(pos) != Square::Empty {
                return false;
            }
            
            reconstructed.set(pos, Square::Occupied(player));
        }
        
        // Reconstructed board must match current board
        reconstructed == *game.board()
    }
    
    fn description() -> &'static str {
        "Board squares are monotonic (never overwritten)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::tictactoe::{GameSetup, GameResult};

    #[test]
    fn test_empty_game_holds() {
        let game = GameSetup::new().start(Player::X);
        assert!(MonotonicBoardInvariant::holds(&game));
    }

    #[test]
    fn test_single_move_holds() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(game)) = game.make_move(action) {
            assert!(MonotonicBoardInvariant::holds(&game));
        } else {
            panic!("Expected in-progress game");
        }
    }

    #[test]
    fn test_multiple_moves_hold() {
        let moves = vec![
            Move::new(Player::X, Position::TopLeft),
            Move::new(Player::O, Position::Center),
            Move::new(Player::X, Position::TopRight),
            Move::new(Player::O, Position::BottomLeft),
        ];
        
        if let Ok(GameResult::InProgress(game)) = GameInProgress::replay(&moves) {
            assert!(MonotonicBoardInvariant::holds(&game));
        } else {
            panic!("Expected in-progress game");
        }
    }

    #[test]
    fn test_corrupted_board_violates() {
        let mut game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(mut game)) = game.make_move(action) {
            // Corrupt the board by changing an occupied square
            game.board.set(Position::Center, Square::Occupied(Player::O));
            
            // This should violate the invariant
            assert!(!MonotonicBoardInvariant::holds(&game));
        }
    }
}
