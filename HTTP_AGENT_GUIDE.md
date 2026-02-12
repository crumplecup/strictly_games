# HTTP Agent Integration Guide

## Architecture

**HTTP Transport** enables multi-agent connectivity:

```
┌─────────────┐      HTTP       ┌──────────────┐
│   Agent 1   │ ────────────────▶│              │
│ (Copilot)   │                  │  MCP Server  │
└─────────────┘                  │   (HTTP)     │
                                 │              │
┌─────────────┐      HTTP       │ localhost:   │
│   Agent 2   │ ────────────────▶│    3000      │
│ (Another    │                  │              │
│  instance)  │                  └──────────────┘
└─────────────┘

┌─────────────┐      stdio
│     TUI     │      (display only, no conflict!)
└─────────────┘
```

## Quick Start

### 1. Start HTTP Server

```bash
cargo run --bin server_http

# Output:
# Server ready at http://localhost:3000/
# Agents can connect and call make_move, get_board, start_game tools
```

### 2. Configure MCP Client

Update `~/.copilot/mcp-config.json`:

```json
{
  "mcpServers": {
    "strictly-games": {
      "transport": {
        "type": "http",
        "url": "http://localhost:3000"
      }
    }
  }
}
```

### 3. Connect Agent

```bash
copilot --additional-mcp-config @/home/erik/.copilot/mcp-config.json
```

### 4. Play!

```
Human: Let's play tic-tac-toe. Start a new game.

Agent: [calls start_game()]