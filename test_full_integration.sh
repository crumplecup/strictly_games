#!/usr/bin/env bash
# Full integration test: HTTP server + MCP agent + TUI
set -e

echo "=== Strictly Games Full Integration Test ==="
echo ""

# Start HTTP server
echo "1. Starting HTTP server on port 3000..."
cargo run -- http &
SERVER_PID=$!
echo "   Server PID: $SERVER_PID"
sleep 3

# Start MCP agent
echo ""
echo "2. Starting MCP agent (connects to HTTP server)..."
RUST_LOG=info cargo run -- agent --server-url http://localhost:3000 --test-play &
AGENT_PID=$!
echo "   Agent PID: $AGENT_PID"
sleep 5

# Cleanup
echo ""
echo "Cleaning up..."
kill $AGENT_PID 2>/dev/null || true
kill $SERVER_PID 2>/dev/null || true

echo ""
echo "=== Test Complete ==="
echo "Check logs above to verify:"
echo "  ✓ Server started and accepted connections"
echo "  ✓ Agent connected via HTTP"
echo "  ✓ play_game tool was called"
echo "  ✓ Sampling request triggered"
echo "  ✓ Claude API called"
echo "  ✓ Move returned and applied"
