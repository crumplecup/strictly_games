//! Craps workflow: propositions and proof-carrying tools.

mod propositions;
mod tools;

pub use propositions::{BetsPlaced, PointEstablished};
pub use tools::{
    ComeOutOutput, PointRollOutput, execute_comeout_roll, execute_place_bets, execute_point_roll,
};
