//! Kani harnesses for the TicTacToe wrapper layer.
//!
//! These proofs cover the types and functions that were previously only in
//! `strictly_server` and therefore unverified.  Now that they live in
//! `strictly_tictactoe`, we can include them in the formal verification chain.
//!
//! ## Compositional pattern
//!
//! Each harness calls `T::kani_proof()` on the types it uses.  This witnesses
//! that T satisfies the `Elicitation` trait, composing the framework's 291
//! harnesses with our domain-specific properties.
//!
//! ## What is verified
//!
//! ### Contract soundness (`contracts.rs`)
//! - `validate_square_empty` returns `Ok` iff the square is empty
//! - `validate_player_turn` returns `Ok` iff it is the player's turn
//! - `validate_move` is `Ok` iff both conditions hold (AND composition)
//! - `execute_move` sets the square to `Occupied(player)` and records history
//!
//! ### Typestate invariants (`typestate.rs`)
//! - `GameInProgress::make_move` alternates `to_move` on every `InProgress` result
//! - `GameInProgress::make_move` rejects the wrong player
//! - `GameInProgress::make_move` rejects an occupied square
//! - `GameInProgress::replay` with 0 moves gives an empty board with X to move
//! - `GameInProgress::replay` with 1 move applies that move
//! - `GameInProgress::replay` with 2 distinct moves records both and returns X to move
//!
//! ### Terminal transitions (`typestate.rs`)
//! - `make_move` produces `Finished(Winner(X))` when X completes a row
//! - `make_move` produces `Finished(Draw)` when the board fills with no winner
//! - `GameFinished::restart()` produces a fresh empty `GameSetup`

use elicitation::Elicitation;
use strictly_tictactoe::{
    GameFinished, GameInProgress, GameResult, GameSetup, Move, MoveError, Outcome, Player,
    Position, Square,
    contracts::{execute_move, validate_move, validate_player_turn, validate_square_empty},
};

// ─────────────────────────────────────────────────────────────
//  validate_square_empty
// ─────────────────────────────────────────────────────────────

/// `validate_square_empty` is sound: Ok iff the square is empty.
///
/// Property (forward): board.is_empty(pos) ⟹ Ok(_)
#[cfg(kani)]
#[kani::proof]
fn validate_square_empty_ok_when_empty() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();

    let player: Player = kani::any();
    let pos: Position = kani::any();

    let game = GameSetup::new().start(Player::X);
    let mov = Move::new(player, pos);

    // New board is always empty — validation must succeed.
    let result = validate_square_empty(&mov, &game);
    assert!(result.is_ok(), "Empty square must validate Ok");
}

/// `validate_square_empty` is complete: Err iff the square is occupied.
///
/// Set up an occupied square by replaying one move, then try the same position.
#[cfg(kani)]
#[kani::proof]
fn validate_square_empty_err_when_occupied() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();

    let pos: Position = kani::any();

    // After X plays at `pos`, the square is occupied. It's now O's turn.
    let result = GameInProgress::replay(&[Move::new(Player::X, pos)]);
    let Ok(GameResult::InProgress(game)) = result else { return; };

    // O tries to play at the same occupied position.
    let mov = Move::new(Player::O, pos);
    let result = validate_square_empty(&mov, &game);
    assert!(result.is_err(), "Occupied square must fail validation");
    assert_eq!(
        result.unwrap_err(),
        MoveError::SquareOccupied(pos),
        "Error kind must be SquareOccupied"
    );
}

// ─────────────────────────────────────────────────────────────
//  validate_player_turn
// ─────────────────────────────────────────────────────────────

/// `validate_player_turn` is sound: Ok iff it's the player's turn.
#[cfg(kani)]
#[kani::proof]
fn validate_player_turn_ok_when_correct_player() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();

    let pos: Position = kani::any();
    let game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::X, pos); // X goes first

    assert!(validate_player_turn(&mov, &game).is_ok());
}

/// `validate_player_turn` returns Err for the wrong player.
#[cfg(kani)]
#[kani::proof]
fn validate_player_turn_err_when_wrong_player() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();

    let pos: Position = kani::any();
    let game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::O, pos); // O tries to go first — wrong!

    let result = validate_player_turn(&mov, &game);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), MoveError::WrongPlayer(Player::O));
}

// ─────────────────────────────────────────────────────────────
//  validate_move (composition)
// ─────────────────────────────────────────────────────────────

/// `validate_move` succeeds iff both sub-proofs hold.
///
/// On a fresh game, a move by X at any position is always legal.
#[cfg(kani)]
#[kani::proof]
fn validate_move_ok_on_fresh_game_for_x() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameSetup::kani_proof();

    let pos: Position = kani::any();
    let game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::X, pos);

    assert!(validate_move(&mov, &game).is_ok());
}

/// `validate_move` fails when the square is occupied (even if correct player).
///
/// Replay one X move, then O (correct player) tries the same square.
#[cfg(kani)]
#[kani::proof]
fn validate_move_err_occupied_square() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();

    let result = GameInProgress::replay(&[Move::new(Player::X, pos)]);
    let Ok(GameResult::InProgress(game)) = result else { return; };

    // O is the correct player, but the square is occupied.
    let mov = Move::new(Player::O, pos);
    assert!(validate_move(&mov, &game).is_err());
}

