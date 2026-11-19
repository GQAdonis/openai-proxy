# openai-proxy

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

React 19 and Vite 8. The Rust service will serve the frontend from the same binary. Cargo's build script builds and embeds it. UI components use shadcn/ui.

## Users

A developer running a local proxy can chat and manage their own connection. In a shared deployment, chat users send messages and administrators manage the service connection. Authorization belongs to the service.

## Product purpose

Let people send a message, follow the streaming reply, and tell whether a failed request can be retried. Let an authorized operator see connection health and request a refresh. A completed Codex login should take effect without a service restart.

## Operating context

Chat and administration are proposed surfaces. The existing service exposes OpenAI-compatible endpoints and streaming transports. A chat user needs an actionable connection status. An administrator needs refresh progress, the last successful update and an indication when sign-in is required. Credential values stay out of both surfaces.
