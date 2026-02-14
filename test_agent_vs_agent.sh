#!/bin/bash
# Test agent vs agent tic-tac-toe game
set -e

SESSION_ID="test_game_$(date +%s)"
SERVER_URL="http://localhost:3000"

echo "🎮 Starting Agent vs Agent Test"
echo "================================"
echo "Session ID: $SESSION_ID"
echo "Server: $SERVER_URL"
echo ""


echo "✅ Server is running"
echo ""

# Start both agents in background
echo "🤖 Starting Agent X (Player X)..."
RUST_LOG=info cargo run -- agent \
    --config agent_x_config.toml \
    --server-url "$SERVER_URL" \
    --test-play \
    --test-session "$SESSION_ID" \
    > /tmp/agent_x.log 2>&1 &
AGENT_X_PID=$!

echo "🤖 Starting Agent O (Player O)..."
RUST_LOG=info cargo run -- agent \
    --config agent_o_config.toml \
    --server-url "$SERVER_URL" \
    --test-play \
    --test-session "$SESSION_ID" \
    > /tmp/agent_o.log 2>&1 &
AGENT_O_PID=$!

echo ""
echo "📝 Agents started:"
echo "   Agent X PID: $AGENT_X_PID (log: /tmp/agent_x.log)"
echo "   Agent O PID: $AGENT_O_PID (log: /tmp/agent_o.log)"
echo ""
echo "⏳ Waiting for game to complete..."
echo "   (Press Ctrl+C to stop)"
echo ""

# Wait for both agents
wait $AGENT_X_PID 2>/dev/null
AGENT_X_EXIT=$?

wait $AGENT_O_PID 2>/dev/null
AGENT_O_EXIT=$?

echo ""
echo "🏁 Game Complete!"
echo "================================"
echo ""

# Show results
echo "📊 Agent X Results:"
echo "-------------------"
tail -20 /tmp/agent_x.log | grep -E "(Game Over|Winner|Draw|ERROR)" || echo "See /tmp/agent_x.log for details"

echo ""
echo "📊 Agent O Results:"
echo "-------------------"
tail -20 /tmp/agent_o.log | grep -E "(Game Over|Winner|Draw|ERROR)" || echo "See /tmp/agent_o.log for details"

echo ""
echo "Exit codes: Agent X=$AGENT_X_EXIT, Agent O=$AGENT_O_EXIT"
echo ""
echo "Full logs:"
echo "  - /tmp/agent_x.log"
echo "  - /tmp/agent_o.log"
