//! Tests for typestate game architecture.

use strictly_games::{GameInProgress, GameResult, GameSetup, Move, MoveError, Outcome, Position};
// Player from tictactoe is re-exported as TicTacToePlayer
use strictly_games::TicTacToePlayer as Player;

#[test]
fn test_typestate_lifecycle() {
    // Setup phase
    let game = GameSetup::new();

    // Start game
    let game = game.start(Player::X);
    assert_eq!(game.to_move(), Player::X);

    // Make moves
    let action = Move::new(Player::X, Position::Center);
    let result = game.make_move(action).expect("Valid move");

    let game = match result {
        GameResult::InProgress(g) => g,
        GameResult::Finished(_) => panic!("Game shouldn't finish after one move"),
    };

    assert_eq!(game.to_move(), Player::O);
}

#[test]
fn test_contracts_prevent_invalid_moves() {
    let game = GameSetup::new().start(Player::X);

    // Valid move
    let action = Move::new(Player::X, Position::Center);
    let result = game.make_move(action);
    assert!(result.is_ok());

    let game = match result.unwrap() {
        GameResult::InProgress(g) => g,
        GameResult::Finished(_) => panic!("Unexpected finish"),
    };

    // Try to play same square - should fail
    let action = Move::new(Player::O, Position::Center);
    let result = game.make_move(action);
    assert!(matches!(result, Err(MoveError::SquareOccupied(_))));
}

#[test]
fn test_wrong_player_rejected() {
    let game = GameSetup::new().start(Player::X);

    // Try to play as O when it's X's turn
    let action = Move::new(Player::O, Position::Center);
    let result = game.make_move(action);
    assert!(matches!(result, Err(MoveError::WrongPlayer(_))));
}

#[test]
fn test_replay_from_history() {
    let moves = vec![
        Move::new(Player::X, Position::Center),
        Move::new(Player::O, Position::TopLeft),
        Move::new(Player::X, Position::BottomRight),
        Move::new(Player::O, Position::TopRight),
        Move::new(Player::X, Position::BottomLeft),
    ];

    let result = GameInProgress::replay(&moves).expect("Valid replay");

    match result {
        GameResult::InProgress(game) => {
            assert_eq!(game.history().len(), 5);
            assert_eq!(game.to_move(), Player::O);
        }
        GameResult::Finished(_) => panic!("Game shouldn't finish"),
    }
}

#[test]
fn test_win_detection() {
    let moves = vec![
        Move::new(Player::X, Position::TopLeft),
        Move::new(Player::O, Position::Center),
        Move::new(Player::X, Position::TopCenter),
        Move::new(Player::O, Position::BottomLeft),
        Move::new(Player::X, Position::TopRight), // X wins top row
    ];

    let result = GameInProgress::replay(&moves).expect("Valid replay");

    match result {
        GameResult::Finished(game) => {
            assert_eq!(game.outcome(), &Outcome::Winner(Player::X));
        }
        GameResult::InProgress(_) => panic!("Game should be finished"),
    }
}

#[test]
fn test_draw_detection() {
    let moves = vec![
        Move::new(Player::X, Position::TopLeft),
        Move::new(Player::O, Position::Center),
        Move::new(Player::X, Position::TopRight),
        Move::new(Player::O, Position::TopCenter),
        Move::new(Player::X, Position::MiddleLeft),
        Move::new(Player::O, Position::MiddleRight),
        Move::new(Player::X, Position::BottomCenter),
        Move::new(Player::O, Position::BottomLeft),
        Move::new(Player::X, Position::BottomRight), // Draw
    ];

    let result = GameInProgress::replay(&moves).expect("Valid replay");

    match result {
        GameResult::Finished(game) => {
            assert_eq!(game.outcome(), &Outcome::Draw);
        }
        GameResult::InProgress(_) => panic!("Game should be finished"),
    }
}

#[test]
fn test_restart() {
    let game = GameSetup::new().start(Player::X);

    let action = Move::new(Player::X, Position::Center);
    let _result = game.make_move(action).unwrap();

    // Force finish by replaying to end
    let moves = vec![
        Move::new(Player::X, Position::TopLeft),
        Move::new(Player::O, Position::Center),
        Move::new(Player::X, Position::TopCenter),
        Move::new(Player::O, Position::BottomLeft),
        Move::new(Player::X, Position::TopRight),
    ];

    let result = GameInProgress::replay(&moves).unwrap();

    if let GameResult::Finished(game) = result {
        let new_game = game.restart();
        let new_game = new_game.start(Player::X);
        assert_eq!(new_game.to_move(), Player::X);
        assert!(new_game.history().is_empty());
    }
}
