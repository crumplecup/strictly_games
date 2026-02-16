//! Elicitation router for type-safe LLM interaction.
//!
//! This module demonstrates the Elicitation Framework's core pattern:
//! The `elicit_router!` macro automatically generates MCP tools for each type,
//! enabling type-safe construction through LLM conversation.
//!
//! ## Generated Tools
//!
//! - `elicit_position` - Select pattern: LLM chooses from valid Position enum variants
//! - `elicit_player` - Select pattern: LLM chooses X or O
//!
//! These tools are the foundation for type-safe agent interaction.
//! Higher-level game tools compose these to enforce contracts and game rules.

#![allow(missing_docs)] // Macro-generated code

use crate::games::tictactoe::{Player, Position};
use elicitation::elicit_router;

elicit_router! {
    pub TicTacToeElicitRouter: Position, Player
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_exists() {
        let _router = TicTacToeElicitRouter;
    }

    #[test]
    fn test_tool_router_method() {
        let _tool_router = TicTacToeElicitRouter::tool_router();
    }
}