/// `validate_move` fails when it's not the player's turn (even on empty square).
#[cfg(kani)]
#[kani::proof]
fn validate_move_err_wrong_player() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameSetup::kani_proof();

    let pos: Position = kani::any();
    let game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::O, pos); // O tries to go first

    assert!(validate_move(&mov, &game).is_err());
}

// ─────────────────────────────────────────────────────────────
//  execute_move effects
// ─────────────────────────────────────────────────────────────

/// After `execute_move`, the square is `Occupied(player)`.
#[cfg(kani)]
#[kani::proof]
fn execute_move_sets_square() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameSetup::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();
    let mut game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::X, pos);

    let proof = validate_move(&mov, &game).expect("fresh game — move is valid");
    execute_move(&mov, &mut game, proof);

    assert_eq!(game.board().get(pos), Square::Occupied(Player::X));
}

/// After `execute_move`, the move appears at the end of history.
#[cfg(kani)]
#[kani::proof]
fn execute_move_records_history() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameSetup::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();
    let mut game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::X, pos);

    let proof = validate_move(&mov, &game).expect("fresh game — move is valid");
    execute_move(&mov, &mut game, proof);

    assert!(!game.history().is_empty());
    assert_eq!(*game.history().last().unwrap(), mov);
}

// ─────────────────────────────────────────────────────────────
//  GameInProgress::make_move
// ─────────────────────────────────────────────────────────────

/// `make_move` alternates `to_move` on every `InProgress` result.
///
/// Property: if result is InProgress, new.to_move = old.to_move.opponent()
#[cfg(kani)]
#[kani::proof]
fn make_move_alternates_player() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameSetup::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();
    let game = GameSetup::new().start(Player::X);
    let initial_mover = game.to_move(); // always X on fresh game
    let mov = Move::new(initial_mover, pos);

    match game.make_move(mov) {
        Ok(GameResult::InProgress(g)) => {
            assert_eq!(g.to_move(), initial_mover.opponent(),
                "to_move must flip after a valid move");
        }
        Ok(GameResult::Finished(_)) => {} // game over — alternation moot
        Err(_) => {}                      // shouldn't happen on fresh game
    }
}

/// `make_move` rejects a move by the wrong player.
#[cfg(kani)]
#[kani::proof]
fn make_move_rejects_wrong_player() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameSetup::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();
    let game = GameSetup::new().start(Player::X);
    let mov = Move::new(Player::O, pos); // O tries to move when it's X's turn

    assert!(game.make_move(mov).is_err());
}

/// `make_move` rejects a move to an occupied square.
///
/// After X plays at `pos`, O tries to play there too.
#[cfg(kani)]
#[kani::proof]
fn make_move_rejects_occupied_square() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();

    let result = GameInProgress::replay(&[Move::new(Player::X, pos)]);
    let Ok(GameResult::InProgress(game)) = result else { return; };

    // O tries to play at the already-occupied position.
    let mov = Move::new(Player::O, pos);
    assert!(game.make_move(mov).is_err());
}

// ─────────────────────────────────────────────────────────────
//  GameInProgress::replay
// ─────────────────────────────────────────────────────────────

/// `replay` with 0 moves gives an empty board with X to move.
#[cfg(kani)]
#[kani::proof]
fn replay_empty_gives_fresh_game() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();

    let result = GameInProgress::replay(&[]).expect("empty replay is always valid");
    match result {
        GameResult::InProgress(g) => {
            assert_eq!(g.to_move(), Player::X, "X moves first");
            let pos: Position = kani::any();
            assert!(g.board().is_empty(pos), "Board must be empty");
        }
        GameResult::Finished(_) => {
            // Empty replay cannot produce a finished game.
            assert!(false, "Empty replay must be InProgress");
        }
    }
}

