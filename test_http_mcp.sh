#!/bin/bash
# Test HTTP MCP server functionality

SERVER_URL="http://localhost:3000"

echo "=== Testing Strictly Games MCP HTTP Server ==="
echo ""

# Test 1: Server responds
echo "1. Testing server health..."
curl -s -o /dev/null -w "HTTP Status: %{http_code}\n" $SERVER_URL
echo ""

# Test 2: Initialize (with proper headers)
echo "2. Testing initialize..."
curl -s "$SERVER_URL" \
  -H "Accept: application/json, text/event-stream" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | jq -r '.result.serverInfo.instructions // "ERROR"'
echo ""

# Test 3: List tools
echo "3. Listing available tools..."
curl -s "$SERVER_URL" \
  -H "Accept: application/json, text/event-stream" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | jq -r '.result.tools[].name // "ERROR"'
echo ""

echo "=== Test Complete ==="
echo "If you see tool names above, the server is working!"
