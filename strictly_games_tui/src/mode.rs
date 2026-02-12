//! Game mode selection.

/// Game mode - who is the opponent?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Human vs Simple AI  
    HumanVsAI,
    /// Human vs Agent (via MCP - requires manual setup)
    HumanVsAgent,
    /// AI vs AI (SimpleAI vs SimpleAI for testing)
    AIVsAI,
}

impl GameMode {
    /// Returns display name.
    pub fn name(&self) -> &str {
        match self {
            GameMode::HumanVsAI => "Human vs AI",
            GameMode::HumanVsAgent => "Human vs Agent",
            GameMode::AIVsAI => "AI vs AI",
        }
    }
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::HumanVsAI
    }
}
