# Switchboard architecture — macOS-first Rust + CEF

## Product boundary

Switchboard is a macOS-first browser with an original interface and SigmaOS-inspired productivity workflows. Chromium Embedded Framework 145 is the rendering engine. Rust owns state, lifecycle, persistence, policy, and native view orchestration. The local `app://ui` document renders derived state and emits intents.

The alpha excludes public distribution, signing/notarization, cloud sync, collaboration, AI, updater infrastructure, and non-macOS platforms. Password and general-purpose extension support remain disabled until the all-provider compatibility gate in `PASSWORD_EXTENSION_COMPATIBILITY.md` passes.

## Non-negotiable boundaries

- Rust is the single source of truth. UI collapse state and presentation details may be local; hierarchy, ordering, selection, split membership, status, and lifecycle may not.
- State changes follow intent → candidate state → complete SQLite transaction → in-memory swap → patch.
- A failed transaction changes neither memory nor revision.
- Content pages have no privileged bridge and cannot load `app://`.
- The UI loads only bundled resources, has a restrictive CSP, and uses an ephemeral request context with default cookie schemes disabled.
- Every profile owns one persistent `CefRequestContext`; workspaces never create storage boundaries.
- Inactive profiles retain no live content browsers.
- There is never more than one live browser generation for a tab.

## macOS and CEF packaging

The supported browser launch is a generated `.app`, not `cargo run`. `scripts/build_macos_dev_app.sh` creates:

- `Switchboard Dev.app` and its `Info.plist`;
- the pinned CEF framework and resources;
- dedicated main, Renderer, GPU, and Plugin helper applications;
- main/helper entitlements; and
- an inside-out ad-hoc signature verified with `codesign --deep --strict`.

Startup fails if the process is outside an app bundle or if the framework, resources, helper executable, or sandbox library is missing. `cef_settings_t.no_sandbox` is always zero. Normal development uses the real macOS Keychain; mock Keychain is accepted only by `Switchboard Smoke.app`.

Every helper initializes `libcef_sandbox.dylib` before loading the CEF framework, then destroys the sandbox context after `cef_execute_process` returns. The sandbox library and framework handles are owned separately so teardown always unloads CEF before destroying the helper sandbox.

Host-facing C ABI definitions are pinned to CEF `145.0.26` / API `14500`. Normal builds consume a checked-in, version-verified bindgen snapshot for high-risk callback and settings structures; regenerating that snapshot is an explicit developer action and is rejected for another CEF version.

## Context and scheme isolation

The UI browser uses its own ephemeral request context. Profile contexts use separate direct-child cache paths at `root_cache_path/profile-<profile_id>` with persistent cookies. The direct-child layout satisfies Chromium's profile-path restriction while retaining one storage boundary per Switchboard profile. Content browser creation receives the owning profile context explicitly. Calls into the raw CEF C API transfer an additional reference for ref-counted client and request-context parameters, preserving the host-owned persistent references across browser close and recreation.

The `app` scheme factory is registered only on the ephemeral UI request context and only for the exact `ui` domain; it is never registered globally or on a profile context. This lets the initial `app://ui` request resolve before `OnAfterCreated` while leaving content contexts without an `app` handler. The bridge separately validates the retained UI browser identity and accepts only the exact `app://ui` origin on its main frame. HTTP content navigations to `app://` are rejected by Rust, and a content client receives no handler for privileged UI messages.

## Typed bridge and revisions

All payloads use versioned `serde` envelopes with a 64 KiB input limit and a non-empty bounded request id. Unknown fields, unknown commands, malformed JSON, and unsupported protocol versions are rejected.

Rust → UI messages are:

- `snapshot { snapshot }`;
- `patch { patch }`;
- `restore_requested { tab_id, generation }`;
- `search_results { query, results }`; and
- bounded acknowledgements/errors.

UI → Rust messages are whitelisted `UiCommand` variants only. The UI receives one snapshot and ordered patches. Each patch declares contiguous `from_revision` and `to_revision`; a mismatch triggers `request_resync` and a new snapshot. There is no snapshot polling or prompt-based native bridge.

Decoded stateful UI commands are deferred to the next macOS main-queue turn. This prevents browser creation or teardown from running reentrantly inside a CEF display callback.

The current alpha transport serializes these envelopes through the privileged main-frame CEF callback. Moving the same envelope contract to CEF process-message/V8 plumbing remains a native acceptance item; the reducer and UI contract must not change when that transport is replaced.

## Domain model

### Profiles and workspaces

A profile is the browsing-data isolation boundary. A workspace belongs to exactly one profile and stores its primary tab, optional secondary tab, split-enabled state, and a divider ratio clamped to 25–75%. `active_tab_id` is a compatibility alias for the primary tab during the native-host transition.

### Tab tree

Open tabs form an arbitrary-depth tree through `parent_tab_id`; workspace `tab_order` is the authoritative depth-first order. Pinned and locked are independent. Pinned siblings precede unpinned siblings at each level, and moving a parent moves its subtree. Reducer validation rejects cross-profile moves, invalid parents, cycles, invalid sibling positions, and invalid split membership.

Observed and custom titles are stored separately. A custom title wins until cleared.

### Status

A tab is one of:

- `Open`;
- `Done { completed_at_ms, restore }`; or
- `Snoozed { wake_at_ms, restore }`.

