---
name: proxy-interface
description: Design and implement the React 19 and Vite 8 streaming chat and credential administration interface.
---

# Chat and administration

Read `PRODUCT.md` and the current project Impeccable context first. Work in `frontend/`. Use Impeccable for the design brief and craft, UI UX Pro Max for focused interaction or accessibility research, and the official shadcn skill for component discovery and composition. Inspect the Vite app's own `components.json` before selecting registry components. Consider AI Elements for conversation/scroll behavior only after comparing its message model to the proxy's real stream. Chat can use the existing HTTP protocol directly.

Cover the local developer who operates both surfaces and the shared deployment with separate chat and admin permissions. Model streaming, stop, failure before output, partial responses, retry and focus return. Respect a reader who has scrolled up. Provide keyboard-accessible controls, usable narrow layouts, visible focus and completion/status announcements that avoid announcing every token. Present refresh-in-progress, refreshed, temporary failure and sign-in-required to authorized operators. Keep bearer values and upstream account secrets out of the DOM, storage, browser logs and browser requests. Hiding an admin button complements server enforcement.

Agree on the API and asset base paths with runtime and build-release. Verify from the packaged Rust service with network access to the Vite dev server disabled. Use deterministic SSE and admin fixtures for UI review.

Sources: [shadcn agent skill](https://ui.shadcn.com/docs/skills), [shadcn on Vite](https://ui.shadcn.com/docs/installation/vite), [AI Elements Conversation](https://elements.ai-sdk.dev/components/conversation), [Vite backend integration](https://vite.dev/guide/backend-integration.html).
