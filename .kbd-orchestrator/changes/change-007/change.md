# change-007: SKILL.md + scripts/ + references/
**Status:** [ ] pending  
**Agent:** general-purpose  
**Files:** `SKILL.md` (new), `scripts/start.sh` (new), `scripts/start-mcp.sh` (new), `references/auth.md` (new)  
**Depends on:** change-002 (MCP stdio working before documenting it)

## Goal
Package the proxy as an agentskills.io compliant skill. Two sub-skills with progressive disclosure. `scripts/` provides executable entry points. `references/` provides deep-dive docs loaded on demand.

## Tasks
- [ ] Create `SKILL.md` at repo root with valid agentskills.io frontmatter
- [ ] Skill 1 (`openai-proxy/setup`): prerequisites, build steps, auth, env vars, opencode integration
- [ ] Skill 2 (`openai-proxy/mcp`): Claude Code mcpServers config, tool reference table, Streamable HTTP usage
- [ ] `metadata` block with activation keywords for each skill
- [ ] `allowed-tools` block listing safe tools each skill may use
- [ ] Create `scripts/start.sh`: checks for binary, builds if needed, runs proxy
- [ ] Create `scripts/start-mcp.sh`: runs `openai-proxy --mcp-stdio` for Claude Code mcpServers
- [ ] Create `references/auth.md`: full auth.json format, token expiry, refresh flow, troubleshooting
- [ ] `chmod +x scripts/*.sh`

## Skill File Structure
```
SKILL.md               ← progressive metadata + two skill bodies
scripts/
  start.sh             ← build + run proxy
  start-mcp.sh         ← run as MCP stdio server
references/
  auth.md              ← deep-dive auth reference
```

## Acceptance
- `SKILL.md` has valid YAML frontmatter with `name` and `description` fields
- Both scripts execute without error when binary is built
- `references/auth.md` covers all troubleshooting scenarios from README
