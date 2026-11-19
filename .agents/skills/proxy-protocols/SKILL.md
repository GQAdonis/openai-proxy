---
name: proxy-protocols
description: Maintain Axum routing and OpenAI-compatible, AG-UI, ACP, MCP and A2A service contracts.
---

# Proxy protocols

Trace each normal entry point to `AppState`, its upstream request, and the return transport. Compare the two wire formats in `src/proxy.rs`, model resolution in `src/model_catalog.rs` and the auth interface owned by codex-identity. Keep backend URL, profile, user identity and request headers consistent within a request. Update HTTP and stdio callers when replacing a startup credential.

Translate text, tool calls, final tool arguments, finish reasons, usage, cancellation and upstream errors. Account for partial SSE frames, disconnects and backpressure. Preserve the existing MCP tool schema, AG-UI event ordering and API route status codes when mounting the embedded UI. Keep administrative requests on an authenticated same-origin route with an explicit origin policy. Review the current permissive CORS layer and listener settings before exposing that route.

Hand stable request, stream and redacted admin-status fixtures to the UI owner. Verify actual bytes and event order using local fake upstreams and the normal router. Source contracts are maintained in `docs/ag-ui.md`, `docs/acp.md`, `docs/a2a.md` and the [Responses streaming reference](https://platform.openai.com/docs/api-reference/responses-streaming).
