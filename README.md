# Switchboard

Rust-first browser orchestration scaffold for a CEF-based macOS browser.

## Workspace layout

- `crates/switchboard-core`: Canonical state, intents, reducer, snapshot/patch engine.
- `crates/switchboard-app`: Minimal binary bootstrap entrypoint.
- `crates/switchboard-cef-sys`: CEF FFI types and dynamic symbol loader (`dlopen`).
- `docs/DESIGN_DOC.md`: Architecture and lifecycle contract.
- `AGENTS.md`: Agent operating constraints.

## Development build

```bash
cargo test
SWITCHBOARD_CEF_DIST=/absolute/path/to/cef_binary_145... ./scripts/build_macos_dev_app.sh
open "target/dev-bundle/Switchboard Dev.app"
```

The browser must run from the generated `.app`. Bare `cargo run` is intentionally rejected because it cannot provide the required CEF helper applications, framework resources, sandbox library, entitlements, or signing order. The bundle locates CEF and its dedicated helper automatically.

The UI bridge uses versioned JSON envelopes, pushed snapshots/patches, strict command decoding, and no snapshot polling or prompt-based transport.

Optional overrides:
- `SWITCHBOARD_CEF_FRAMEWORK_DIR`
- `SWITCHBOARD_CEF_RESOURCES_DIR`
- `SWITCHBOARD_CEF_BROWSER_SUBPROCESS`
- `SWITCHBOARD_CEF_MAIN_BUNDLE_PATH`
- `SWITCHBOARD_CEF_API_VERSION` (defaults to `14500`; set explicitly if using a different CEF build)
- `SWITCHBOARD_CEF_ROOT_CACHE_PATH`
- `SWITCHBOARD_CEF_TMPDIR`
- `SWITCHBOARD_STATE_DB_PATH` (absolute development/smoke database path; defaults to Application Support)
- `SWITCHBOARD_CEF_USE_MOCK_KEYCHAIN` (`1/true` only inside the dedicated `Switchboard Smoke.app`; rejected by normal development bundles)
- `SWITCHBOARD_CEF_PASSWORD_STORE` (optional Chromium `--password-store=<value>`, e.g. `basic` for dev)
- `SWITCHBOARD_CEF_AUTOPLAY_POLICY` (optional Chromium `--autoplay-policy=<value>`, defaults to `no-user-gesture-required`)
- `SWITCHBOARD_CEF_VERBOSE_ERRORS` (`1` to include raw loader details)

Media playback note:
- YouTube Live and other livestreams often require H264/AAC support.
- The standard packaged CEF framework does not dynamically load a standalone `libffmpeg.dylib`; copying one into the bundle does not enable proprietary codecs.
- H264/AAC support requires a complete matching CEF build with the appropriate Chromium codec flags and a separate licensing review; see [issue #1](https://github.com/porthorian/switchboard/issues/1).

Note on macOS keychain prompts:
- CEF/Chromium uses the login keychain by default (`Chromium Safe Storage` entry).
- Upstream Chromium does not provide a simple runtime switch for a custom keychain name like `Switchboard`; that requires deeper platform customization.
- Development builds use the real macOS Keychain. Mock Keychain behavior is limited to the automated smoke bundle.

## Current product scope

Switchboard supports Rust-authoritative profiles/workspaces, arbitrary-depth tab trees, completion/archive, snooze, Lazy Search, per-profile history, two-pane split state, focus mode, Browser/Insert modes, native Chromium history, and a two-visible-plus-eight-warm lifecycle budget. Password/extension controls are deliberately absent; see [the compatibility gate](docs/PASSWORD_EXTENSION_COMPATIBILITY.md).

## CEF bindings generation

Normal builds consume the checked-in bindings snapshot generated from CEF `145.0.26+g6ed7554+chromium-145.0.7632.110` with API version `14500`. Regeneration is an explicit developer action and rejects a mismatched distribution.

```bash
./scripts/generate_cef_bindings.sh /absolute/path/to/cef_binary_145...
./scripts/generate_cef_bindings.sh --check /absolute/path/to/cef_binary_145...
```
