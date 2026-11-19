# SDK chat drivers

## Purpose

Allow authorized users to use Codex from OpenCode and to run persistent chat conversations against either SDK through the proxy.

## ADDED Requirements

### Requirement: OpenCode V2 submits subscription turns to the Codex provider

The plugin SHALL register a Codex provider through the V2 ABI on a supported stable OpenCode host (minimum 2.3.0, major 2), locally and from its packed distribution. It SHALL preserve overrides, reload the permitted catalog after a confirmed login, keep credentials out of configuration, logs and shell environments, and route subscription turns to the consenting principal’s Codex runtime. It SHALL surface denials without implicit fallback.

#### Scenario: Local and packaged provider complete a normal OpenCode turn

- **WHEN** an operator loads each distribution in an isolated 2.3.0 host and a subscriber submits a supported text prompt through the registered model
- **THEN** the actual host discovers the provider, receives a streamed response through the Codex SDK route, and the recorded destination identifies the selected principal without exposing credentials
- **AND** a global registration is discoverable when the host starts from outside the project and prior user settings survive installation

#### Scenario: Host or plugin ABI is unsupported

- **WHEN** the host identifies as 1.x, a release below 2.3.0, a prerelease, a different major or an unknown version, or it lacks a required V2 capability
- **THEN** setup reports the incompatible host before registration or file writes, and the released plugin offers no V1 hooks, entry point or fallback
- **AND** a later eligible 2.x host is marked verified only after real local/packed discovery and an observed turn on that version

#### Scenario: Login refreshes only the intended inventory

- **WHEN** the plugin finishes interactive login and the identity service confirms the new generation
- **THEN** the plugin reloads the inventory for that connection, preserves user overrides, and can service the next eligible turn without restarting the proxy
- **AND** a failed or cancelled login leaves the previous committed identity governed by the resolver

#### Scenario: Quota denial and evidence of Pro use

- **WHEN** the provider rejects a model, revokes access, exhausts quota or the operator lacks independent usage attribution
- **THEN** the service reports the corresponding unavailable, denied, quota or unverified state, makes no second billable attempt, and keeps every account's state separate
- **AND** certification of highest-tier ChatGPT Pro usage requires a separately authorized real turn with redacted, independently attributable allowance evidence

### Requirement: Authenticated conversations use the selected SDK driver

The versioned REST API SHALL bind each conversation to its owner, approved workspace, account, driver and native session. Codex SDK and OpenCode SDK SHALL both support durable turn admission, ordered output, native cancellation, status and follow-up in the same native session. Reconnection SHALL never repeat a prompt or a tool effect.

#### Scenario: Each driver streams a turn and resumes after restart

- **WHEN** the same user creates separate Codex and OpenCode conversations, sends a prompt to each and later reopens them after a supervised worker restart
- **THEN** the independent driver trace identifies the selected native SDK and original native thread/session ID for each conversation, incremental output is emitted once in order, and a follow-up resumes the correct native context
- **AND** an event subscriber reconnecting from a retained event ID receives only later events

#### Scenario: Stop and recovery reflect native work

- **WHEN** an owner cancels an active turn on either driver, or a worker disappears while a turn may still be running
- **THEN** cancellation reaches the native runtime, and the API records a cancelled terminal state only once completion of cancellation is established
- **AND** ambiguous outcomes remain identifiable pending reconciliation, a retry of the same admitted turn cannot execute it twice, and missing workers cannot silently change the selected driver

#### Scenario: Missing V2 SDK worker

- **WHEN** an OpenCode driver is requested but its packaged V2 SDK or compatible execution runtime is unavailable
- **THEN** admission returns driver-unavailable without selecting a V1 SDK, using API billing or switching to another driver
- **AND** available Codex and direct REST routes retain their own capability status

#### Scenario: Another user supplies a conversation or permission reference

- **WHEN** a caller presents another user's conversation, native session, event cursor or pending permission reference
- **THEN** the server rejects access before a native read or write, and reveals no conversation existence, output, workspace, actions or credentials

### Requirement: One execution owner enforces tool policy per turn

Each turn SHALL bind one execution owner, route and native permission policy. The text provider SHALL reject unsupported host-tool requests before execution. Explicit OpenCode delegation SHALL run one whole Codex-owned turn and return its result once. The service SHALL block recursive routes before further native work.

#### Scenario: Native tools execute once and denied effects never occur

- **WHEN** an authorized user invokes a permitted, observable tool and attempts a denied operation on each applicable driver, including a delegated Codex turn
- **THEN** the independent target records exactly one authorized effect and zero denied effects, and native tool observations remain telemetry
- **AND** replaying the same turn or delegation ID does not repeat the effect

#### Scenario: Unsupported function inputs or recursion

- **WHEN** a text-provider turn requires host-owned function calls or an OpenCode driver selects a destination that resolves back into its own orchestration path
- **THEN** the request fails with an actionable capability or routing error before another native turn, credential exchange, or tool effect
