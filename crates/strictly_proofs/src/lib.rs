//! Strictly Proofs - Formal verification trifecta
//!
//! This crate contains formal verification harnesses for strictly_tictactoe
//! using three verification frameworks:
//!
//! - **Kani**: Symbolic execution with bounded model checking
//! - **Verus**: Linear temporal logic specifications
//! - **Creusot**: Why3-based deductive verification
//!
//! Each framework proves properties about the pure game logic in strictly_tictactoe.
//! This crate has minimal dependencies and can be verified independently.

#![warn(missing_docs)]

pub mod kani_proofs;
pub mod verus_proofs;
pub mod creusot_proofs;
