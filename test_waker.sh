#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

echo "=== Testing Waker Notification System ==="
echo ""

# Create game and make first move
echo "1. Creating game session..."
curl -s -X POST http://localhost:3000 \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_game","arguments":{"session_id":"waker_test","player_x_name":"Human","player_o_name":"Claude"}}}' \
  | sed 's/^data: //' | jq -r '.result.content[0].text'

echo ""
echo "2. Human makes move (position 5)..."
curl -s -X POST http://localhost:3000 \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"make_move","arguments":{"session_id":"waker_test","player_id":"waker_test_human","position":5}}}' \
  | sed 's/^data: //' | jq -r '.result.content[0].text'

echo ""
echo "3. Starting bridge with waker (watching for 5 seconds)..."
echo ""

# Start bridge and capture both stdout and stderr
(echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | \
 RUST_LOG=debug GAME_SESSION_ID=waker_test AGENT_NAME=Claude \
 timeout 5 ./target/debug/stdio_http_bridge) 2>&1 | tee /tmp/waker_output.log

echo ""
echo "=== Waker Debug Output ==="
grep -E "(Waker|Turn check|our_mark|notification|Queued)" /tmp/waker_output.log || echo "❌ No waker debug messages found!"

echo ""
echo "=== Looking for JSON-RPC notifications ==="
grep '"method":"notifications/message"' /tmp/waker_output.log || echo "❌ No notifications sent!"
