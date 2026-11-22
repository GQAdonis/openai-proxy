# Embedded KnowMe console

The console uses the KnowMe desktop monogram, app icon, colors, and typefaces. Fonts and icons ship locally, and the light and dark palettes follow the browser's preference.

## Build

Use Node 26.5.0 and npm 11.16.0. From the repository root, install the locked dependencies and build the service.

```sh
npm ci --prefix frontend --include=dev
cargo build --release --locked
```

Cargo runs the TypeScript check, generates offline A2UI validators, and builds Vite into Cargo's output directory. The Rust binary embeds the resulting HTML, CSS, JavaScript, and fonts. The frontend uses React 19 and Vite 8. AG-UI client packages are pinned at 1.0.5. The renderer validates A2UI 1.0 snapshots against the [vendored wire schemas](src/schemas/README.md).

## Run

Open the service's `/ui/` for chat or `/ui/settings` for connections and client setup. A local loopback installation can use a pairing code or opt into an automatic local browser session. Shared deployments use individual principal tokens. The browser uses the service's HttpOnly session cookie for subsequent requests. Native chat requires the matching Codex or OpenCode SDK runtime and its account or upstream configuration. [Console operation](../docs/console.md)

For frontend development, run the service on port 8181 and then run `npm run dev` from this directory. Use the Vite URL under `/ui/`. Use the service origin to verify a production build and its browser session behavior.

The Docker `runtime` target carries the embedded UI and API. The default `sdk-runtime` target adds Bun, the locked workers, the sibling OpenCode plugin, and the platform-specific Codex CLI. On macOS, the updater builds both layers for the existing versioned user LaunchAgent and saves a verified rollback receipt. [LaunchAgent deployment](../docs/console.md#update-the-macos-launchagent)
