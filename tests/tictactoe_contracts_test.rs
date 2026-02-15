//! Tests for tic-tac-toe typestate state machine.

use strictly_games::{Game, InProgress, Position, TicTacToePlayer as Player};

#[test]
fn test_place_legal_move() {
    let game = Game::<InProgress>::new();
    let result = game.place(Position::Center);
    assert!(result.is_ok(), "Center square should be valid");
}

#[test]
fn test_place_occupied_square() {
    let game = Game::<InProgress>::new();
    let game = match game.place(Position::Center).unwrap() {
        strictly_games::GameTransition::InProgress(g) => g,
        _ => panic!("First move shouldn't end game"),
    };

    let result = game.place(Position::Center);
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("occupied"));
}

#[test]
fn test_alternating_players() {
    let game = Game::<InProgress>::new();
    assert_eq!(game.to_move(), Player::X);

    let game = match game.place(Position::Center).unwrap() {
        strictly_games::GameTransition::InProgress(g) => g,
        _ => panic!("Game shouldn't end after first move"),
    };

    assert_eq!(game.to_move(), Player::O);
}

#[test]
fn test_game_transition_to_won() {
    let game = Game::<InProgress>::new();
    
    // X plays top row
    let game = match game.place(Position::TopLeft).unwrap() {
        strictly_games::GameTransition::InProgress(g) => g,
        _ => panic!("Shouldn't win yet"),
    };
    
    // O plays somewhere else
    let game = match game.place(Position::Center).unwrap() {
        strictly_games::GameTransition::InProgress(g) => g,
        _ => panic!("Shouldn't win yet"),
    };
    
    // X continues top row
    let game = match game.place(Position::TopCenter).unwrap() {
        strictly_games::GameTransition::InProgress(g) => g,
        _ => panic!("Shouldn't win yet"),
    };
    
    // O plays somewhere else
    let game = match game.place(Position::BottomLeft).unwrap() {
        strictly_games::GameTransition::InProgress(g) => g,
        _ => panic!("Shouldn't win yet"),
    };
    
    // X completes top row - wins!
    let result = game.place(Position::TopRight).unwrap();
    
    match result {
        strictly_games::GameTransition::Won(won_game) => {
            assert_eq!(won_game.winner(), Player::X);
        }
        _ => panic!("Should have won"),
    }
}

#[test]
fn test_terminal_state_cannot_place() {
    // This test demonstrates compile-time safety:
    // Game<Won> and Game<Draw> don't have .place() method
    // Uncommenting this would be a compile error:
    
    // let game = Game::<InProgress>::new();
    // let won_game = ...; // somehow get Game<Won>
    // won_game.place(Position::Center); // ❌ Compile error!
}

