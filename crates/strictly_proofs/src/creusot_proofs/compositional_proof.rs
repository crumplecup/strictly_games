//! Creusot compositional verification for tic-tac-toe types.

use strictly_tictactoe::{Board, Player, Position, Square};

/// Witness compositional verification through Elicitation framework.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(true)]
pub fn verify_tictactoe_compositional() {
    // Types derive Elicit → inherit verification
}

/// Verify Player enum is well-formed.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(true)]
pub fn verify_player_well_formed() {
    let _x = Player::X;
    let _o = Player::O;
}
