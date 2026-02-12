# Elicitation-Based Architecture - Current Status

## What Was Required (That I Completely Missed)
Build tic-tac-toe using **elicitation framework** where the server creates an **interactive state machine** via MCP sampling/createMessage that traps the agent in a game loop until completion.

## What Was Built Instead (WRONG)
- Traditional request/response MCP tools
- Manual session management
- Tried notification wakers (fundamentally incompatible with MCP)
- Agent had to poll or be notified externally

## Current Implementation

### Types Ready for Elicitation ✅
```rust
// All types derive Elicit
#[derive(Elicit)]
pub enum Player { X, O }

#[derive(Elicit)]
pub struct Board { ... }

#[derive(Elicit)]
pub struct Move {
    pub position: u8,  // 0-8
}

#[derive(Elicit)]
pub struct GameState { ... }
```

### Stub Tool Created ⏳
`play_game` tool exists but doesn't actually use elicitation yet:
- Takes session_id and player_name
- Returns current game state
- TODO: Implement actual elicitation loop

## What Still Needs to Be Done

### 1. Implement Sampling Support
The server needs access to MCP sampling to call back to the agent:
```rust
// Current (stub):
pub async fn play_game(&self, Parameters(req): Parameters<PlayGameRequest>) 
    -> Result<CallToolResult, McpError>

// Needed:
pub async fn play_game(&self, peer: Peer<RoleServer>, Parameters(req): Parameters<PlayGameRequest>)
    -> Result<CallToolResult, McpError>
{
    loop {
        // Check game state
        match game.status() {
            GameStatus::Won | GameStatus::Draw => break,
            GameStatus::InProgress => {
                // Elicit move from agent using sampling
                let agent_move = Move::elicit(&peer).await?;
                
                // Apply move
                game.make_move(agent_move.position)?;
            }
        }
    }
}
```

### 2. Use Elicitation Trait
```rust
use elicitation::Elicitation;

// In the game loop:
let move_result = Move::elicit(&sampling_context).await?;
```

### 3. Handle Both Players
- Human plays via TUI (HTTP client)
- Agent plays via elicitation loop
- Server coordinates turns

### 4. Key Architectural Point
The agent should **never leave the play_game tool** until the game ends:
- Tool is called once
- Enters elicitation loop
- Agent responds to sampling/createMessage requests
- Loop continues: elicit move → validate → apply → check status → repeat
- Returns only when game is Won or Draw

## References
- `/home/erik/repos/elicitation/README.md` - Elicitation framework docs
- `/home/erik/repos/elicitation/tictactoe.md` - Game server POC design
- `/home/erik/repos/elicitation/examples/tool_composition.rs` - Example of elicitation tools

## Next Steps
1. Study elicitation sampling API
2. Implement proper `play_game` with elicitation loop
3. Test agent vs agent gameplay
4. Integrate with TUI for human vs agent
