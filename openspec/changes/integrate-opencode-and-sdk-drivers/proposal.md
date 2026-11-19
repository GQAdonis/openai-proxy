# OpenCode integration and SDK chat drivers

## Why

The existing OpenCode plugin targets the V1 host and the proxy forwards REST chat directly to a model provider. Users need a V2 plugin that can use their authorized Codex subscription and a persistent REST conversation that can run either agent SDK with predictable tool permissions.

## What Changes

- Add a server plugin for OpenCode 2.3.0, a provider destination dedicated to Codex, and an explicit delegated tool for Codex-owned coding turns.
- Add authenticated, versioned REST conversation and turn resources with stream replay, native cancellation and capability reporting for each SDK.
- Supervise server-side workers, persist native conversation bindings, and expose redacted, attributable failures and run status to the console.

## Capabilities

### New Capabilities

- `sdk-chat-drivers` covers native OpenCode integration, durable conversations, both driver adapters, and ownership of tool execution.

### Modified Capabilities

None.

## Impact

The change affects the TypeScript plugin, Axum REST routing, persistence of conversation metadata, and server-side SDK/CLI packaging. Direct clients retain `/v1/chat/completions`. The identity change supplies the authoritative principal and generation. The administration change owns safe client configuration writes and the console consumes the versioned REST API. The release must declare the runtime needed by each enabled driver.
