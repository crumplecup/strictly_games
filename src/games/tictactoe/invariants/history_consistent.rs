//! History consistency invariant: history length matches occupied squares.

use super::super::{GameInProgress, Square};
use super::Invariant;

/// Invariant: History length equals number of occupied squares.
///
/// Every move in history corresponds to exactly one occupied square.
/// No moves are missing, no squares are filled without a move.
pub struct HistoryConsistentInvariant;

impl Invariant<GameInProgress> for HistoryConsistentInvariant {
    fn holds(game: &GameInProgress) -> bool {
        let history_len = game.history().len();
        
        let occupied_count = game.board()
            .squares()
            .iter()
            .filter(|s| **s != Square::Empty)
            .count();
        
        history_len == occupied_count
    }
    
    fn description() -> &'static str {
        "History length matches number of occupied squares"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::tictactoe::{GameSetup, GameResult, Move, Player, Position};

    #[test]
    fn test_empty_game_holds() {
        let game = GameSetup::new().start(Player::X);
        assert!(HistoryConsistentInvariant::holds(&game));
    }

    #[test]
    fn test_single_move_holds() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(game)) = game.make_move(action) {
            assert!(HistoryConsistentInvariant::holds(&game));
            assert_eq!(game.history().len(), 1);
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
            assert!(HistoryConsistentInvariant::holds(&game));
            assert_eq!(game.history().len(), 4);
        } else {
            panic!("Expected in-progress game");
        }
    }

    #[test]
    fn test_corrupted_history_violates() {
        let mut game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(mut game)) = game.make_move(action) {
            // Corrupt by adding extra square without history entry
            game.board.set(Position::TopLeft, Square::Occupied(Player::O));
            
            // This should violate the invariant
            assert!(!HistoryConsistentInvariant::holds(&game));
        }
    }

    #[test]
    fn test_full_game_holds() {
        let moves = vec![
            Move::new(Player::X, Position::TopLeft),
            Move::new(Player::O, Position::TopCenter),
            Move::new(Player::X, Position::TopRight),
            Move::new(Player::O, Position::MiddleLeft),
            Move::new(Player::X, Position::Center),
            Move::new(Player::O, Position::MiddleRight),
            Move::new(Player::X, Position::BottomCenter),
            Move::new(Player::O, Position::BottomLeft),
        ];
        
        if let Ok(GameResult::InProgress(game)) = GameInProgress::replay(&moves) {
            assert!(HistoryConsistentInvariant::holds(&game));
            assert_eq!(game.history().len(), 8);
        }
    }
}
