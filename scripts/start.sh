#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$REPO_DIR/target/release/openai-proxy"

cd "$REPO_DIR"

if [[ ! -f "$BINARY" ]]; then
  echo "Binary not found — building release binary..."
  cargo build --release
fi

exec "$BINARY" "$@"
