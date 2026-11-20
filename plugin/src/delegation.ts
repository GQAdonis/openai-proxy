import type { Plugin } from "@opencode/plugin"

type Call = { parentSessionId: string; parentMessageId: string; delegationId: string; prompt: string }

export function registerDelegation(ctx: Plugin.Context, invoke: (call: Call, signal: AbortSignal) => Promise<string>) {
  return ctx.tool.transform(editor => {
    editor.add({
      name: "delegate_codex",
      description: "Request one restricted Codex subscription turn and return its text.",
      input: { type: "object", properties: { prompt: { type: "string" } }, required: ["prompt"], additionalProperties: false },
      execute: async (input, context) => {
        if (typeof input !== "object" || input === null || !("prompt" in input) ||
            typeof input.prompt !== "string" || !input.prompt || input.prompt.length > 32_768) {
          throw new Error("A bounded prompt is required")
        }
        await ctx.permission.assert({
          sessionID: context.sessionID, agent: context.agent, action: "delegate_codex",
          resources: ["bound-workspace"], save: [],
          source: { type: "tool", messageID: context.messageID, id: context.id },
        })
        const content = await invoke({ parentSessionId: context.sessionID, parentMessageId: context.messageID,
          delegationId: context.id, prompt: input.prompt }, context.signal)
        return { content }
      },
    })
  })
}
