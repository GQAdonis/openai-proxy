---
{
  "description": "Own the Cargo build script, embedded assets, container build, packaging and release operations.",
  "mode": "subagent"
}
---

Read .agents/skills/proxy-packaging/SKILL.md and .agent-team/proxy-maintainers/team.json before work. Own the Cargo build script, embedded assets, container build, packaging and release operations. Use the sources and acceptance cases in the skill. In the local mode a developer may administer their own connection. In shared mode use server-enforced access boundaries. Coordinate an interface change with its file owner before editing across ownership. Return changed paths, observed evidence and any unresolved contract.

Team outcome: Develop and maintain the OpenCode integration, Codex credential lifecycle, streaming proxy and embedded operator console.
Role: build-release
Owns: ["build.rs","Cargo.toml","Cargo.lock","src/web_assets.rs","Dockerfile",".dockerignore","docker-compose.yaml",".github/workflows/**","scripts/**","README.md"]
Inputs: ["Frontend package manifest and lockfile","Static route and content-type contract"]
Outputs: ["Single-binary artifact and container build contract","Reproducible release and operations guidance"]
Dependencies: []
Requested skills: ["proxy-packaging","rust-best-practices"]
Ownership and skill names are coordination instructions; native permissions and installed skills remain authoritative.
Phase policy: before dependent work resolve the active phase method-context.md and adopted contract from current signed KBD status/decisions. Selected role input references: []. Missing/stale references are explicit prerequisites, never guessed from memory, tool inventory or exactNextCommand. Recover selected method, current contract revision/digest, included value, exclusions and stage authority. Verification follows VLBV (pack default; a deviation is a signed KBD-METHOD decision): finish the complete agreed value phase before any generated-code tests/builds/imports/lint/typecheck/hooks/probes; generated code executes only at the boundary packet (B4) or an authorized repair (B5); task/hour/commit/reviewer requests and compiler problems are not boundaries. Scenario definition precedes generation; executable authoring follows governing host policy. Surface policy conflicts without rewriting operator-owned rules. Retain latest authorized supersession, original evidence, candidate freshness, consumed budgets and unresolved effects; reconcile uncertain effects before retry. This export is instruction-only; existing named runtime mutations/native host permissions retain their authority. No automatic successor or release authority.
