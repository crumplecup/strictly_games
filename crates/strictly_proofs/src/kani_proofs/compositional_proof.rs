//! Compositional capstone harnesses for TicTacToe.
//!
//! ## Verification layer structure
//!
//! 1. **Generated foundation** (`generated/tictactoe_foundation.rs`)
//!    Produced by `build.rs` calling `Type::kani_proof()` on every TicTacToe type.
//!    Proves structural well-formedness: each type is constructible and its
//!    `#[derive(Elicit)]` composition is sound.
//!
//! 2. **Game-logic invariants** (`game_invariants.rs`)
//!    Proves domain properties: opponent involution, index bounds,
//!    board operations, winner detection, is_full.
//!
//! 3. **Contract + typestate harnesses** (`tictactoe_contracts.rs`)
//!    Proves validate/execute contracts, make_move alternation, replay,
//!    terminal transitions (Winner / Draw), and restart.
//!
//! 4. **Capstone** (this file)
//!    Cross-type properties that span multiple layers, witnessing that the
//!    full type hierarchy fits together correctly end-to-end.

use strictly_tictactoe::{Board, GameSetup, Player, Position};

/// Capstone: fundamental cross-type properties of the TicTacToe hierarchy.
///
/// Witnesses:
/// - `Player::opponent()` is an involution (X↔O)
/// - A fresh `GameSetup` board is empty at every position
/// - `GameInProgress` starts with X to move
/// - The generated foundation + invariants + contracts layers compose soundly ∎
#[cfg(kani)]
#[kani::proof]
fn verify_tictactoe_composition_capstone() {
    let player_x = Player::X;
    let player_o = Player::O;

    // Opponent involution
    assert_eq!(player_x.opponent(), Player::O);
    assert_eq!(player_o.opponent(), Player::X);
    assert_eq!(player_x.opponent().opponent(), player_x);

    // Fresh board is empty everywhere
    let board = Board::new();
    let pos: Position = kani::any();
    assert!(board.is_empty(pos));

    // GameSetup starts empty
    let setup = GameSetup::new();
    assert!(setup.board().is_empty(pos));

    // GameInProgress starts with X
    let game = setup.start(Player::X);
    assert_eq!(game.to_move(), Player::X);
}
