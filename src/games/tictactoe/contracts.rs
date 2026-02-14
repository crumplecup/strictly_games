//! Contract system for verified tic-tac-toe moves.
//!
//! This module defines propositions and proof-carrying functions that enforce
//! game rules at the type level. Invalid moves are unrepresentable.

use super::types::{Board, GameState, GameStatus, Move, Player};
use elicitation::contracts::{And, Established, Prop};
use tracing::instrument;

/// Proposition: The specified square is empty.
#[derive(Debug, Clone, Copy)]
pub struct SquareIsEmpty;
impl Prop for SquareIsEmpty {}

/// Proposition: The game is not over (still in progress).
#[derive(Debug, Clone, Copy)]
pub struct GameNotOver;
impl Prop for GameNotOver {}

/// Proposition: It is the specified player's turn.
#[derive(Debug, Clone, Copy)]
pub struct PlayersTurn;
impl Prop for PlayersTurn {}

/// Proposition: The move position is in bounds (0-8).
#[derive(Debug, Clone, Copy)]
pub struct MoveInBounds;
impl Prop for MoveInBounds {}

/// Composite proposition: A move is legal.
///
/// A legal move must satisfy ALL of:
/// - Square is empty
/// - Game is not over
/// - It's the player's turn
/// - Position is in bounds
pub type LegalMove = And<And<And<SquareIsEmpty, GameNotOver>, PlayersTurn>, MoveInBounds>;

/// Proposition: A move has been made.
#[derive(Debug, Clone, Copy)]
pub struct MoveMade;
impl Prop for MoveMade {}

/// Validates that a position is in bounds.
///
/// # Returns
///
/// Returns a proof of `MoveInBounds` if the position is 0-8.
#[instrument(skip(mv), fields(position = mv.position))]
pub fn validate_bounds(mv: &Move) -> Result<Established<MoveInBounds>, String> {
    if mv.position <= 8 {
        Ok(Established::assert())
    } else {
        Err(format!("Position {} out of bounds (must be 0-8)", mv.position))
    }
}

/// Validates that a square is empty.
///
/// # Returns
///
/// Returns a proof of `SquareIsEmpty` if the square at the move position is empty.
#[instrument(skip(board, mv), fields(position = mv.position))]
pub fn validate_square_empty(
    board: &Board,
    mv: &Move,
) -> Result<Established<SquareIsEmpty>, String> {
    if board.is_empty(mv.position as usize) {
        Ok(Established::assert())
    } else {
        Err(format!(
            "Square {} is already occupied",
            mv.position
        ))
    }
}

/// Validates that the game is not over.
///
/// # Returns
///
/// Returns a proof of `GameNotOver` if the game status is `InProgress`.
#[instrument(skip(state), fields(status = ?state.status()))]
pub fn validate_game_in_progress(
    state: &GameState,
) -> Result<Established<GameNotOver>, String> {
    if matches!(state.status(), GameStatus::InProgress) {
        Ok(Established::assert())
    } else {
        Err(format!("Game is over: {:?}", state.status()))
    }
}

/// Validates that it's the player's turn.
///
/// # Returns
///
/// Returns a proof of `PlayersTurn` if the current player matches.
#[instrument(skip(state), fields(player = ?player, current = ?state.current_player()))]
pub fn validate_players_turn(
    state: &GameState,
    player: Player,
) -> Result<Established<PlayersTurn>, String> {
    if state.current_player() == player {
        Ok(Established::assert())
    } else {
        Err(format!(
            "Not your turn. Current player: {:?}",
            state.current_player()
        ))
    }
}

/// Validates all conditions for a legal move and returns composite proof.
///
/// This is the main entry point for move validation. It checks all conditions
/// and returns a single proof that the move is legal.
///
/// # Arguments
///
/// * `state` - Current game state
/// * `mv` - The move to validate
/// * `player` - The player making the move
///
/// # Returns
///
/// Returns `Ok(proof)` if all conditions pass, where `proof` is `Established<LegalMove>`.
/// Returns `Err(msg)` with a descriptive error if any condition fails.
#[instrument(skip(state, mv), fields(player = ?player, position = mv.position))]
pub fn validate_move(
    state: &GameState,
    mv: &Move,
    player: Player,
) -> Result<Established<LegalMove>, String> {
    use elicitation::contracts::both;

    // Validate each condition
    let bounds_proof = validate_bounds(mv)?;
    let empty_proof = validate_square_empty(state.board(), mv)?;
    let game_proof = validate_game_in_progress(state)?;
    let turn_proof = validate_players_turn(state, player)?;

    // Compose proofs: ((empty ∧ game) ∧ turn) ∧ bounds
    let empty_and_game = both(empty_proof, game_proof);
    let with_turn = both(empty_and_game, turn_proof);
    let legal = both(with_turn, bounds_proof);

    Ok(legal)
}

/// Executes a move with proof of legality.
///
/// This function can only be called with a proof that the move is legal,
/// providing compile-time guarantees that all preconditions are met.
///
/// # Arguments
///
/// * `game` - Mutable game reference
/// * `mv` - The move to execute
/// * `_player` - The player making the move (unused, validated by proof)
/// * `_proof` - Proof that the move is legal (consumed)
///
/// # Returns
///
/// Returns a proof that a move was made.
#[instrument(skip(game, mv, _proof), fields(player = ?_player, position = mv.position))]
pub fn execute_move(
    game: &mut super::Game,
    mv: &Move,
    _player: Player,
    _proof: Established<LegalMove>,
) -> Established<MoveMade> {
    // At this point, all conditions are guaranteed by the proof
    game.make_move(mv.position as usize)
        .expect("Move must be valid with proof");

    Established::assert()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::tictactoe::Game;

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
}
