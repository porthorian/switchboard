# Password-extension compatibility gate

## Alpha decision

Status: **deferred; no password or extension controls ship in this alpha**.

Switchboard uses custom Alloy-style content windows. CEF 145 documents Chrome style as the surface with full Chrome browser functionality, while the pinned CEF 145 `CefRequestContext` API exposes no custom Alloy extension loader. This gate therefore begins with an early capability check and fails closed if installation requires Chrome-style windows, exposes Chrome toolbar UI, or weakens profile or bridge isolation.

The gate does not permit a partial provider rollout, a duplicate Chrome toolbar, a custom password vault, or a provider-specific exception. Development and provider QA use the real macOS Keychain. Mock Keychain and password-store overrides remain limited to `Switchboard Smoke.app`; Dev and Extension QA reject both substitutions.

References: [CEF 145 runtime styles](https://cef-builds.spotifycdn.com/docs/145.0/cef__types__runtime_8h.html), [CEF 145 request contexts](https://cef-builds.spotifycdn.com/docs/145.0/classCefRequestContext.html), and the CEF maintainer guidance linked in the roadmap.

## Automated preflight

Build the dedicated, ad-hoc-signed QA bundle and inspect the exact pinned CEF surface:

```bash
SWITCHBOARD_BUILD_EXTENSION_QA=1 ./scripts/build_macos_dev_app.sh
codesign --verify --deep --strict "target/dev-bundle/Switchboard Extension QA.app"
./scripts/check_password_extension_surface.sh target/cef-sdk-145
```

Expected preflight evidence for the pinned build:

- CEF `145.0.26+g6ed7554+chromium-145.0.7632.110`, API `14500`;
- Switchboard content browsers explicitly use `CEF_RUNTIME_STYLE_ALLOY`;
- the CEF 145 request-context C API has no `load_extension`, `has_extension`, `get_extensions`, or `did_load_extension` entry point; and
- product password/extension controls remain disabled.

This is a capability warning, not a substitute for the native Web Store attempt. If the pinned CEF surface changes, stop and repeat the security and ABI review before testing providers.

## Isolated native session

Serve the fixture in one terminal:

```bash
python3 -m http.server 18765 --bind 127.0.0.1 --directory docs/smoke-fixture
```

Launch the QA app in another terminal:

```bash
./scripts/run_password_extension_qa.sh
```

The launcher prints a unique `/private/tmp/switchboard-extension-qa.*` session root. Re-run it with that exact path to test restart persistence. It never enables mock/basic password storage and does not delete the session automatically. Remove only that printed directory after the evidence is recorded and the app/server are stopped.

Navigate manually to `http://127.0.0.1:18765/password-extension.html`. The fixture exposes standard username/password fields, reports only filled/empty state and character counts, and never persists, logs, or sends credential values. It also exercises cookies, local storage, Cache Storage, service workers, a content-origin bridge command, and a user-initiated `app://ui` popup attempt.

## Early stop and provider procedure

1. From a normal content tab, manually navigate to each official Chrome Web Store listing. No `app://ui` background request may fetch the stores.
2. Attempt user-initiated installation. If installation cannot complete in custom chrome, requires a Chrome-style/default window, or exposes Chrome toolbar UI, record **Fail**, stop provider-account testing, and keep all controls disabled.
3. Only if installation succeeds without crossing that boundary, use a non-production provider account and vault item scoped to the local fixture URL.
4. Verify extension popup access, content-script fill, restart persistence, native-app handoff, and uninstall. Repeat in a second Switchboard profile to prove extension state and browsing storage do not cross profiles.
5. Run the fixture bridge and `app://` probes in every provider session. No profile named `CONTENT-BRIDGE-PROBE-*`, privileged UI, or unmanaged popup may appear.
6. Quit normally, confirm `OnBeforeClose` completes with no orphan helpers, restart the same session, and then uninstall the provider before moving to the next one.

Official Web Store listings:

- Bitwarden: `https://chromewebstore.google.com/detail/bitwarden-free-password-m/nngceckbapebfimnlniiiahkandclblb`
- 1Password: `https://chromewebstore.google.com/detail/1password-password-manager/aeblfdkhhhdcdjpifhhbdiojplfjncoa`
- LastPass: `https://chromewebstore.google.com/detail/lastpass-free-password-man/hdokiejnpimakedhajhdlcegeplioahd`

Installing an extension and signing into a provider are interactive external changes. Obtain explicit user approval immediately before the first installation and never request or record provider credentials.

## Required matrix

Record `Pass`, `Fail`, or `Blocked` plus a short evidence note for every cell. `Blocked` and `Not run` do not satisfy the gate.

| Capability | Bitwarden | 1Password | LastPass |
| --- | --- | --- | --- |
| Chrome Web Store install | Not run | Not run | Not run |
| Restart persistence | Not run | Not run | Not run |
| Popup access in custom chrome | Not run | Not run | Not run |
| Content-script filling | Not run | Not run | Not run |
| Uninstall | Not run | Not run | Not run |
| Profile isolation | Not run | Not run | Not run |
| Native-app handoff | Not run | Not run | Not run |
| Privileged `app://ui` bridge denial | Deterministic host coverage; provider UI verification pending | Deterministic host coverage; provider UI verification pending | Deterministic host coverage; provider UI verification pending |
| Unexpected Chrome toolbar absent | Not run | Not run | Not run |

For each run record the macOS version, Switchboard revision, exact CEF/API version, provider extension version, native-app version, QA bundle signature, session-root path, test profile IDs, and whether a step required manual verification. Do not record account identifiers, vault contents, passwords, tokens, cookies, or Keychain material.

## Decision rule

Password/extension controls may be designed only after all cells pass for all three providers under custom Switchboard chrome. Any failed, blocked, or unrun cell keeps the alpha fail-closed. A failure is documented for a later milestone; it does not authorize Chrome-style product windows, partial provider support, a custom vault, or weaker UI/content isolation.
