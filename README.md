# Switchboard

Rust-first browser orchestration scaffold for a CEF-based macOS browser.

## Workspace layout

- `crates/switchboard-core`: Canonical state, intents, reducer, snapshot/patch engine.
- `crates/switchboard-app`: Minimal binary bootstrap entrypoint.
- `docs/DESIGN_DOC.md`: Architecture and lifecycle contract.
- `AGENTS.md`: Agent operating constraints.

## Quick start

```bash
cargo run -p switchboard-app
cargo test
```
