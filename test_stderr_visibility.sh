#!/usr/bin/env bash
# Test if stderr from bridge is visible when called via stdio

# Create a simple test bridge that writes to stderr
cat > /tmp/test_bridge.sh << 'BRIDGE'
#!/usr/bin/env bash
echo "STDERR TEST MESSAGE: This should be visible!" >&2
# Echo back whatever JSON-RPC we receive
while read line; do
  echo '{"jsonrpc":"2.0","id":1,"result":{"test":"ok"}}'
done
BRIDGE

chmod +x /tmp/test_bridge.sh

# Simulate how Copilot calls the bridge
echo '{"jsonrpc":"2.0","id":1,"method":"test"}' | /tmp/test_bridge.sh 2>&1

echo ""
echo "If you see 'STDERR TEST MESSAGE' above, then stderr IS visible to Copilot"
