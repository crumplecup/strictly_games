//! Tests for tic-tac-toe contract system.

use strictly_games::{execute_move, validate_move, Game, Move, TicTacToePlayer as Player};

#[test]
fn test_validate_legal_move() {
    let game = Game::new();
    let state = game.state();
    let mv = Move { position: 4 }; // Center square

    let result = validate_move(state, &mv, Player::X);
    assert!(result.is_ok(), "Center square should be valid");
}

#[test]
fn test_validate_out_of_bounds() {
    let game = Game::new();
    let state = game.state();
    let mv = Move { position: 9 }; // Out of bounds

    let result = validate_move(state, &mv, Player::X);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("out of bounds"));
}

#[test]
fn test_validate_occupied_square() {
    let mut game = Game::new();
    game.make_move(4).expect("First move should succeed");
    let state = game.state();
    let mv = Move { position: 4 }; // Already occupied

    let result = validate_move(state, &mv, Player::O);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("occupied"));
}

#[test]
fn test_validate_wrong_turn() {
    let game = Game::new();
    let state = game.state();
    let mv = Move { position: 4 };

    // X goes first, so O shouldn't be able to move
    let result = validate_move(state, &mv, Player::O);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("Not your turn"));
}

#[test]
fn test_execute_move_with_proof() {
    let mut game = Game::new();
    let mv = Move { position: 4 };
    let player = Player::X;

    // Get proof that move is legal
    let proof = validate_move(game.state(), &mv, player).expect("Move should be valid");

    // Execute with proof
    let _move_made = execute_move(&mut game, &mv, player, proof);

    // Verify move was applied
    assert!(!game.state().board().is_empty(4));
}
