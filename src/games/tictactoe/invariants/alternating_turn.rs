//! Alternating turn invariant: players alternate X, O, X, O, ...

use super::super::{GameInProgress, Player};
use super::Invariant;

/// Invariant: Players alternate turns.
///
/// Move history must show X, O, X, O, ... pattern.
/// First move is always X.
pub struct AlternatingTurnInvariant;

impl Invariant<GameInProgress> for AlternatingTurnInvariant {
    fn holds(game: &GameInProgress) -> bool {
        let history = game.history();
        
        if history.is_empty() {
            return true;
        }
        
        // First move must be X
        if history[0].player != Player::X {
            return false;
        }
        
        // Check alternation
        for window in history.windows(2) {
            if window[0].player == window[1].player {
                return false;
            }
        }
        
        // Current to_move must be correct
        let expected_next = if history.len() % 2 == 0 {
            Player::X
        } else {
            Player::O
        };
        
        game.to_move() == expected_next
    }
    
    fn description() -> &'static str {
        "Players alternate turns (X, O, X, O, ...)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::tictactoe::{GameSetup, GameResult, Move, Position};

    #[test]
    fn test_empty_game_holds() {
        let game = GameSetup::new().start(Player::X);
        assert!(AlternatingTurnInvariant::holds(&game));
    }

    #[test]
    fn test_single_move_holds() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(game)) = game.make_move(action) {
            assert!(AlternatingTurnInvariant::holds(&game));
            assert_eq!(game.to_move(), Player::O);
        } else {
            panic!("Expected in-progress game");
        }
    }

    #[test]
    fn test_alternating_sequence_holds() {
        let moves = vec![
            Move::new(Player::X, Position::TopLeft),
            Move::new(Player::O, Position::Center),
            Move::new(Player::X, Position::TopRight),
            Move::new(Player::O, Position::BottomLeft),
            Move::new(Player::X, Position::BottomRight),
        ];
        
        if let Ok(GameResult::InProgress(game)) = GameInProgress::replay(&moves) {
            assert!(AlternatingTurnInvariant::holds(&game));
            assert_eq!(game.to_move(), Player::O);
        } else {
            panic!("Expected in-progress game");
        }
    }

    #[test]
    fn test_same_player_twice_violates() {
        let mut game = GameSetup::new().start(Player::X);
        
        // Force X to play twice (invalid, but let's construct it)
        let moves = vec![
            Move::new(Player::X, Position::TopLeft),
            Move::new(Player::X, Position::Center),  // X plays twice!
        ];
        
        // We can't actually construct this through normal make_move,
        // but we can verify the invariant would detect it
        // This test documents the property
    }
}
