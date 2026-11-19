---
name: proxy-team
description: Route work in the openai-proxy repository to its six maintenance roles. Use for plugin, auth, transport, chat, packaging or verification work.
---

# Proxy maintenance team

From the repository root, read `.agent-team/proxy-maintainers/team.json`. Choose the role by its owned files and desired output. Each role's first skill lives under `.agents/skills/<skill>/SKILL.md`; open that file directly if the host does not list it. Supplemental pack skills are optional when unavailable. Read the source links and turn the applicable checks into acceptance criteria for the current change.

| Work | Role | First skill |
|---|---|---|
| OpenCode 2.x extensions and migration | opencode-extensions | proxy-opencode |
| Codex SDK, managed refresh and login pickup | codex-identity | proxy-identity |
| HTTP, streaming, MCP and other protocol adapters | proxy-runtime | proxy-protocols |
| Chat and administration screens | chat-experience | proxy-interface |
| Cargo build script, asset embedding and distribution | build-release | proxy-packaging |
| Regression, access controls and acceptance evidence | service-reviewer | proxy-verification |

Use the host's native subagent facility when available and authorized. Otherwise perform the selected role's work in the active conversation. Record a same-context review as such. The implementation owner requests cross-boundary changes from the owner named in the manifest. The team inherits the active harness's model and permissions.

See `.agent-team/proxy-maintainers/TOOLS.md` for native role locations and the OpenSpec discovery matrix.