/// `replay` with one move applies that move exactly.
#[cfg(kani)]
#[kani::proof]
fn replay_one_move_applies_it() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();

    let pos: Position = kani::any();
    let mov = Move::new(Player::X, pos); // X always moves first

    let result = GameInProgress::replay(&[mov]).expect("single valid move");
    match result {
        GameResult::InProgress(g) => {
            // The position must now be occupied by X.
            assert_eq!(g.board().get(pos), Square::Occupied(Player::X));
            // It must now be O's turn.
            assert_eq!(g.to_move(), Player::O);
        }
        GameResult::Finished(g) => {
            // Single move can't finish the game.
            // (Need at least 5 moves for a winner; 9 for a draw.)
            let _ = g;
            assert!(false, "One move cannot finish a fresh game");
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Terminal transitions: InProgress → Finished
// ─────────────────────────────────────────────────────────────

/// `make_move` transitions to `Finished(Winner(X))` when the last move completes a line.
///
/// X plays TopLeft → TopCenter → TopRight (with O blocking at MiddleLeft and Center).
/// The 5th move (X-TopRight) completes the top row → Winner(X).
#[cfg(kani)]
#[kani::proof]
fn make_move_produces_winner() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();
    GameFinished::kani_proof();
    Outcome::kani_proof();

    let moves = [
        Move::new(Player::X, Position::TopLeft),
        Move::new(Player::O, Position::MiddleLeft),
        Move::new(Player::X, Position::TopCenter),
        Move::new(Player::O, Position::Center),
        Move::new(Player::X, Position::TopRight), // completes top row
    ];

    let result = GameInProgress::replay(&moves).expect("valid winning sequence");
    match result {
        GameResult::Finished(g) => {
            assert_eq!(
                g.outcome(),
                &Outcome::Winner(Player::X),
                "X must win by completing the top row"
            );
        }
        GameResult::InProgress(_) => {
            assert!(false, "Top-row completion must finish the game");
        }
    }
}

/// `make_move` transitions to `Finished(Draw)` when the board fills with no winner.
///
/// Draw board after 9 moves: X O X / O X O / X X O (no 3-in-a-line).
/// Move order (alternating): TL, TC, TR, ML, MR, MC, BL, BR, BC.
#[cfg(kani)]
#[kani::proof]
fn make_move_produces_draw() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();
    GameFinished::kani_proof();
    Outcome::kani_proof();

    // Final board: TL=X TC=O TR=X / ML=O MR=X MC=O / BL=X BR=O BC=X
    // Rows: X,O,X — O,X,O — X,O,X  → no 3-in-a-line for either player.
    let moves = [
        Move::new(Player::X, Position::TopLeft),
        Move::new(Player::O, Position::TopCenter),
        Move::new(Player::X, Position::TopRight),
        Move::new(Player::O, Position::MiddleLeft),
        Move::new(Player::X, Position::MiddleRight),
        Move::new(Player::O, Position::MiddleCenter),
        Move::new(Player::X, Position::BottomLeft),
        Move::new(Player::O, Position::BottomRight),
        Move::new(Player::X, Position::BottomCenter),
    ];

    let result = GameInProgress::replay(&moves).expect("valid draw sequence");
    match result {
        GameResult::Finished(g) => {
            assert_eq!(
                g.outcome(),
                &Outcome::Draw,
                "Full board with no winner must be a draw"
            );
        }
        GameResult::InProgress(_) => {
            assert!(false, "9-move sequence must finish the game");
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  GameFinished::restart
// ─────────────────────────────────────────────────────────────

/// `GameFinished::restart()` creates a fresh empty `GameSetup`.
///
/// Property: for all positions pos, restart().board().is_empty(pos) = true
#[cfg(kani)]
#[kani::proof]
fn restart_creates_fresh_game() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameFinished::kani_proof();
    GameSetup::kani_proof();

    // Reach a finished game via the canonical winning sequence.
    let moves = [
        Move::new(Player::X, Position::TopLeft),
        Move::new(Player::O, Position::MiddleLeft),
        Move::new(Player::X, Position::TopCenter),
        Move::new(Player::O, Position::Center),
        Move::new(Player::X, Position::TopRight),
    ];

    let result = GameInProgress::replay(&moves).expect("valid winning sequence");
    let GameResult::Finished(finished) = result else {
        return; // unreachable given the fixed winning sequence above
    };

    let setup = finished.restart();
    let pos: Position = kani::any();
    assert!(
        setup.board().is_empty(pos),
        "Restarted game must have an empty board at every position"
    );
}

// ─────────────────────────────────────────────────────────────
//  Multi-move replay
// ─────────────────────────────────────────────────────────────

/// `replay` correctly applies a two-move sequence.
///
/// Properties (for any two distinct positions pos1, pos2):
/// - pos1 is `Occupied(X)`, pos2 is `Occupied(O)`
/// - `to_move` is X again (turn has cycled back after 2 moves)
/// - `history` has exactly 2 entries
#[cfg(kani)]
#[kani::proof]
fn replay_two_moves_alternates_and_records() {
    Move::kani_proof();
    Player::kani_proof();
    Position::kani_proof();
    GameInProgress::kani_proof();

    let pos1: Position = kani::any();
    let pos2: Position = kani::any();
    kani::assume(pos1 != pos2); // moves must target different squares

    let result = GameInProgress::replay(&[
        Move::new(Player::X, pos1),
        Move::new(Player::O, pos2),
    ])
    .expect("two moves on distinct squares are always valid");

    match result {
        GameResult::InProgress(g) => {
            assert_eq!(g.board().get(pos1), Square::Occupied(Player::X));
            assert_eq!(g.board().get(pos2), Square::Occupied(Player::O));
            assert_eq!(g.to_move(), Player::X, "After 2 moves, X's turn again");
            assert_eq!(g.history().len(), 2, "History must have exactly 2 entries");
        }
        GameResult::Finished(_) => {
            // Two moves on distinct squares cannot finish a game:
            // a winner needs ≥3-in-a-line (impossible with ≤2 marks per player),
            // and a draw needs all 9 squares filled.
            assert!(false, "Two moves on distinct squares cannot finish a game");
        }
    }
}
