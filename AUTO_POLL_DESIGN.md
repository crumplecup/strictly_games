# Auto-Polling Design

## Problem
MCP notifications are server-to-client during request processing only.
Cannot send unsolicited notifications from background tasks.

## Solutions Considered

### 1. ❌ Background Waker (Current - Doesn't Work)
- Waker task tries to send notifications outside request context
- MCP doesn't support unsolicited notifications
- Copilot never sees them

### 2. ✅ Agent Auto-Poll Loop
- Agent explicitly polls get_board every N seconds
- Detects when it's their turn
- Makes move automatically
- **Requires agent to be told once: "poll and play until game ends"**

### 3. ✅ Tool Response Injection
- When human makes move via TUI, inject response in get_board result
- "Human moved to position 5. It's your turn now!"
- Requires TUI to call a "notify_agent" endpoint after moves
- Bridge intercepts get_board and adds context

### 4. ❌ SSE Server Events (Not Supported)
- Would require HTTP/SSE transport (broken in Copilot CLI)
- Only stdio works

## Recommended: Hybrid Approach

1. **Agent starts with instruction**: "Poll get_board every 2 seconds and make moves when it's your turn"
2. **Tool adds context**: get_board response includes hint when recently updated
3. **Agent stops polling**: When game ends (Won/Draw/Timeout)

This is how multiplayer games actually work - clients poll for state updates.
