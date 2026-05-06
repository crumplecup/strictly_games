//! Verus proof gallery for strictly_games.
//!
//! Each level validates a specific pattern used in the generated Verus VSM companions.
//! All types are inline (Verus cannot resolve workspace crate dependencies).
//!
//! ## Progression
//!
//! | Level | Pattern                                                    |
//! |-------|------------------------------------------------------------|
//! | SGV1  | Inline enum, `match *state { ... count@ ... }` pattern     |
//! | SGV2  | Vec field, `history@.len()` bound in spec fn               |
//! | SGV3  | Multi-state lifecycle with assume_specification contracts  |

pub mod level1;
pub mod level2;
pub mod level3;
