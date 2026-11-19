# Using the proxy maintenance team

Open the repository as the workspace and select the `proxy-team` skill. Start with the appropriate role from `team.json`. The team keeps the model and permissions already configured for the active harness. Routine work usually needs one owner. Request a second context for independent review when it matters.

| Harness | How to use the team |
|---|---|
| Codex | Project agents live in `.codex/agents/`. Native names use underscores, for example `codex_identity`. |
| Claude Code | Project subagents live in `.claude/agents/`. Select a role by its hyphenated ID. |
| OpenCode 2.x | Project subagents live in `.opencode/agents/`. List them with `opencode debug agents --standalone`. |
| Kimi Code | Project profiles live in `.kimi-code/agents/`. Select one with `kimi --agent codex-identity`. |
| MiniMax Code | In an ordinary session use the project skill in `.minimax/skills/`. Native roles live under `.minimax/agents/`; launching from the repository root with `MINIMAX_DATA_DIR="$PWD/.minimax" mcode` selects this project's data directory, which has its own settings and sign-in state. |
| GitHub Copilot | Project agents live in `.github/agents/` using the `.agent.md` format. |
| DeepSeek Harness | `.dsh/profiles/` contains six role personas and an optional team composition. The host selects and activates profiles. |
| Zed | Zed's effective project instructions point to the team. Use `proxy-team` or ask the active agent to read the role from `team.json`. In an external ACP session, delegation depends on the external agent. |
| Other OpenSpec tools | Open the project and load the `proxy-team` skill from the path registered for the tool. Its routing table points to all six project playbooks. |

## Discovery and maintenance

`openspec-tools.json` records the project paths for every available tool in OpenSpec 2.1.0. Nine IDs use OpenSpec's generic fallback. If a client using that fallback does not auto-discover `.agents/skills`, open `.agents/skills/proxy-team/SKILL.md` explicitly. Windsurf is the legacy ID for Devin. Use the host's usual trust and skill-reload mechanisms when required. The entry in each additional directory is a relative link to one project-owned skill. Checkouts need symlink support for automatic discovery through those links. [OpenSpec supported tools](https://github.com/Fission-AI/OpenSpec/blob/v2.1.0/docs-lab/reference/supported-tools.md)

The six project skills are readable directly under `.agents/skills/`. The role manifest also names supporting Prometheus skills available on the configured development machine. Install the pack and the official shadcn skill on a new machine for their full workflows. The core role playbooks and sources travel with this repository. The UI owner uses project-installed Impeccable, UI UX Pro Max for focused interaction research, and shadcn for component lookup. To provision the native Impeccable engine on another platform run `impeccable install --project --providers=codex --no-hooks --yes`. [shadcn skills](https://ui.shadcn.com/docs/skills)

Run `node scripts/check-agent-team.mjs` after changing the team, moving skill directories, or checking out on a new machine. To compare the matrix with the installed OpenSpec release, pass the path to its `dist/core/config.js` as the sole argument. On a pack upgrade, regenerate the map from its public tool IDs and resolved installer project paths, update the relative links, and rerun the check.

Update `team.json` first when responsibilities change. Use the installed agent-team-creator CLI's `validate` and `export` commands for each native target. Review diffs and preserve any project-specific permissions when applying exports. OpenCode 2.3 uses a plural `permissions` array for the reviewer. The creator's portable routing record names the primary Codex install. The other checked-in native formats use the same six role IDs. [OpenCode 2.3 agent schema](https://github.com/anomalyco/opencode/blob/v2.3.0/services/site/src/docs/content/agents.mdx)

## Cross-role work

For a login or refresh change, the identity owner defines snapshot and error semantics. The runtime owner applies them to every transport. The plugin owner connects successful login to reconciliation. For the embedded console, runtime and UI owners agree on redacted admin and stream contracts, while the release owner delivers the asset build and mount. The reviewer checks the assembled candidate from the normal client or packaged executable using fixture credentials.
