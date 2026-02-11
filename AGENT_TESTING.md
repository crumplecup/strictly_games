# Testing Agent Integration

The TUI now supports agent players via MCP! Here's how to test it.

## Logging

All logs go to `strictly_games_tui.log` to avoid interfering with the TUI display.

```bash
# Watch logs in another terminal
tail -f strictly_games_tui.log
```

## Mode 1: Human vs SimpleAI (Default)

```bash
cargo run -p strictly_games_tui
# or explicitly:
cargo run -p strictly_games_tui ai
```

Play against the simple AI that picks the first available square.

## Mode 2: Human vs Agent (MCP Integration)

This demonstrates the full MCP architecture.

### Terminal 1: Run the MCP Server

```bash
# Run the strictly_games MCP server
cargo run --bin strictly_games
```

The server is now waiting for MCP clients to connect via stdio.

### Terminal 2: Connect Copilot CLI as Agent

```bash
# Connect copilot to the MCP server
copilot --additional-mcp-config @~/.copilot/mcp-config.json
```

Your `~/.copilot/mcp-config.json` should have:

```json
{
  "mcpServers": {
    "strictly-games": {
      "command": "/home/erik/repos/strictly_games/target/debug/strictly_games",
      "args": [],
      "env": {}
    }
  }
}
```

### Terminal 3: Run the TUI in Agent Mode

```bash
cargo run -p strictly_games_tui agent
```

### Terminal 4 (Optional): Watch Logs

```bash
tail -f strictly_games_tui.log
```

### How to Play

1. **TUI shows it's your turn (Human / X)**: Press 1-9 to make a move
2. **TUI shows "AI is thinking..." (Agent / O's turn)**: 
   - Switch to Terminal 2 (copilot)
   - Ask copilot: "Call the get_board tool to see the current state"
   - Then: "Call make_move with position 4" (or whatever position)
3. **TUI updates** when the agent's move comes through!

### Example Agent Commands

In the copilot terminal:

```
> Call get_board to show me the current game state
[copilot calls tool, shows board]

> Call make_move with position 4
[move is sent to TUI via channel]
```

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
- ✅ Agent makes moves via MCP tools
- ✅ Full game loop with agent participation
- ✅ Clean logging (no interference with TUI)
- ✅ Mode selection via command line

## Troubleshooting

**TUI not responding to agent moves?**
- Check `strictly_games_tui.log` for errors
- Verify copilot is connected (shows tools in copilot)
- Make sure you're calling `make_move` with position 0-8 (not 1-9!)

**Copilot can't find tools?**
- Verify MCP config path is correct
- Check that strictly_games binary exists at configured path
- Try building with `cargo build` first

**TUI display corrupted?**
- All logging now goes to file, not terminal
- If still seeing issues, check for stray print statements

## Next: Full Automation

Future work:
- Add agent prompting/guidance so it calls tools automatically
- Build agent wrapper that auto-calls tools based on game state
- Use agent with function-calling that can autonomously use tools

But the hard part is done - the architecture works!
