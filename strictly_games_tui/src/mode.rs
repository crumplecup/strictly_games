//! Game mode selection.

/// Game mode - who is the opponent?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Human vs Simple AI  
    HumanVsAI,
    /// Human vs Agent (via MCP)
    HumanVsAgent,
}

impl GameMode {
    /// Returns display name.
    pub fn name(&self) -> &str {
        match self {
            GameMode::HumanVsAI => "Human vs AI",
            GameMode::HumanVsAgent => "Human vs Agent",
        }
    }
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::HumanVsAI
    }
}
