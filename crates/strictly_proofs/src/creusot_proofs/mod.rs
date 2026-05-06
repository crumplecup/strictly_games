//! Creusot formal verification proofs for strictly_games.

#[cfg(creusot)]
pub mod gallery;

#[cfg(creusot)]
pub mod bankroll_financial;

#[cfg(creusot)]
pub mod compositional_proof;

#[cfg(creusot)]
pub mod game_invariants;

#[cfg(creusot)]
pub mod vsm_extern_specs;

#[cfg(creusot)]
pub mod tui_breakpoints;

pub mod generated;
