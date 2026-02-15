//! Game rules for tic-tac-toe.
//!
//! This module contains pure functions for evaluating game state
//! according to tic-tac-toe rules. Rules are separated from board
//! storage to enable composition into contract systems.

pub mod draw;
pub mod win;

pub use draw::{is_draw, is_full};
pub use win::check_winner;
