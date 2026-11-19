## ADDED Requirements

### Requirement: Effective settings and privilege boundaries

The service SHALL provide all 16 settings areas in the capability inventory to an authorized operator, showing the effective value, source, mutability, privileges, apply timing and any unmet prerequisite. The service SHALL enforce identical permissions on direct management requests and UI-originated requests.

#### Scenario: Local owner inspects a masked setting

- **WHEN** a paired local owner opens settings with a port overridden by the serving process and memory omitted at compile time
- **THEN** the UI identifies the effective listener, its overriding source and restart requirement and explains why memory controls are unavailable.

#### Scenario: Shared chat user inspects their permitted connection

- **WHEN** a signed-in chat user requests `GET /api/v1/me/connection`
- **THEN** the server returns only their redacted connection state, permitted driver IDs, availability, supported controls and allowed recovery actions. A request for another subject is rejected without leaking that subject’s existence or state.

#### Scenario: Shared guest attempts privileged changes

- **WHEN** a chat-only user requests host edits or another user's credential refresh, or the browser submits a cross-origin management request
- **THEN** the server rejects the action, causes no write or refresh, and withholds credentials, tokens and other users' metadata from the response and logs.

### Requirement: OpenCode setup preserves local configuration

The service SHALL preview, apply and undo project and user-level OpenCode V2 setup through a version-aware writer that preserves unrelated values, comments, permissions and user overrides. It SHALL distinguish plugin registration from the static provider, the SDK chat driver and Codex-native plugin installation.

#### Scenario: Project and global installs have different destinations

- **WHEN** an owner selects global setup, verifies the preview, applies it twice and launches OpenCode from a directory outside the project
- **THEN** the installed host discovers the intended provider and model, the second apply changes no bytes, unrelated settings survive and undo removes only the owned entries.

#### Scenario: V1 configuration or an unsupported OpenCode host is discovered

- **WHEN** setup CLI or the admin UI detects a V1/mixed config, unknown host version, or a host outside the supported stable V2 range
- **THEN** it reports the affected layer and target version, preserves all existing file bytes and permission bits, and provides upgrade guidance or an explicitly V2-labelled export
- **AND** an attempt to apply with a previous preview or a force flag produces no host write

#### Scenario: Preview becomes stale

- **WHEN** another process edits the target between preview and apply or undo, or when the file is malformed, outside scope or redirected to an unapproved destination
- **THEN** the service returns a conflict or validation error, preserves all operator content and offers a new preview where possible.

### Requirement: Editors can launch the proxy over ACP

The service SHALL provision Zed and JetBrains AI Assistant as ACP clients of the installed `openai-proxy serve --acp-stdio` command and offer a generic stdio launch descriptor for other compatible clients. It SHALL expose only capabilities the installed adapter can honor.

#### Scenario: Editor starts a session

- **WHEN** an operator applies the editor-specific configuration in a supported editor profile and sends a prompt
- **THEN** the editor initializes the named proxy agent, creates a session, receives a streamed update and reports the actual terminal outcome; diagnostics stay off protocol stdout.

#### Scenario: A remote browser requests editor setup

- **WHEN** the editor is on the visitor's machine or a requested client version is unsupported on the service host
- **THEN** the console identifies the target boundary and offers a version-labelled export or an unavailable reason without editing a different machine.

### Requirement: Common clients can register the proxy as an MCP server

The service SHALL offer scoped registration for Codex CLI, Claude Code, OpenCode V2, Zed and VS Code/Copilot through each supported client's configuration. It SHALL support stdio and Streamable HTTP wherever the negotiated client/transport supports them, preserve the four public tool names and protect account metadata.

#### Scenario: Clients discover and invoke a registered proxy

- **WHEN** a real configured client connects to its registered stdio command or protected HTTP endpoint and lists tools
- **THEN** the client can invoke a benign tool, sees the effective model catalog, and distinguishes advisory model selection from global mutation. Redacted auth status is scoped to its owner.

#### Scenario: OpenCode registration uses its native V2 schema

- **WHEN** a supported OpenCode host receives an authorized stdio or HTTP MCP registration
- **THEN** the scoped writer adds the owned entry under `mcp.servers`, and the client can list and call the proxy's tools
- **AND** a legacy direct-entry MCP file blocks the registration before any write and retains its previous bytes

#### Scenario: Client cannot complete the requested transport

- **WHEN** setup encounters an unsupported protocol version, unavailable listener, or missing executable
- **THEN** the UI reports that registration is unverified with its specific cause, and preserves other installed servers.

### Requirement: Optional features reflect effective runtime behavior

The service SHALL distinguish build-disabled, unconfigured, overridden, unhealthy and ready states and enforce actor scope for tools, skills, memory, hooks and discovery across applicable modes.

#### Scenario: Operator changes skill, hook and discovery configuration

- **WHEN** an authorized operator selects an injection limit, a synthetic webhook destination and an A2A card setting
- **THEN** the effective selector limit, observed hook event and externally served card match the applied mode or report a pending restart. A static MCP schema is identified separately from its executor.

#### Scenario: Shared user tries to access another user's memory

- **WHEN** a user searches, lists, deletes or prompts with another user's known document identifier or supplied scope header
- **THEN** the service denies or filters the operation, preserves the document and prevents cross-user retrieval or injection. Embeddings show their own credential and billing boundary.
