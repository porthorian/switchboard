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
Milestone 1 now boots a privileged CEF UI view at `app://ui`, so a CEF distribution path is required on macOS.

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

The `app://ui` shell currently exposes a minimal prompt-based bridge marker (`__switchboard_intent__`) with a strict allowlist (`navigate http(s)://...`).

Optional overrides:
- `SWITCHBOARD_CEF_FRAMEWORK_DIR`
- `SWITCHBOARD_CEF_RESOURCES_DIR`
- `SWITCHBOARD_CEF_BROWSER_SUBPROCESS`
- `SWITCHBOARD_CEF_MAIN_BUNDLE_PATH`
- `SWITCHBOARD_CEF_API_VERSION` (defaults to `14500`; set explicitly if using a different CEF build)
- `SWITCHBOARD_CEF_ROOT_CACHE_PATH`
- `SWITCHBOARD_CEF_TMPDIR`
- `SWITCHBOARD_CEF_USE_MOCK_KEYCHAIN` (`1/true` to force `--use-mock-keychain`, defaults to enabled in debug builds)
- `SWITCHBOARD_CEF_PASSWORD_STORE` (optional Chromium `--password-store=<value>`, e.g. `basic` for dev)
- `SWITCHBOARD_CEF_VERBOSE_ERRORS` (`1` to include raw loader details)

Note on macOS keychain prompts:
- CEF/Chromium uses the login keychain by default (`Chromium Safe Storage` entry).
- Upstream Chromium does not provide a simple runtime switch for a custom keychain name like `Switchboard`; that requires deeper platform customization.
- For local development, mock keychain mode avoids repeated prompts.

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
