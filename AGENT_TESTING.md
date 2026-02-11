# Testing Agent Integration

The TUI now supports agent players via MCP! Here's how to test it.

## Mode 1: Human vs SimpleAI (Default)

Just run the TUI:

```bash
cargo run -p strictly_games_tui
```

Play against the simple AI that picks the first available square.

## Mode 2: Human vs Agent (Manual Testing)

This demonstrates the full MCP architecture with a human acting as the agent.

### Terminal 1: Run the MCP Server

```bash
# Run the strictly_games MCP server
RUST_LOG=debug cargo run --bin strictly_games
```

The server is now waiting for MCP clients to connect via stdio.

### Terminal 2: Run Copilot CLI as Agent

```bash
# Connect copilot to the MCP server
copilot --additional-mcp-config @~/.copilot/mcp-config.json
```

Your `~/.copilot/mcp-config.json` should have:

```json
{
  "mcpServers": {
    "strictly-games": {
      "command": "target/release/strictly_games",
      "args": [],
      "env": {}
    }
  }
}
```

### Terminal 3: Run the TUI

```bash
# Edit main.rs to set mode to HumanVsAgent
# Change: let mode = GameMode::default();
# To:     let mode = GameMode::HumanVsAgent;

RUST_LOG=debug cargo run -p strictly_games_tui
```

### How to Play

1. **TUI shows it's your turn (Human / X)**: Press 1-9 to make a move
2. **TUI shows "AI is thinking..." (Agent / O's turn)**: 
   - Switch to Terminal 2 (copilot)
   - Ask copilot to make a move: "Call the make_move tool with position 4"
   - Or: "Show me the board with get_board, then make a move"
3. **TUI updates** when the agent's move comes through!

### What's Happening

```
TUI Orchestrator          MCP Server              Copilot Agent
     │                         │                        │
     ├─ Agent's turn           │                        │
     ├─ get_move() blocks...   │                        │
     │                         │                        │
     │                         │ ←── make_move(4) ───── │ (you type this)
     │                         │                        │
     │                         ├─ validates move        │
     │                         ├─ sends via channel     │
     │                         │                        │
     │ ←─────────────────────── 4                       │
     │                         │                        │
     ├─ applies move           │                        │
     ├─ updates UI             │                        │
     ├─ next turn...           │                        │
```

The MCP server's `make_move` tool sends the position through a channel directly to `AgentPlayer::get_move()`!

## Architecture Proof

This manual testing **proves** the architecture works:
- ✅ TUI orchestrator coordinates gameplay
- ✅ MCP server handles tool calls
- ✅ Channel bridges MCP server to orchestrator
- ✅ Agent (you!) makes moves via MCP tools
- ✅ Full game loop with agent participation

## Next: Full Automation

Future work:
- Add agent prompting/guidance so it calls tools automatically
- Or: Build agent wrapper that auto-calls tools based on game state
- Or: Use agent with function-calling that can autonomously use tools

But the hard part is done - the architecture works!
