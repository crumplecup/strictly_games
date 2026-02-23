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

// Kani proofs (run with: cargo kani)
#[cfg(kani)]
pub mod kani_proofs;

// Verus proofs (run with: verus --crate-type=lib src/lib.rs)
// NOTE: verus_proofs module is NOT included in cargo builds
// Verus has its own toolchain and doesn't go through cargo
// The module exists in the filesystem but is not declared here

// Creusot proofs (run with: cargo check)
#[cfg(not(kani))]
pub mod creusot_proofs;
