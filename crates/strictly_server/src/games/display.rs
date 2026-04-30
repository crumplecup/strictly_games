//! `GameDisplay` — the core display trait for game state types.
//!
//! Mirrors `ArchiveDisplay` from the elicitation framework.  Every game state
//! type that can be rendered implements this to produce an AccessKit node
//! sub-tree.  The `*_to_verified_tree()` helpers in [`crate::tui::game_ir`]
//! wrap those sub-trees in the standard window/banner/status scaffolding.

use elicit_accesskit::{NodeId, NodeJson};

/// Trait implemented by every game display type to produce an AccessKit
/// node tree.
///
/// The `Mode` associated type captures the competing display strategies for
/// a given game state.  The implementation decides how to lay out nodes
/// depending on which mode is active.
pub trait GameDisplay {
    /// The set of supported display strategies for this type.
    type Mode: Default;

    /// Build the AccessKit node list for this state in the given mode.
    ///
    /// `id_base` is the starting `u64` for allocating [`NodeId`]s; callers
    /// must pass a value that does not overlap with other nodes in the same
    /// tree.  Returns `(root_id, nodes)`.
    fn to_ak_nodes(&self, mode: &Self::Mode, id_base: u64) -> (NodeId, Vec<(NodeId, NodeJson)>);
}
