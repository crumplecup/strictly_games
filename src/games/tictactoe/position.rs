//! Position enum with Select paradigm for tic-tac-toe moves.

use super::types::Board;
use elicitation::{ElicitCommunicator, ElicitError, ElicitErrorKind, ElicitServer, Prompt, Select};
use rmcp::{Peer, RoleServer};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// A position on the tic-tac-toe board (0-8).
///
/// This enum uses the Select paradigm - agents choose from
/// a finite set of options. The game server filters which
/// positions are valid (unoccupied) before elicitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, elicitation::Elicit, strum::EnumIter)]
pub enum Position {
    /// Top-left (position 0)
    TopLeft,
    /// Top-center (position 1)
    TopCenter,
    /// Top-right (position 2)
    TopRight,
    /// Middle-left (position 3)
    MiddleLeft,
    /// Center (position 4)
    Center,
    /// Middle-right (position 5)
    MiddleRight,
    /// Bottom-left (position 6)
    BottomLeft,
    /// Bottom-center (position 7)
    BottomCenter,
    /// Bottom-right (position 8)
    BottomRight,
}

/// View struct for filtered position selection.
///
/// Wraps a dynamic list of valid positions to enable
/// context-aware selection through the framework.
#[derive(Debug, Clone)]
pub struct ValidPositions {
    /// Filtered list of valid positions
    pub positions: Vec<Position>,
}

impl Prompt for ValidPositions {}

impl Select for ValidPositions {
    fn options() -> &'static [Self] {
        // This will never be called - we override elicit_position() instead
        &[]
    }
    
    fn labels() -> &'static [&'static str] {
        // This will never be called - we override elicit_position() instead
        &[]
    }
    
    fn from_label(_label: &str) -> Option<Self> {
        // This will never be called - we override elicit_position() instead
        None
    }
}

impl ValidPositions {
    /// Elicit a position from the filtered list.
    pub async fn elicit_position(
        self,
        peer: Peer<RoleServer>,
    ) -> Result<Position, ElicitError> {
        // Build prompt with filtered options
        let mut prompt = String::from("Please select a Position:\n\nOptions:\n");
        for (idx, pos) in self.positions.iter().enumerate() {
            prompt.push_str(&format!("{}. {}\n", idx + 1, pos.label()));
        }
        prompt.push_str(&format!("\nRespond with the number (1-{}) or exact label:", self.positions.len()));
        
        // Use framework's ElicitServer
        let server = ElicitServer::new(peer);
        let response: String = server.send_prompt(&prompt).await?;
        
        // Parse response
        let selected = if let Ok(num) = response.trim().parse::<usize>() {
            if num >= 1 && num <= self.positions.len() {
                self.positions[num - 1]
            } else {
                return Err(ElicitError::new(ElicitErrorKind::ParseError(
                    format!("Invalid number: {}", num)
                )));
            }
        } else {
            Position::from_label(response.trim())
                .filter(|pos| self.positions.contains(pos))
                .ok_or_else(|| ElicitError::new(ElicitErrorKind::ParseError(
                    format!("Invalid position: {}", response)
                )))?
        };
        
        Ok(selected)
    }
}

impl Position {
    /// Get label for this position (for display).
    #[instrument]
    pub fn label(&self) -> &'static str {
        match self {
            Position::TopLeft => "Top-left",
            Position::TopCenter => "Top-center",
            Position::TopRight => "Top-right",
            Position::MiddleLeft => "Middle-left",
            Position::Center => "Center",
            Position::MiddleRight => "Middle-right",
            Position::BottomLeft => "Bottom-left",
            Position::BottomCenter => "Bottom-center",
            Position::BottomRight => "Bottom-right",
        }
    }

    /// Parse from label or number (0-8).
    #[instrument]
    pub fn from_label_or_number(s: &str) -> Option<Position> {
        // Try as number first (position index 0-8)
        if let Ok(num) = s.trim().parse::<usize>() {
            return Self::from_index(num);
        }

        // Try as label (case-insensitive, partial match)
        let s_lower = s.to_lowercase();
        <Position as strum::IntoEnumIterator>::iter().find(|pos| {
            let label = pos.label().to_lowercase();
            label.contains(&s_lower) || s_lower.contains(&label)
        })
    }
    
    /// Converts position to board index (0-8).
    #[instrument]
    pub fn to_index(self) -> usize {
        match self {
            Position::TopLeft => 0,
            Position::TopCenter => 1,
            Position::TopRight => 2,
            Position::MiddleLeft => 3,
            Position::Center => 4,
            Position::MiddleRight => 5,
            Position::BottomLeft => 6,
            Position::BottomCenter => 7,
            Position::BottomRight => 8,
        }
    }

    /// Converts position to u8 (0-8).
    #[instrument]
    pub fn to_u8(self) -> u8 {
        self.to_index() as u8
    }

    /// Creates position from board index.
    #[instrument]
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Position::TopLeft),
            1 => Some(Position::TopCenter),
            2 => Some(Position::TopRight),
            3 => Some(Position::MiddleLeft),
            4 => Some(Position::Center),
            5 => Some(Position::MiddleRight),
            6 => Some(Position::BottomLeft),
            7 => Some(Position::BottomCenter),
            8 => Some(Position::BottomRight),
            _ => None,
        }
    }

    /// All 9 positions.
    pub const ALL: [Position; 9] = [
        Position::TopLeft,
        Position::TopCenter,
        Position::TopRight,
        Position::MiddleLeft,
        Position::Center,
        Position::MiddleRight,
        Position::BottomLeft,
        Position::BottomCenter,
        Position::BottomRight,
    ];

    /// Filters positions by board state - returns only empty squares.
    ///
    /// This is the key method for dynamic selection: we have a static
    /// enum with all positions, but filter which ones to present based
    /// on runtime board state.
    #[instrument(skip(board))]
    pub fn valid_moves(board: &Board) -> Vec<Position> {
        Self::ALL
            .iter()
            .copied()
            .filter(|pos| board.is_empty(*pos))
            .collect()
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}
