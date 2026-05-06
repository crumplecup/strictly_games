//! Auto-generated Kani foundation proofs.
//!
//! These files are written by `build.rs` at compile time by calling
//! `Type::kani_proof()` on each game type and deduplicating by function name.
//!
//! Do not edit the included files — regenerate with `cargo build -p strictly_proofs`.

pub mod blackjack_foundation;
pub mod blackjack_vsm;
pub mod craps_vsm;
pub mod tictactoe_foundation;
pub mod tictactoe_vsm;
