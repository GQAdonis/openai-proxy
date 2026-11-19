# openai-proxy constraints

Use the project instructions and the active phase's verification boundary. The default Rust build must complete without warnings. Keep the memory backend feature-gated.

```yaml
constraints:
  - id: default-build
    severity: blocking
    command: cargo build
  - id: rust-warnings
    severity: blocking
    command: cargo clippy -- -D warnings
  - id: unit-tests
    severity: blocking
    command: cargo test --lib
  - id: opencode-typecheck
    severity: blocking
    command: bun run --cwd plugin typecheck
```

Live integration tests consume provider quota and require credentials. Use isolated credential and transport fixtures for routine verification. Preserve user provider overrides and resolve models against the service catalogue.
