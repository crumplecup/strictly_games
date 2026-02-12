# Manual Testing Instructions

## Test HTTP Server with Copilot CLI

The HTTP server is ready for testing. Here's how to verify it works:

### Step 1: Start the Server

In terminal 1:
```bash
cd /home/erik/repos/strictly_games
RUST_LOG=info cargo run --bin server_http
```

You should see:
```
Server ready at http://localhost:3000/
Agents can connect and call make_move, get_board, start_game tools
```

### Step 2: Verify MCP Config

Check `~/.copilot/mcp-config.json` has:
```json
{
  "mcpServers": {
    "strictly-games": {
      "type": "sse",
      "url": "http://localhost:3000",
      "tools": ["*"]
    }
  }
}
```

**Important:** Use `"type": "sse"` (Server-Sent Events), not `"http"`. The `tools` field is required.

### Step 3: Start Copilot CLI

In terminal 2:
```bash
copilot --additional-mcp-config @/home/erik/.copilot/mcp-config.json
```

### Step 4: Test Tool Discovery

Ask the agent:
```
What tools are available from the strictly-games server?
```

Expected: Agent should list `start_game`, `get_board`, `make_move` tools

### Step 5: Play a Game

```
Let's play tic-tac-toe! Start a new game and make the first move in the center.
```

Expected: Agent calls `start_game()` then `make_move(4)`

## What to Look For

**Server Terminal:**
- Session creation logs
- Tool call logs
- Move notifications

**Copilot Terminal:**
- Tool discovery works
- Tools can be called
- Responses come back

## Known Status

✅ Server compiles and runs
✅ HTTP endpoint responding
✅ Sessions being created
⏸️ Awaiting manual test with copilot CLI

## Next After This Works

1. Update TUI to connect as MCP client to HTTP server
2. Implement TUI → HTTP → Agent flow
3. Test human-vs-agent gameplay
4. Test agent-vs-agent gameplay
