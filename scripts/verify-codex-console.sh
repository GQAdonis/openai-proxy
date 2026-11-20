#!/usr/bin/env bash
set -euo pipefail
umask 077
root="$(cd "$(dirname "$0")/.." && pwd -P)"
exec python3 -B "$root/tests/console_boundary/runner.py" "$@"
