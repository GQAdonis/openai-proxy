## Why

openai-proxy has no runtime extensibility — no event callbacks, no way to observe or react to what the proxy is doing mid-stream. This makes it impossible to integrate with observability pipelines, feed events to AG-UI-compatible frontends, trigger parallel skill execution, or audit tool calls without forking the binary. A webhook-based hooks system solves all of these without embedding any protocol in the core binary.

## What Changes

- Add `src/hooks.rs` with a `ProxyHooks` trait (async, object-safe) and a no-op `NullHooks` default impl.
- Add `WebhookHooks` impl that reads `hooks.toml` at startup and POSTs AG-UI-compatible JSON to configured URLs on each event type.
- Add `hooks` field (`Arc<dyn ProxyHooks>`) to `AppState` in `src/lib.rs`.
- Call hooks at 7 points in `src/proxy.rs`: request received, text delta, tool call start, tool call args delta, tool result submitted, response complete, error.
- Add `--hooks-config <path>` CLI flag and `PROXY_HOOKS_CONFIG` env var to `src/main.rs`.
- Ship `hooks.example.toml` at project root showing the full format.

## Capabilities

### New Capabilities
- `proxy-hooks-trait`: Extensible async event interface for proxy lifecycle events.
- `webhook-hooks`: Config-file-driven HTTP webhook delivery for all hook events.
- `ag-ui-compatible-payloads`: Event payloads follow AG-UI semantics so upstream consumers can treat this as an AG-UI event source without the proxy being a native AG-UI server.

### Modified Capabilities
- `proxy-request-pipeline`: 7 hook callsites added (all non-blocking, errors logged but not propagated to client).

## Impact

- Affected files: `src/hooks.rs` (new), `src/lib.rs`, `src/proxy.rs`, `src/main.rs`, `hooks.example.toml` (new).
- New dependency: `toml` crate for config parsing (already in Cargo.toml via other deps — verify).
- Backward compatible: hooks are opt-in; no config file = NullHooks = zero behavior change.
- No breaking changes to existing API, MCP, or CLI interfaces.
