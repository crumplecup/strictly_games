# Strictly Games TUI

Terminal UI for playing games with AI agents.

## Running

```bash
# Human vs SimpleAI (default)
cargo run -p strictly_games_tui
# or explicitly:
cargo run -p strictly_games_tui ai

# Human vs Agent (requires MCP server running)
cargo run -p strictly_games_tui agent
```

## Logging

All logs write to `strictly_games_tui.log` to avoid interfering with the display.

```bash
# Watch logs in another terminal
tail -f strictly_games_tui.log
```

## Features

### Phase 1 (Complete)
- ✅ Human vs Human tic-tac-toe
- ✅ Beautiful ratatui rendering with colored board
- ✅ Keyboard controls (1-9 for moves, R to restart, Q to quit)

### Phase 2 (Complete)
- ✅ Player trait abstraction with async coordination
- ✅ HumanPlayer implementation (keyboard input via channels)
- ✅ SimpleAI opponent (picks first available square)
- ✅ AgentPlayer via MCP channel integration
- ✅ Orchestrator for turn-based gameplay
- ✅ Event-driven UI updates
- ✅ Game mode selection (ai/agent)
- ✅ File-based logging (no TUI interference)

## Agent Mode

When running in agent mode, the TUI waits for an MCP agent to call `make_move` tools.

See [AGENT_TESTING.md](../AGENT_TESTING.md) for detailed setup instructions.

Quick setup:
1. Terminal 1: `cargo run --bin strictly_games` (MCP server)
2. Terminal 2: `copilot --additional-mcp-config @~/.copilot/mcp-config.json`
3. Terminal 3: `cargo run -p strictly_games_tui agent`
4. Terminal 4: `tail -f strictly_games_tui.log` (optional)

Play by having the agent call `make_move` when it's their turn!

## Architecture

```
Terminal UI (ratatui)
    ↓ keyboard events
Main Event Loop  
    ├─→ Keys → HumanPlayer (channel)
    └─→ Events ← Orchestrator
                    ↓
                Player Trait (async)
                ├─ HumanPlayer ✅
                ├─ SimpleAI ✅  
                └─ AgentPlayer ✅ (via MCP channel)
                        ↑
                    MCP Server (strictly_games)
                        ↑
                    Agent (copilot CLI)
```

## Phase 2 Achievement

This proves the elicitation concept for interactive agent gameplay:
- Type-safe game state
- Async orchestration between players
- MCP tool integration via channels
- Real-time UI updates
- Agent participation through standard MCP tools

The architecture enables natural language game interfaces, agent game masters, and multi-agent gameplay with type-safe operational semantics for LLMs.
