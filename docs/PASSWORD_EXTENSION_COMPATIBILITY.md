# Password-extension compatibility gate

## Alpha decision

Status: **deferred; no password or extension controls ship in this alpha**.

Switchboard uses custom Alloy-style chrome. The gate does not permit a partial provider rollout, a duplicate Chrome toolbar, weaker UI/content isolation, or a custom password vault. Until an instrumented CEF 145 bundle proves the entire matrix for all three providers, the product fails closed and exposes no password-manager controls.

Development builds use the real macOS Keychain. `--use-mock-keychain` is accepted only when the executable is inside `Switchboard Smoke.app`.

## Required matrix

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

## Gate procedure

Use non-production accounts in a dedicated smoke bundle. Test each row in a fresh profile and again after restart. Confirm cookies, local storage, service workers, cache, extension state, and native-app handoff do not cross profile boundaries. Inspect both popup and content-script behavior. Attempt `app://` navigation and every privileged command from a content page and require denial.

Any failed row keeps all provider controls disabled. A passing matrix must record the exact CEF API/hash, macOS version, extension versions, bundle signature, and whether any provider step required manual verification.
