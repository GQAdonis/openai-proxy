---
name: proxy-packaging
description: Build and distribute an embedded Vite frontend from Cargo build.rs and preserve the proxy's operational entry points.
---

# Build and release

The proposed frontend lives under `frontend/`. Agree on its pinned package manager, lockfile, build command and output contract with chat-experience. Keep dependency acquisition explicit in developer, CI and container setup. Have `build.rs` invoke the locked project's Vite build, check its exit status, and emit assets under Cargo's `OUT_DIR`. Embed the emitted index and asset table in the Rust binary. Avoid recursive Cargo invocations and rebuilding because generated outputs changed.

Declare rerun inputs for UI source, public assets, lockfile, package manifest, HTML, tsconfig, Vite config and build-relevant environment. Serve correct content types and caching for hashed assets. Limit SPA fallback to HTML navigation under the UI route so a missing asset or unknown API route stays a 404. Preserve API/MCP/ACP modes, optional Rust features, and a runtime image that serves the UI without Node or a checkout.

Verify a clean release build, incremental rebuild after a UI edit, a failed frontend build, and execution from a directory without frontend source. Inspect the shipped artifact and container stages. Record toolchain prerequisites and artifact provenance.

Sources: [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html), [Vite production build](https://vite.dev/guide/build.html).
