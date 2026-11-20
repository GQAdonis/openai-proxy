/* Loaded by VS Code's extension test host in a new disposable user-data directory. */
const fs = require("node:fs");
const vscode = require("vscode");

exports.run = async function () {
  const receipt = process.env.BOUNDARY_CODE_RECEIPT;
  const marker = process.env.BOUNDARY_CODE_MARKER;
  const store = (result) => fs.writeFileSync(receipt, JSON.stringify(result), { mode: 0o600, flag: "wx" });
  if (!vscode.lm?.invokeTool) {
    store({ unavailable: "VS Code's installed language-model tool API is unavailable" });
    return;
  }
  const names = ["chat_completion", "list_models", "check_auth", "set_model"];
  const deadline = Date.now() + 12000;
  let identifiers;
  do {
    identifiers = Object.fromEntries(names.map((tool) => {
      const actual = vscode.lm.tools.find((t) => t.name.includes("proxy") && t.name.endsWith(tool));
      return [tool, actual?.name];
    }));
    if (names.every(name => identifiers[name])) break;
    await new Promise(resolve => setTimeout(resolve, 250));
  } while (Date.now() < deadline);
  if (!names.every(name => identifiers[name])) {
    store({ unavailable: "The dedicated editor profile has no active proxy tool catalogue", discovered: Object.keys(identifiers).filter(k => identifiers[k]) });
    return;
  }
  const abort = new vscode.CancellationTokenSource();
  const timer = setTimeout(() => abort.cancel(), 8000);
  try {
    const answer = await vscode.lm.invokeTool(identifiers.set_model, { input: { task: "quick " + marker } }, abort.token);
    const text = answer.content.filter(part => typeof part.value === "string").map(part => part.value).join("");
    store({ version: vscode.version, tools: identifiers, markerMatched: text.includes(marker), advisoryMatched: text.includes("codex-mini") });
  } finally {
    clearTimeout(timer);
    abort.dispose();
  }
};
