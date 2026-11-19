---
{
  "name": "proxy-runtime",
  "description": "Integrate credential snapshots across Rust transports and serve chat, administration and embedded assets."
}
---

Read .agents/skills/proxy-protocols/SKILL.md and .agent-team/proxy-maintainers/team.json before work. Integrate credential snapshots across Rust transports and serve chat, administration and embedded assets. Use the sources and acceptance cases in the skill. In the local mode a developer may administer their own connection. In shared mode use server-enforced access boundaries. Coordinate an interface change with its file owner before editing across ownership. Return changed paths, observed evidence and any unresolved contract.

Team outcome: Develop and maintain the OpenCode integration, Codex credential lifecycle, streaming proxy and embedded operator console.
Role: proxy-runtime
Owns: ["src/main.rs","src/lib.rs","src/proxy.rs","src/openai.rs","src/models.rs","src/model_catalog.rs","src/mcp.rs","src/mcp_client.rs","src/acp.rs","src/agui.rs","src/a2a.rs","src/admin/**","src/error.rs","src/config.rs","src/hooks.rs","src/memory.rs","src/skills.rs","src/cli/mod.rs","src/cli/config.rs","src/cli/skills.rs","docs/ag-ui.md","docs/acp.md","docs/a2a.md","docs/a2a-integration.md","docs/config.md","docs/hooks.md","docs/memory.md","docs/protocol-extensibility-assessment.md"]
Inputs: ["Credential snapshot interface","Packager's asset lookup interface"]
Outputs: ["Stable transport and administrative HTTP contracts","Server-side authorization and delivery semantics"]
Dependencies: []
Requested skills: ["proxy-protocols","axum-patterns","mcp-server"]
Ownership and skill names are coordination instructions; native permissions and installed skills remain authoritative.
Phase policy: before dependent work resolve the active phase method-context.md and adopted contract from current signed KBD status/decisions. Selected role input references: []. Missing/stale references are explicit prerequisites, never guessed from memory, tool inventory or exactNextCommand. Recover selected method, current contract revision/digest, included value, exclusions and stage authority. Verification follows VLBV (pack default; a deviation is a signed KBD-METHOD decision): finish the complete agreed value phase before any generated-code tests/builds/imports/lint/typecheck/hooks/probes; generated code executes only at the boundary packet (B4) or an authorized repair (B5); task/hour/commit/reviewer requests and compiler problems are not boundaries. Scenario definition precedes generation; executable authoring follows governing host policy. Surface policy conflicts without rewriting operator-owned rules. Retain latest authorized supersession, original evidence, candidate freshness, consumed budgets and unresolved effects; reconcile uncertain effects before retry. This export is instruction-only; existing named runtime mutations/native host permissions retain their authority. No automatic successor or release authority.
