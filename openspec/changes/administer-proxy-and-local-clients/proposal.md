## Why

Configuring this proxy currently means combining CLI flags, TOML, environment variables and client-specific files. The existing setup commands can replace an unparseable configuration, and the public HTTP service has no authenticated host administration interface.

## What Changes

- Add an authenticated settings API and settings panels for every supported deployment, integration and optional feature.
- Provide scoped discovery, effective configuration, preview, apply and undo for local OpenCode, ACP and MCP clients.
- Make real client behavior, account isolation and preservation of operator files acceptance requirements.

## Capabilities

### New Capabilities

- `proxy-administration`: Effective settings, local machine provisioning, protocol registration and feature administration.

### Modified Capabilities

None.

## Impact

Scope: admin routes and authorization in Axum, effective configuration and CLI setup, integration-specific settings panels, MCP/ACP adapters, optional memory, hooks, skills, model discovery and configuration writers. The identity change owns credential reconciliation. The SDK change owns provider registration and driver execution. The embedded console change owns the shell, common controls, asset serving and A2UI presentation.
