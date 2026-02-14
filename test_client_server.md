# Client-Server Integration Test

## 1. What commands can the client issue?

**MCP Commands (via `/message` endpoint):**
1. `initialize` - Establish MCP session
2. `notifications/initialized` - Confirm initialization  
3. `tools/call` with `make_move` - Place piece on board
4. `tools/call` with `start_game` - Initialize new game

**REST Commands:**
1. `GET /health` - Server health check
2. `GET /api/sessions/:id/game` - Get game state as AnyGame JSON

## 2. How should the server respond?

**MCP initialize:**
- Response: JSON-RPC result with capabilities
- Headers: `mcp-session-id: <uuid>` 
- Status: 200

**MCP initialized notification:**
- Response: Empty or acknowledgment
- Status: 202

**MCP make_move tool:**
- Response: JSON-RPC result with board state
- Updates session state
- Status: 200

**REST /api/sessions/:id/game:**
- Response: AnyGame JSON `{"InProgress": {...}}` or `{"Won": {...}}` etc
- Status: 200

## 3. Does it?

**TEST NEEDED:**
Run server, capture logs, send each command, verify responses.

The TUI issue: Moves are sent but don't update the board. This means either:
- Server receives but doesn't process
- Server processes but TUI doesn't see update
- Wrong session ID being used

**Next step:** Run manual curl tests to isolate the problem.