Close and Done converge on completion. Locked tabs cannot be completed, snoozed, or permanently deleted. Completion saves the former parent and sibling index. Restore uses that location when valid and otherwise appends at the root while preserving pinned ordering.

After completion, selection is deterministic: next sibling, previous sibling, parent, first root, then none. Completed tabs are retained for 30 days and up to 500 per profile and pruned at completion/startup. Snoozed tabs use a one-shot UI timer; startup and application reactivation process overdue entries.

## Runtime lifecycle

Runtime residency is rebuilt after startup and is never persisted:

- `Active`: primary visible browser;
- `Secondary`: second visible browser;
- `Warm`: hidden live browser in the active profile LRU;
- `Discarded`: metadata only; and
- `Restoring`: visible placeholder awaiting a frame acknowledgement.

The active profile has at most two visible browsers plus eight warm browsers. Both visible tabs are excluded from the warm budget. Inactive profiles have zero live browsers.

Activating a discarded tab follows this sequence:

1. The reducer selects it and marks it `Restoring`.
2. The committed patch is sent to the UI.
3. Rust emits `restore_requested(tab_id, generation)`.
4. The UI renders and responds from its next animation frame.
5. Rust accepts only the matching pending generation and creates the native view.
6. Successful creation commits `Active` or `Secondary` and emits the next patch.

Switching away removes the pending token. Stale acknowledgements and CEF callbacks are no-ops. Navigating an existing tab calls the existing main frame's `load_url`; it never recreates the browser.

Every UI and content client installs a life-span handler. Window close requests `CloseBrowser` for every live browser and keeps the CEF loop alive until the final `OnBeforeClose`; client-owned handlers are not released before that callback. Discarding a tab also requests an explicit close and retires its client until close completion. If the same tab becomes live again while its prior browser is closing, restoration remains pending; `OnBeforeClose` emits close completion and the UI receives a fresh restore request before CEF creates the replacement browser.

Content loading state and Chromium back/forward capability come only from `CefLoadHandler::OnLoadingStateChange`; display progress callbacks never query browser lifecycle methods. Main-frame `OnLoadEnd` emits the successful-load signal used for history, while `OnLoadError` emits bounded diagnostics and suppresses history. Retained browser wrappers are attributed to `(tab_id, browser_generation)` and may be refreshed only when CEF confirms they represent the same underlying browser.

## Persistence schema v2

`rusqlite` uses the system SQLite library. Schema v2 contains normalized `meta`, `profiles`, `workspaces`, `tabs`, `settings`, and `history` tables with foreign keys and transactional replacement.

Persisted fields are domain state and the committed revision. Loading flags, native handles, generations, warm order, and runtime residency are reconstructed. If a database is not schema v2, it and any WAL/SHM companions are renamed to a timestamped versioned backup before an empty v2 database is created. Nothing is silently deleted.

History is per profile. A successful HTTP(S) main-frame completion records a visit. Consecutive normalized URLs coalesce while increasing visit count and updating last-visit time. History has no automatic retention and is removed only by clear-current-profile or clear-all.

## Lazy Search

New Tab opens Lazy Search and creates no state until selection. Rust queries and ranks open tabs across all profiles/workspaces, whitelisted commands, active-profile history, and a configurable web-search fallback.

`>` restricts commands, `history:` restricts history, and `done:` searches completed tabs. Archived tabs are otherwise hidden. Ranking is exact, then prefix, then substring; frequency, recency, and title provide deterministic tie-breakers. Selecting an existing tab activates its profile/workspace/tab. URL and history selections create a tab only when selected.

## Split, focus, and keyboard modes

Two existing tabs from one workspace can be visible. The secondary remains in the normal tree. Clearing split preserves secondary selection but returns the hidden browser to ordinary warm-pool policy. Completing or snoozing the secondary clears it and disables split. Rust persists selection and ratio; native layout clamps the ratio.

CEF popup dispositions are intercepted so unmanaged Chrome windows never appear. Command-click/new-tab dispositions create a subtab beneath the source. Shift-click/new-window dispositions create that subtab and assign it to the secondary pane. The callback carries the source generation, and obsolete generations are ignored by the runtime.

Focus mode is session-only. It moves the UI view behind content and lays content across the full native root without changing tab state.

Browser and Insert are explicit modes with a persistent UI indicator. Switching profile/workspace/tab resets Browser mode. Browser mode owns single-key actions; `I` enters Insert and `Escape` returns to Browser, including when focus is inside content. Command-based shortcuts remain active in both modes. Setting validation rejects duplicate bindings.

## UI performance

The UI flattens only expanded tree nodes and virtualizes that visible list. Expansion, hover, scroll, drag affordances, panels, and presentation layout stay local. A drop produces one authoritative move intent. Rust does not stream layout micro-events or expose the complete history database.

## Security and release gates

Deterministic tests cover reducer invariants, SQLite reset/rollback/revision behavior, bridge decoding and resync, generation attribution, deferred creation, browser reuse, warm limits, shutdown requests, and the 200-tab workload.

The environment-dependent native gate in `MACOS_CEF_SMOKE.md` must pass before calling the alpha complete. The password provider matrix is a separate manual/instrumented gate. A passing unit suite or a generated bundle is not proof that either native gate passed.
