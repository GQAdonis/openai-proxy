# ACP external agent

The proxy implements [Agent Client Protocol](https://agentclientprotocol.com/) over stdio for editors such as Zed and JetBrains. The editor launches a dedicated process under the credential owner's account. The process uses its configured Codex login or an OpenAI API key with a Responses API route.

```sh
openai-proxy serve --acp-stdio
```

Standard input and output carry JSON-RPC 2.0. Diagnostic logs go to standard error. Use a dedicated owner and credential store for this process. [Credential operation](codex-auth.md)

## Configure Zed

Preview the change to the user settings file, then apply it.

```sh
openai-proxy setup acp --zed --global --dry-run
openai-proxy setup acp --zed --global
```

For manual setup, add a custom external agent in Zed and put this entry in `settings.json`. Use the absolute path of the installed executable, then select `openai-proxy` when starting an external agent thread. [Zed external agents](https://zed.dev/docs/ai/external-agents)

```json
{
  "agent_servers": {
    "openai-proxy": {
      "type": "custom",
      "command": "/absolute/path/to/openai-proxy",
      "args": ["serve", "--acp-stdio"]
    }
  }
}
```

## Configure JetBrains AI Assistant

Preview and apply the global registration.

```sh
openai-proxy setup acp --jetbrains --global --dry-run
openai-proxy setup acp --jetbrains --global
```

For manual setup, choose **Add Custom Agent** in AI Chat. Add the executable and arguments to `~/.jetbrains/acp.json`, then select `openai-proxy` in AI Chat. [JetBrains custom agents](https://www.jetbrains.com/help/ai-assistant/acp.html#add-a-custom-agent)

```json
{
  "agent_servers": {
    "openai-proxy": {
      "command": "/absolute/path/to/openai-proxy",
      "args": ["serve", "--acp-stdio"]
    }
  }
}
```

The KnowMe console can preview and apply these entries on the service host or export a descriptor for a different machine. Other ACP editors can use the same executable and arguments through `openai-proxy setup acp --generic`. [Console client setup](console.md#settings-and-client-setup)

## Session behavior

| Method | Behavior |
| --- | --- |
| `initialize` | Negotiates the protocol and returns the agent capabilities. |
| `session/new` | Creates a session bound to the credential owner and an existing workspace. |
| `session/prompt` | Sends text to the configured Responses backend and emits incremental `session/notification` chunks. |

A credential owner change invalidates the prior session. Start a new editor session after changing accounts. The dedicated ACP process streams assistant text and returns the prompt stop reason.
