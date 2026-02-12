# Copilot CLI HTTP Transport Bug

## Issue Summary

Copilot CLI v0.0.407 does not correctly implement the MCP HTTP/SSE transport specification,
preventing it from connecting to rmcp HTTP servers.

## Evidence

### 1. MCP Specification Requirements

From [MCP Transports Specification](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports):

> The client MUST include an Accept header, listing both application/json and text/event-stream as supported content types.

**Required header:** `Accept: application/json, text/event-stream`

### 2. What Copilot Actually Sends

Server logs show copilot sends a GET request (not POST):

```
2026-02-12T02:27:39.925273Z  INFO server_http: Incoming HTTP request 
  method=GET 
  uri=/ 
  headers={
    "user-agent": "copilot/0.0.407 (linux v24.11.1) term/WezTerm", 
    "accept": "text/event-stream",  # ❌ Missing application/json
    ...
  }
```

**Problems:**
- Uses GET instead of POST for initialization
- Only includes `text/event-stream` in Accept header
- Missing `application/json` mime type

### 3. What Should Happen

Correct MCP HTTP/SSE transport flow:

1. **Initialize (POST):**
   ```
   POST / HTTP/1.1
   Content-Type: application/json
   Accept: application/json, text/event-stream
   
   {"jsonrpc": "2.0", "method": "initialize", ...}
   ```

2. **Server Response (SSE):**
   ```
   HTTP/1.1 200 OK
   Content-Type: text/event-stream
   
   data: {"jsonrpc": "2.0", "id": 1, "result": {...}}
   ```

3. **Subsequent requests (POST):**
   - All tool calls via POST with dual Accept header
   - Server can choose JSON or SSE response format

### 4. Actual Behavior

```
GET / HTTP/1.1            # ❌ Should be POST
Accept: text/event-stream # ❌ Missing application/json

Response: 405 Method Not Allowed
```

Result: Copilot fails to connect, silently falls back, doesn't show MCP tools.

## Verification

### Manual Test (Works)

```bash
echo '{"jsonrpc": "2.0", "id": 1, "method": "initialize", ...}' \
  | curl -X POST http://localhost:3000 \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    --data-binary @-
```

**Result:** ✅ Server responds correctly

```
data: {"jsonrpc":"2.0","id":1,"result":{...}}
```

### Copilot Test (Fails)

```bash
copilot --additional-mcp-config @/home/erik/.copilot/mcp-config.json
```

**Result:** ❌ No tools from strictly-games MCP server appear

## Configuration Used

`~/.copilot/mcp-config.json`:
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

## Impact

**Blocked Use Case:** TUI + Agent simultaneous gameplay

- TUI needs stdio for display (ratatui)
- Agent needs MCP transport for tool calls
- HTTP was chosen to allow both simultaneously
- Copilot's incorrect HTTP implementation prevents connection

## Workarounds

1. **Use stdio** (works, but blocks TUI):
   ```json
   {
     "type": "stdio",
     "command": "/path/to/strictly_games",
     "tools": ["*"]
   }
   ```

2. **File copilot CLI bug:** Report to github/copilot-cli

3. **Create HTTP proxy:** Transform copilot's requests to match spec
   - Accept GET → convert to POST
   - Add missing Accept header values
   - Requires custom middleware

4. **Wait for fix:** Hope copilot CLI team fixes HTTP/SSE implementation

## Related Issues

- [copilot-cli#1360](https://github.com/github/copilot-cli/issues/1360) - Streamable HTTP session errors
- MCP Spec: [HTTP/SSE Transport](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports)

## Conclusion

This is a **copilot CLI bug**, not an rmcp or strictly_games issue. The rmcp server correctly implements
the MCP specification. Copilot CLI's HTTP/SSE client needs to be updated to follow the spec.

---

**Status:** Blocked on copilot CLI fix  
**Date:** 2026-02-12  
**Version:** Copilot CLI v0.0.407, rmcp v0.15.0
