#!/usr/bin/env bash
# Run openai-proxy in MCP stdio mode.
# Used as the command in Claude Code's mcpServers config:
#
#   {
#     "mcpServers": {
#       "openai-proxy": {
#         "command": "/path/to/openai-proxy/scripts/start-mcp.sh"
#       }
#     }
#   }
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$REPO_DIR/target/release/openai-proxy"

cd "$REPO_DIR"

if [[ ! -f "$BINARY" ]]; then
  echo "Binary not found — building release binary..." >&2
  cargo build --release >&2
fi

exec "$BINARY" --mcp-stdio
