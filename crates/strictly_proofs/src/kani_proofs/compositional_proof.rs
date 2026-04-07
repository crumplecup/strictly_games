//! Compositional verification harnesses for tic-tac-toe.
//!
//! ## Compositional Verification Strategy
//!
//! This crate showcases elicitation's **verification trifecta** through composition:
//!
//! 1. **Framework Proofs** (elicitation_kani: 291 harnesses)
//!    - Primitive types (String, i32, bool, etc.)
//!    - Collections (Vec, HashMap, etc.)
//!    - External types (Url, Uuid, Regex, DateTime, etc.)
//!
//! 2. **Game Logic Proofs** (strictly_proofs: 13 + 14 harnesses)
//!    - Player.opponent() involution
//!    - Position.to_index() bounds
//!    - Board operations (get, set, is_empty)
//!    - Winner detection (rows, columns, diagonals)
//!    - Contract soundness (validate_move, execute_move)
//!    - Typestate invariants (make_move, replay)
//!
//! 3. **Composition Witnesses** (this file)
//!    - Primitive types: Player, Position, Square, Board
//!    - Wrapper types: Move, Outcome, GameSetup, GameInProgress, GameFinished, GameResult
//!    - Each `#[derive(Elicit)]` type composes framework + game proofs
//!    - Compilation proves type-safe composition ∎
//!
//! ## Cloud of Assumptions
//!
//! **Trust:**
//! - Elicitation framework's 291 Kani proofs
//! - Rust's type system (enums exhaustive, bounds checked)
//!
//! **Verify:**
//! - Game-specific invariants (opponent, winner, board state)
//! - Contract soundness (validate_move, execute_move)
//! - Typestate invariants (make_move, replay turn alternation)
//! - Type composition is sound (this file)
//!
//! ## The Compositional Proof
//!
//! Calling `T::kani_proof()` inside a Kani harness witnesses that T satisfies
//! the `Elicitation` trait, which means the framework's 291 proofs apply to T.
//! Combined with domain-specific harnesses, the full proof chain is:
//!   291 (framework) + 13 (game logic) + 14 (wrapper layer) = 318 total proofs

use elicitation::Elicitation;
use strictly_tictactoe::{
    Board, GameFinished, GameInProgress, GameResult, GameSetup, Move, Outcome, Player, Position,
    Square,
};

/// Compositional proof: framework verification composes with primitive game types.
///
/// Witnesses that Player, Position, Square, Board all implement Elicitation,
/// composing the framework's 291 proofs with our 13 game-logic harnesses.
#[cfg(kani)]
#[kani::proof]
fn verify_tictactoe_compositional() {
    // Witness: framework's 291 proofs compose through each primitive type.
    Player::kani_proof();
    Position::kani_proof();
    Square::kani_proof();
    Board::kani_proof();

    let _player_x = Player::X;
    let _player_o = Player::O;
    let _square_empty = Square::Empty;
    let _square_x = Square::Occupied(Player::X);
    let _board = Board::new();

    assert!(matches!(_player_x.opponent(), Player::O));
    assert!(matches!(_player_o.opponent(), Player::X));
    assert!(_board.is_empty(Position::Center));

    // ∴ Primitive type verification stack proven by composition ∎
}

/// Compositional proof: framework verification composes with wrapper types.
///
/// Witnesses that Move, Outcome, GameSetup, GameInProgress, GameFinished, and
/// GameResult all implement Elicitation, composing the framework's 291 proofs
/// with our 14 wrapper-layer harnesses.
///
/// Together with `verify_tictactoe_compositional` and the 27 property harnesses,
/// this closes the full TicTacToe verification chain:
///   291 (framework) + 13 (game logic) + 14 (wrapper layer) = 318 total proofs ∎
#[cfg(kani)]
#[kani::proof]
fn verify_tictactoe_wrapper_compositional() {
    // Witness: framework's 291 proofs compose through each wrapper type.
    Move::kani_proof();
    Outcome::kani_proof();
    GameSetup::kani_proof();
    GameInProgress::kani_proof();
    GameFinished::kani_proof();
    GameResult::kani_proof();

    // Spot-check that construction and composition work end-to-end.
    let setup = GameSetup::new();
    assert!(setup.board().is_empty(Position::Center));

    let game = GameSetup::new().start(Player::X);
    assert_eq!(game.to_move(), Player::X);

    let _outcome_win = Outcome::Winner(Player::X);
    let _outcome_draw = Outcome::Draw;

    // ∴ Wrapper type verification stack proven by composition ∎
}
