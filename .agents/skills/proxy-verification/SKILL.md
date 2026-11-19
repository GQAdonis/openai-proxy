---
name: proxy-verification
description: Review credential isolation, streaming compatibility, accessibility and single-binary releases with observable acceptance evidence.
---

# Service verification

Take acceptance scenarios from the active change. Work from synthetic accounts, a temporary Codex home, a mocked App Server and a fake upstream. Run the ordinary plugin, CLI, router, browser or packaged binary for the behavior under review. Record versions, command, expected result, observation and any untested boundary.

Prioritize concurrent expiry, logout during refresh, another process completing login, a non-admin calling refresh directly, chat isolation in shared mode, partial SSE/tool calls, offline assets and release execution without frontend sources. For UI, include keyboard flow, screen-reader status behavior and redaction. For integration releases verify V2 plugin loading in the actual OpenCode version. Prefer a separate reviewer context for independent findings. Supply the smallest reproducible regression and the owning role. Tests that reach real provider accounts are separately identified from fixture-based verification.
