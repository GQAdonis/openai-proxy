# Assessment: MCP Server + agentskills.io Skill Packaging
**Phase:** mcp-skill-packaging  
**Date:** 2026-05-01  
**Status:** PROCEED — High Value

---

## Verdict: YES — Strong Case for Both

Adding MCP server support and agentskills.io skill packaging to `openai-proxy` is a high-value move with low implementation risk. The research is unambiguous on both fronts.

---

## Finding 1: MCP Server Support

### Why it matters

MCP (Model Context Protocol) has become the dominant agentic integration standard:
- 10,000+ public servers as of early 2026
- Linux Foundation endorsed (neutral governance, long-term stability)
- Native support in: Claude Code, Codex CLI, Gemini CLI, Cursor, VS Code Copilot, Zed
- Streamable HTTP is the recommended transport for 2026 (SSE-based transport deprecated in the spec)

Currently, `openai-proxy` is only accessible as an HTTP proxy — clients must be reconfigured to point at `localhost:8080`. An MCP server wrapping the same proxy logic would let any MCP-capable tool call Codex completions directly as a **tool call**, without any base URL reconfiguration.

### What the MCP server would expose

| MCP Tool | Description |
|---|---|
| `chat_completion` | Call the proxy: accepts model, messages[], stream flag — returns completion text |
| `list_models` | Returns available models (gpt-5.3-codex, codex-mini, etc.) |
| `check_auth` | Verifies `~/.codex/auth.json` is present and valid |
| `set_model` | Hint to set preferred model for the session |

The MCP server is a thin adapter over the existing proxy logic — no new upstream integration required.

### Transport strategy

Use **dual transport** (best practice for 2026):

1. **stdio transport** — local invocation via `openai-proxy --mcp-stdio`. Works with Claude Code's `mcpServers` config, Codex CLI, Gemini CLI without any networking setup.
2. **Streamable HTTP transport** — `openai-proxy --mcp-http` binds on a separate port (default: 8081). Enables remote MCP clients and multi-user scenarios.

Both transports use the same tool registry; transport selection is a startup flag.

### Implementation scope

- Add `rmcp` or `mcp-sdk` Rust crate (or implement MCP JSON-RPC protocol directly — it's simple)
- New `src/mcp.rs` module: tool definitions, JSON-RPC dispatch, stdio/HTTP transport selection
- Estimated: ~300 lines of new Rust code
- Zero changes to existing proxy logic (`src/proxy.rs`, `src/codex.rs`)

---

## Finding 2: agentskills.io Skill Packaging

### Why it matters

agentskills.io is an open standard for distributing skills to AI coding assistants. A single `SKILL.md` file works across Claude Code, Codex CLI, Gemini CLI, Cursor, VS Code — maximum reach with minimal authoring cost.

Ecosystem growth is rapid (18.5× year-over-year). Claude Code marketplace support means discovery through `/skills add` commands with no manual installation steps for end users.

### Skill design: two skills, one package

#### Skill 1: `openai-proxy/start`
Teaches any agent how to start and configure the proxy. Loaded on activation, activates on: "start openai-proxy", "use codex subscription", "codex proxy", "opencode setup".

```markdown
---
name: openai-proxy/start
description: Start and configure the openai-proxy Codex subscription proxy
license: MIT
compatibility:
  claude-code: ">=1.0"
  codex-cli: ">=2.0"
  gemini-cli: ">=0.1"
---

Start the proxy with: `cargo run --release` or `./openai-proxy`
Configure via `~/.codex/auth.json` (written by `codex login`).
Proxy listens on http://localhost:8080/v1 — point any OpenAI-compatible client here.
```

#### Skill 2: `openai-proxy/use-via-mcp`
Teaches agents to invoke Codex completions as MCP tool calls. Loaded when the MCP server is running.

### Hooks for client context customization

The skill spec supports `allowed-tools` and `metadata` fields that clients can override. For context customization:

1. **`CODEX_PROXY_URL` env var** — already supported in `plugin/src/config.ts`; skill references it so clients can point at remote proxy instances
2. **`CODEX_DEFAULT_MODEL` env var** — skill documents this as a hook for clients to override the default model without editing config
3. **MCP `set_model` tool** — enables agents to dynamically switch models within a session
4. **Plugin `shell.env` hook** — already injects `CODEX_AUTH_PATH`; extend to also inject `CODEX_PROXY_URL` and `CODEX_DEFAULT_MODEL`

This gives clients three layers of customization: environment (coarsest), MCP tool call (session-level), and plugin hook (per-shell).

---

## Gap Analysis vs. Current State

| Capability | Current | Gap |
|---|---|---|
| HTTP proxy (OpenAI Chat Completions → Codex) | ✅ Complete | — |
| opencode plugin (Level 1) | ✅ Complete | — |
| Static opencode.json (Level 2) | ✅ Complete | — |
| Any OpenAI-compatible client (Level 3) | ✅ Complete | — |
| MCP server (stdio transport) | ❌ Missing | ~150 lines |
| MCP server (Streamable HTTP transport) | ❌ Missing | ~150 lines |
| `SKILL.md` packaging | ❌ Missing | ~100 lines |
| Claude Code marketplace entry | ❌ Missing | ~50 lines metadata |
| `CODEX_DEFAULT_MODEL` env hook | ❌ Missing | ~10 lines |
| `scripts/start.sh` helper | ❌ Missing | ~20 lines |

Total new code: ~480 lines. Zero breaking changes to existing functionality.

---

## Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| MCP spec changes | Low (Linux Foundation stable) | Pin to spec version in SKILL.md |
| agentskills.io marketplace changes | Low | SKILL.md is self-contained; marketplace is additive |
| MCP adds latency vs. direct HTTP | Negligible | MCP tool calls are thin JSON-RPC wrappers |
| Dual transport complexity | Low | Same tool registry, different I/O layers |

---

## Implementation Plan (Prioritized)

### Phase 1: MCP stdio transport (highest value, lowest effort)
1. Add MCP JSON-RPC protocol types to `src/mcp.rs`
2. Implement `chat_completion`, `list_models`, `check_auth` tools
3. Stdio transport: read from stdin, write to stdout (per MCP spec)
4. Add `--mcp-stdio` CLI flag to `main.rs`

### Phase 2: Streamable HTTP transport
5. Add HTTP/SSE MCP transport on `--mcp-http` flag (default port 8081)
6. Share tool registry with stdio transport

### Phase 3: agentskills.io packaging
7. Create `SKILL.md` at repo root
8. Create `scripts/start.sh` and `scripts/start-mcp.sh`
9. Add `references/auth.md` (auth setup reference doc)
10. Update `README.md` with MCP and skill sections

### Phase 4: Marketplace + hooks
11. Submit to agentskills.io registry
12. Add `CODEX_DEFAULT_MODEL` env var support
13. Extend plugin `shell.env` hook with new env vars

---

## Recommendation

Proceed with all four phases. Phase 1 alone (MCP stdio) delivers 80% of the value in ~150 lines. The agentskills.io packaging (Phase 3) is ~100 lines and makes this discoverable across every major AI coding assistant with zero additional integration work by end users.

The existing architecture is well-positioned: stateless proxy, clean module separation, and `AppState` that's trivially shareable with an MCP handler.
