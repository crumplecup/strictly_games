//! Creusot proof gallery for strictly_games.
//!
//! Each level validates a specific pattern used in the generated VSM companions.
//! Levels build incrementally from trivial invariants to cross-crate field access.
//!
//! ## Progression
//!
//! | Level | Pattern                                          |
//! |-------|--------------------------------------------------|
//! | SGC1  | Inline enum, scalar `count@` invariant           |
//! | SGC2  | Inline struct, `Vec<T>@.len()` invariant         |
//! | SGC3  | Vec push transition preserving `@.len() <= 9`   |
//! | SGC4  | Full multi-state lifecycle, typestate wrappers   |
//! | SGC5  | Cross-crate real `TicTacToeState` field access   |

pub mod level1;
pub mod level2;
pub mod level3;
pub mod level4;
pub mod level5;
