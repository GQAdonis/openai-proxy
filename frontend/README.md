# Embedded console

The console uses KnowMe's desktop monogram, app icon, color ramps and typography from `know-me-system`. Its light and dark palettes follow the browser's color preference, with fonts and icons bundled locally.

Prepare the build host with Node 26.5.0 and npm 11.16.0. Run `npm ci --include=dev` in this directory before a Cargo build, test, or release. The compiler runs TypeScript, generates offline A2UI validators, and builds Vite into Cargo's OUT_DIR. Cargo embeds the resulting index, styles, JavaScript and fonts. Run `cargo build --release --locked` at the repository root. The released HTTP service serves the console at `/ui/` without a JavaScript runtime or a source checkout. Native chat drivers have independent runtime prerequisites.

For development, run the service on port 8181 and `npm run dev` here. Open the Vite URL under `/ui/`. Production packaging and acceptance use the service origin. Local pairing and shared session tokens are entered only into the session sign-in form. The browser uses the issued HttpOnly cookie for subsequent requests.

Use the Docker `runtime` target for the embedded web/API service alone. The default `sdk-runtime` target also ships Bun, the version-locked SDK workers, the sibling delegation plugin, and the platform-specific Codex CLI 0.169.0. Compose binds the listener to loopback and mounts a private Codex home directory, persistent service data, and a separate workspace. When overriding the external port, set PROXY_CONSOLE_ORIGIN to the browser's exact origin. Provision the dedicated Codex volume or directory with the authorized account's login at runtime. With the default named volumes the console pairing code is in the private file identified in the service log.
