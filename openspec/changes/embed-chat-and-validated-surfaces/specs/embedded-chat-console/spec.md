# Spec Delta

## Purpose

The embedded chat console lets an authorized user stream conversations and see effective settings from the running proxy. Its versioned AG-UI interface renders validated interactive A2UI messages in the originating chat.

## ADDED Requirements

### Requirement: Packaged chat and settings console

The HTTP service SHALL serve an accessible React 19 and Vite 8 chat and settings console built by Cargo `build.rs` and embedded in the release binary. It SHALL use the selected SDK and server-authorized settings scope in local-owner and shared deployments. The release and user-visible outcomes are tracked as UI-1.

#### Scenario: Binary and container serve the embedded console

- **WHEN** an operator prepares locked dependencies, builds the release, starts the binary with `openai-proxy serve` from an empty working directory, or launches the final release image
- **THEN** `/ui/` and a direct navigation to `/ui/settings` load with no checkout, separate frontend files, frontend dev server or Node runtime needed to serve them
- **AND** real fingerprinted assets have the correct MIME type and immutable cache policy, HTML revalidates, missing assets and unknown API routes return errors, and existing HTTP and exclusive stdio entry points retain their protocol behavior

#### Scenario: An edited or broken frontend affects the actual Cargo build

- **WHEN** a tracked frontend input changes and Cargo rebuilds the release
- **THEN** the shipped artifact serves the new content from its embedded asset table
- **AND** a subsequent failed Vite invocation or missing referenced asset fails the build and cannot publish the preceding bundle as that release

#### Scenario: A keyboard user streams, reconnects and stops a turn

- **WHEN** an authenticated user selects an available Codex SDK or OpenCode SDK conversation and sends a message using the keyboard
- **THEN** the console uses the selected conversation and turn API, renders incremental text once, preserves the reader's scroll position, and provides visible focus and concise status announcements on desktop and narrow screens
- **AND** reconnect resumes from the last accepted monotonic event ID without submitting a second turn, Stop displays the native cancellation outcome, and a failure before or after output exposes recovery and retains any partial answer
- **AND** the console reports an unavailable selected driver, expired replay window or rejected permission distinctly and leaves the user in control of any new submission

#### Scenario: Settings show what takes effect and where

- **WHEN** a local owner or shared actor opens the permitted settings sections
- **THEN** each option shows the service-reported supported/enabled state, effective redacted value and source, scope/host, allowed actor, and live, per-turn, client-reload, service-restart, externally managed or unavailable effect as applicable
- **AND** the console covers the sixteen phase inventory areas, exposes privileged redacted previews and authorized apply/undo where supported, and identifies remote, read-only, degraded or overridden targets
- **AND** it distinguishes reload, managed refresh in progress, latest pickup, retry and sign-in-required, without putting upstream access tokens, refresh tokens or API keys in browser storage, DOM, logs or requests

#### Scenario: Chat-only access cannot administer the shared service

- **WHEN** a shared chat-only actor opens a settings deep link or submits a direct privileged management request, or supplies another user's identifier in a body
- **THEN** server-derived identity and capabilities restrict the response, the browser does not expose privileged data and the request causes no host write or credential operation
- **AND** a credential owner can act only within their authorized scope, and a service administrator sees only permitted redacted status for another account

### Requirement: Versioned AG-UI streaming and validated A2UI interactions

The service SHALL accept typed AG-UI run inputs at `POST /api/v1/ag-ui/stream`, emit interoperable SSE events and retain the existing `POST /ag-ui/stream` text contract. The console SHALL render only validated A2UI v0.9 activity snapshots using pinned React-compatible package exports and SHALL submit only authorized actions scoped to a live surface. The protocol and user-visible outcomes are tracked as SURFACE-1.

#### Scenario: Typed clients distinguish successful and failed runs

- **WHEN** the versioned client starts a run and the driver emits text, an error, or an incomplete stream
- **THEN** the client accepts typed AG-UI SSE with camelCase identifiers, ordered text lifecycle and exactly one appropriate terminal event for a completed or failed run
- **AND** a known backend error yields `RUN_ERROR`, an interrupted transport leaves the run interrupted, and a legacy client still receives the established text contract at its original path

#### Scenario: An authorized surface can be created, updated, replayed and deleted

- **WHEN** an authorized run delivers an `ACTIVITY_SNAPSHOT` for `activityType: "a2ui-surface"` with a stable `messageId`, `replace: true` and a complete valid `content.a2ui_operations` sequence
- **THEN** the pinned v0.9 processor and React renderer display only the approved catalog's accessible components, apply component/data updates, remove deleted surfaces and give focus a valid destination
- **AND** reapplying the same snapshot produces the same visible state, and another actor's same-named surface or another run's updates cannot change it

#### Scenario: Keyboard activation dispatches one scoped action

- **WHEN** the user activates a permitted live control from the keyboard and the client validates the v0.9 `{version,action}` message and forwards the action in `forwardedProps.a2uiAction.userAction` on a correlated run
- **THEN** the server checks authenticated actor, owning conversation/run, current surface and component, allowed action, trusted origin, context schema and deduplication key before dispatch
- **AND** a valid activation produces one observable effect and a repeated delivery produces no additional effect
- **AND** a forged, stale, foreign or deleted control, an attempted admin action, or a request without the required session/origin protection produces no effect

#### Scenario: Invalid surface content preserves usable chat

- **WHEN** a stream contains plain JSON-looking text or a malformed, unsupported, oversized, truncated, disallowed or out-of-order surface payload
- **THEN** text remains inert, invalid surface input makes no partial change, and the last safe surface and accessible text/error fallback remain usable
- **AND** unregistered components, functions, unsafe resource URLs and generated privileged callbacks cannot trigger execution, fetch a remote catalog or access service settings
