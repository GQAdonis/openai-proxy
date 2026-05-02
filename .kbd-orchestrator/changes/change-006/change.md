# change-006: Plugin shell.env hook extension
**Status:** [ ] pending  
**Agent:** typescript-reviewer  
**File:** `plugin/src/index.ts`  
**Depends on:** change-005

## Goal
Extend the `shell.env` hook to inject `CODEX_PROXY_URL` and `CODEX_DEFAULT_MODEL` into every shell the agent spawns, enabling subprocesses to locate the proxy and use the preferred model without additional configuration.

## Tasks
- [ ] In `shell.env` hook return, add `CODEX_PROXY_URL: process.env.CODEX_PROXY_URL ?? PROXY_BASE_URL`
- [ ] Add `CODEX_DEFAULT_MODEL`: inject only if `process.env.CODEX_DEFAULT_MODEL` is set (avoid empty string override)
- [ ] Verify import of `PROXY_BASE_URL` from `./config.js` is present

## Acceptance
- Shells spawned by opencode have `CODEX_PROXY_URL` set to the configured proxy base URL
- `CODEX_DEFAULT_MODEL` is forwarded when set in the parent environment
