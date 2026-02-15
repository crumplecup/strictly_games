# Next Debugging Step

## What We Know
1. TUI starts successfully
2. Cursor moves (arrow keys work)
3. "Making move" log appears when pressing Enter
4. Board never updates

## What We Need To Find Out
Run the TUI with RUST_LOG=trace and press Enter a few times. The logs will show:
1. What JSON is being sent to the server
2. What the server responds with
3. Whether the move is actually processed

## Command
```bash
RUST_LOG=trace cargo run tui 2>&1 | tee full_trace.log
# Press Enter a few times on different squares
# Then grep for "Serialized position", "Sending MCP", "Got MCP response"
```

## Expected Findings
If the server responds with an error, we'll see it in the response body.
Most likely: "Session not found" or "Not your turn" or similar.

## Root Cause Hypothesis
The TUI creates `session_id = "tui_session"` but the agent might be registering
with a different session ID, or the session isn't being created properly.
