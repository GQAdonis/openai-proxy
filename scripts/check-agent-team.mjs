import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const local = (...parts) => path.join(root, ...parts);
const read = (file) => readFile(local(file), "utf8");
const routing = JSON.parse(await read(".agent-team/project-routing.json"));
const team = JSON.parse(await read(routing.manifest));
const directory = path.dirname(routing.manifest);
const matrix = JSON.parse(await read(path.join(directory, "openspec-tools.json")));
const canonical = await realpath(local(".agents/skills/proxy-team/SKILL.md"));
const lookup = new Map();

for (const [dir, ids] of Object.entries(matrix.skillDirectories)) {
  assert.equal(await realpath(local(dir, "proxy-team/SKILL.md")), canonical, dir);
  for (const id of ids) {
    assert(!lookup.has(id), `Duplicate tool ID ${id}`);
    lookup.set(id, dir);
  }
}
for (const id of Object.values(matrix.aliases)) assert(lookup.has(id), id);
for (const id of matrix.universalFallback) assert.equal(lookup.get(id), ".agents/skills", id);
if (process.argv[2]) {
  const registry = await import(pathToFileURL(path.resolve(process.argv[2])).href);
  const tools = registry.AI_TOOLS.filter((tool) => tool.available);
  assert.equal(lookup.size, tools.length, "OpenSpec tool count changed");
  assert.deepEqual(matrix.aliases, registry.TOOL_ID_ALIASES);
  const fallback = tools.filter(({value}) => value !== "agents" && registry.installerAgent(value).id === "universal");
  assert.deepEqual([...matrix.universalFallback].sort(), fallback.map(({value}) => value).sort());
  for (const tool of tools) assert.equal(lookup.get(tool.value), registry.installerAgent(tool.value).projectSkillsDir, tool.value);
}

const entry = await read(".agents/skills/proxy-team/SKILL.md");
for (const role of team.roles) {
  const id = role.id;
  const skill = role.skills[0];
  const skillPath = `.agents/skills/${skill}/SKILL.md`;
  assert(entry.includes(id) && entry.includes(skill), `Missing route for ${id}`);
  assert((await read(skillPath)).includes(`name: ${skill}\n`), skillPath);
  const nativeFiles = [
    `.codex/agents/${id.replaceAll("-", "_")}.toml`,
    `.claude/agents/${id}.md`, `.opencode/agents/${id}.md`,
    `.kimi-code/agents/${id}.md`, `.minimax/agents/${id}/agent.md`,
    `.github/agents/${id}.agent.md`,
    `.dsh/profiles/${team.id}-${id}/cordis.patch.yml`,
  ];
  for (const file of nativeFiles) {
    const content = await read(file);
    assert(content.includes(skillPath) && content.includes(role.prompt), `Stale role prompt in ${file}`);
  }
}
const reviewFile = await read(".opencode/agents/service-reviewer.md");
const review = JSON.parse(reviewFile.split("\n---\n")[0].slice(4));
for (const action of ["edit", "shell"]) {
  assert(review.permissions.some((rule) => rule.action === action && rule.resource === "*" && rule.effect === "deny"), action);
}
console.log(`${lookup.size} OpenSpec tool IDs, ${Object.keys(matrix.skillDirectories).length} skill directories, ${team.roles.length} roles and seven native formats verified.`);
