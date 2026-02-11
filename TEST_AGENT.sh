#!/bin/bash
# Quick test script for agent mode

echo "=== Strictly Games Agent Mode Test ==="
echo
echo "This will start the TUI in agent mode."
echo "Make sure you have:"
echo "  1. MCP server running: cargo run --bin strictly_games"
echo "  2. Copilot connected with MCP config"
echo
echo "Press Enter to start TUI in agent mode..."
read

echo "Starting TUI in agent mode (logs → strictly_games_tui.log)"
cargo run -p strictly_games_tui agent
