# macOS CEF smoke gate

The deterministic Rust suite is necessary but not sufficient. Run this gate against the generated, ad-hoc-signed `.app` with the pinned CEF 145 distribution.

## Build

```bash
SWITCHBOARD_BUILD_SMOKE=1 \
SWITCHBOARD_CEF_DIST=/absolute/path/to/cef_binary_145... \
./scripts/build_macos_dev_app.sh
codesign --verify --deep --strict "target/dev-bundle/Switchboard Smoke.app"
```

For an isolated QA run, launch the generated executable with absolute temporary paths rather than mutating the normal profile:

```bash
SMOKE_ROOT="$(mktemp -d /private/tmp/switchboard-smoke.XXXXXX)"
SWITCHBOARD_STATE_DB_PATH="$SMOKE_ROOT/state.sqlite3" \
SWITCHBOARD_CEF_ROOT_CACHE_PATH="$SMOKE_ROOT/cef" \
SWITCHBOARD_CEF_TMPDIR="$SMOKE_ROOT/tmp" \
SWITCHBOARD_CEF_USE_MOCK_KEYCHAIN=1 \
SWITCHBOARD_CEF_VERBOSE_ERRORS=1 \
"target/dev-bundle/Switchboard Smoke.app/Contents/MacOS/switchboard-app"
```

The mock-keychain switch is rejected by `Switchboard Dev.app`; ordinary development and manual password-provider verification continue to use the real macOS Keychain.

Serve the deterministic fixture from a separate terminal:

```bash
python3 -m http.server 18765 --bind 127.0.0.1 --directory docs/smoke-fixture
```

## Required evidence

- Chromium sandbox enabled; missing helper, resource, framework, or `libcef_sandbox.dylib` fails startup.
- `http://127.0.0.1:18765/` visibly renders the `SWITCHBOARD_LOCAL_SMOKE_OK` marker and `https://example.com` renders over DNS/TLS.
- Navigating the same tab to a second URL, then using back, forward, and reload, reuses one browser generation without a crash.
- An invalid hostname shows Chromium's native error page and is not written to history.
- Exactly one ephemeral request context serves the exact main-frame `app://ui` browser.
- A content browser cannot load `app://` or dispatch privileged commands.
- Cookies and local storage are shared by tabs in one profile and isolated across profiles and restarts.
- A live tab has one browser generation; stale callbacks cannot update it.
- A discarded tab shows `Restoring`, then creates CEF only after the matching animation-frame acknowledgement.
- Closing/discarding reaches `OnBeforeClose` for every browser, releases the client only afterward, and completes without a surviving helper process.
- Split mode shows exactly two browsers; the steady-state 200-tab workload has no more than two visible plus eight warm browsers, and inactive profiles have zero.
- No Chrome toolbar, telemetry, UI polling, or background UI network traffic appears.
- The run creates no new macOS crash report, logs no profile-context creation failure, and leaves no helper process behind after shutdown.

## Status

The repository contains deterministic reducer, persistence, bridge, ABI-layout, and mock-host coverage for these invariants. Native smoke execution is environment-dependent and must not be recorded as passing until the generated CEF bundle is run and the evidence above is captured. Stop the fixture server, verify port `18765` is released, and remove only the `SMOKE_ROOT` directory created for the run.
