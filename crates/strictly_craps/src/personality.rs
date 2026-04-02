//! Agent personality types for AI co-players.
//!
//! Each personality defines a playing style that affects how AI agents make
//! craps betting decisions. The personality is communicated to the LLM via
//! a tailored system prompt — the game logic is identical regardless of
//! personality.

use std::fmt;

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};

/// Playing personality for an AI craps co-player.
///
/// Determines the system prompt and behavioural guidance given to the LLM
/// when it makes betting decisions.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    Elicit,
    schemars::JsonSchema,
)]
pub enum AgentPersonality {
    /// Sticks to Pass/Don't Pass, minimum bets, avoids proposition bets.
    Conservative,
    /// Mixes line bets with odds, moderate bet sizing, occasional place bets.
    #[default]
    Balanced,
    /// Maximum odds, aggressive place bets, willing to try proposition bets.
    Aggressive,
}

impl AgentPersonality {
    /// All personality variants in display order.
    pub const ALL: [AgentPersonality; 3] = [
        AgentPersonality::Conservative,
        AgentPersonality::Balanced,
        AgentPersonality::Aggressive,
    ];

    /// Display label for menus.
    pub fn label(self) -> &'static str {
        match self {
            Self::Conservative => "Conservative",
            Self::Balanced => "Balanced",
            Self::Aggressive => "Aggressive",
        }
    }

    /// Short flavour text for the TUI.
    pub fn description(self) -> &'static str {
        match self {
            Self::Conservative => "Careful player — sticks to line bets, minimum wagers",
            Self::Balanced => "Smart player — line bets with odds, moderate sizing",
            Self::Aggressive => "Bold player — max odds, place bets, occasional props",
        }
    }

    /// Generates the full system prompt for an LLM agent with this personality.
    ///
    /// The prompt instructs the LLM to:
    /// 1. Act as a craps player with the given personality
    /// 2. Respond with ONLY the option number or label (no explanation)
    /// 3. Follow personality-specific betting heuristics
    pub fn system_prompt(self) -> &'static str {
        match self {
            Self::Conservative => concat!(
                "You are an AI agent playing craps at a casino table. ",
                "Your personality is CONSERVATIVE. Your strategy:\n",
                "- Stick to Pass Line or Don't Pass bets only.\n",
                "- Bet near the table minimum — preserve your bankroll.\n",
                "- Avoid odds bets unless your bankroll is very healthy.\n",
                "- Never place proposition bets (Field, Any Seven, etc.).\n",
                "- If asked for a bet amount, choose a small number close to the minimum.\n",
                "\n",
                "Reply with ONLY the option number or option label — ",
                "no explanation, no punctuation, nothing else. ",
                "For bet amounts, reply with just the number."
            ),
            Self::Balanced => concat!(
                "You are an AI agent playing craps at a casino table. ",
                "Your personality is BALANCED. Your strategy:\n",
                "- Use Pass Line as your core bet.\n",
                "- Always take odds behind your line bet when available.\n",
                "- Bet around 5-10% of your bankroll per round.\n",
                "- Place bets on 6 and 8 when your lesson level allows.\n",
                "- Avoid high-edge proposition bets (Any Seven, Hi-Lo).\n",
                "- If asked for a bet amount, choose a moderate number.\n",
                "\n",
                "Reply with ONLY the option number or option label — ",
                "no explanation, no punctuation, nothing else. ",
                "For bet amounts, reply with just the number."
            ),
            Self::Aggressive => concat!(
                "You are an AI agent playing craps at a casino table. ",
                "Your personality is AGGRESSIVE. Your strategy:\n",
                "- Bet big — 15-25% of your bankroll per round.\n",
                "- Always take maximum odds behind your line bet.\n",
                "- Place bets on multiple numbers when available.\n",
                "- Occasionally try proposition bets for excitement.\n",
                "- Go for Field bets when feeling lucky.\n",
                "- If asked for a bet amount, choose a larger number.\n",
                "\n",
                "Reply with ONLY the option number or option label — ",
                "no explanation, no punctuation, nothing else. ",
                "For bet amounts, reply with just the number."
            ),
        }
    }
}

impl fmt::Display for AgentPersonality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
