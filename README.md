# Switchboard

Rust-first browser orchestration scaffold for a CEF-based macOS browser.

## Workspace layout

- `crates/switchboard-core`: Canonical state, intents, reducer, snapshot/patch engine.
- `crates/switchboard-app`: Minimal binary bootstrap entrypoint.
- `crates/switchboard-cef-sys`: CEF FFI types and dynamic symbol loader (`dlopen`).
- `docs/DESIGN_DOC.md`: Architecture and lifecycle contract.
- `AGENTS.md`: Agent operating constraints.

## Quick start

```bash
cargo run -p switchboard-app
cargo test
```

`cargo run -p switchboard-app` starts the Milestone 1 shell and opens a native macOS window.
By default this uses a `WKWebView` fallback for content rendering.

To render content with CEF, provide either a distribution root or an explicit framework binary:

```bash
SWITCHBOARD_CEF_DIST=/Users/vmarone/projects/cef_binary_145.0.26+g6ed7554+chromium-145.0.7632.110_macosarm64 \
cargo run -p switchboard-app
```

Or:

```bash
SWITCHBOARD_CEF_LIBRARY=/path/to/Release/Chromium\ Embedded\ Framework.framework/Chromium\ Embedded\ Framework \
cargo run -p switchboard-app
```

Optional overrides:
- `SWITCHBOARD_CEF_FRAMEWORK_DIR`
- `SWITCHBOARD_CEF_RESOURCES_DIR`
- `SWITCHBOARD_CEF_BROWSER_SUBPROCESS`
- `SWITCHBOARD_CEF_MAIN_BUNDLE_PATH`
- `SWITCHBOARD_CEF_API_VERSION` (defaults to `14500`; set explicitly if using a different CEF build)
- `SWITCHBOARD_CEF_ROOT_CACHE_PATH`
- `SWITCHBOARD_CEF_TMPDIR`
- `SWITCHBOARD_CEF_VERBOSE_ERRORS` (`1` to include raw loader details)

## CEF bindings generation

The `switchboard-cef-sys` crate supports optional bindgen-driven CEF bindings generation.

```bash
./scripts/generate_cef_bindings.sh <cef_header> [cef_include_dir]
```

Or directly via env vars:

```bash
SWITCHBOARD_CEF_GENERATE_BINDINGS=1 \
SWITCHBOARD_CEF_HEADER=/path/to/cef/include/capi/cef_app_capi.h \
SWITCHBOARD_CEF_INCLUDE_DIR=/path/to/cef/include \
cargo check -p switchboard-cef-sys
```
