# Strictly Games TUI

Terminal UI for playing games with AI agents.

## Features

### Phase 1 (Complete)
- ✅ Human vs Human tic-tac-toe
- ✅ Beautiful ratatui rendering
- ✅ Keyboard controls

### Phase 2 (In Progress)
- ✅ Player trait abstraction
- ✅ Human player implementation
- ✅ Simple AI opponent
- 🔄 Agent player via MCP (coming soon)
- 🔄 "Agent thinking..." animation

## Running

```bash
# Human vs Human (original mode)
cargo run -p strictly_games_tui

# Coming soon: Human vs AI Agent
# cargo run -p strictly_games_tui -- --mode agent
```

## Architecture

```
TUI (ratatui)
    ↓
Orchestrator
    ↓
Player Trait
    ├─ HumanPlayer (keyboard input)
    ├─ SimpleAI (basic AI, no MCP)
    └─ AgentPlayer (MCP client → copilot CLI)
```

## Phase 2 Progress

Current state: We have all the pieces for agent integration:
- Player trait with async get_move()
- Orchestrator for game loop coordination
- Event channels for UI updates
- SimpleAI for testing orchestration

Next: Wire up the orchestrator to the TUI and add agent spawning.
