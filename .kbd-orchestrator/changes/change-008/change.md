# change-008: README.md updates
**Status:** [ ] pending  
**Agent:** general-purpose  
**File:** `README.md`  
**Depends on:** change-007

## Goal
Add MCP and agentskills.io sections to README. All existing sections remain intact. New content slots in cleanly after the existing opencode integration section.

## Tasks
- [ ] Add "Level 4 — MCP Server" subsection under "opencode integration"
- [ ] Include Claude Code `mcpServers` JSON config block for `--mcp-stdio`
- [ ] Include Streamable HTTP usage block for `--mcp-http-port`
- [ ] Add MCP tool reference table (chat_completion, list_models, check_auth, set_model)
- [ ] Add "agentskills.io Skill" section after MCP section
- [ ] Add `MCP_HTTP_PORT` row to env vars table
- [ ] Add `CODEX_DEFAULT_MODEL` row to env vars table
- [ ] Add `--mcp-stdio`, `--mcp-http-port` rows to CLI flags table

## Acceptance
- README renders cleanly with no broken links or tables
- All new code blocks are accurate and tested
