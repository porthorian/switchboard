# Chromium-based Browser (CEF) — macOS-first Rust Design Doc

## 1. Overview

Build a fast, macOS-first desktop browser using **CEF (Chromium Embedded Framework)** as the web engine, with:

* **Rust** as the primary “browser brain” (state, lifecycle, persistence, policies)
* **HTML/CSS/JS** for the browser chrome UI rendered inside CEF (`app://ui`)
* Support for **Profiles** (hard storage isolation) and **Workspaces** (tab organization) with **vertical tabs**
* Scalability target: **50–200 tabs** across workspaces with stable memory via **restore-on-click**

Key principle:

> Chromium/CEF is the engine. Rust is the orchestration layer.

## 2. Non-Goals (MVP)

Not in MVP:

* Extensions
* Sync across devices
* Full password manager
* Adblock engine
* Wayland/Linux support, Windows support

## 3. Platform Scope

* **macOS only** for v1.
* CEF multiprocess model is used as-is (renderer/GPU/utility subprocesses managed by CEF).

## 4. High-Level Architecture

### 4.1 Processes

* **Main App Process (Rust-owned “browser process”)**

  * Owns canonical state (profiles/workspaces/tabs)
  * Owns persistence (SQLite)
  * Owns policies (discarding, permissions)
  * Hosts and manages CEF views (UI + content)
  * Integrates CEF message loop with macOS run loop

* **CEF Subprocesses**

  * Renderer/GPU/utility processes spawned by CEF
  * Packaged inside the app bundle

### 4.2 Views (macOS)

Window layout:

* **UI View**: a dedicated CEF browser instance using **UI Context** rendering `app://ui`
* **Content Container**: a native container view hosting the active tab’s CEF view

Conceptually:

* `NSWindow`

  * Root view

    * UI CEF view (`app://ui`) — vertical tabs, omnibox, workspace/profile selectors
    * Content container — active content webview

## 5. Security & Context Separation

We operate with **separated contexts**:

### 5.1 UI Context (Privileged)

* Only loads `app://...` internal pages
* Minimal/no persistent cookies
* Has access to Rust bridge (strictly limited API)

### 5.2 Content Context (Unprivileged)

* One **Content Context per Profile** (storage isolation boundary)
* Persistent cookies/cache/storage per profile
* No privileged Rust bridge

### 5.3 Hard Rules

* Content tabs **cannot navigate** to `app://...` (block/redirect)
* Rust bridge enabled **only** for trusted UI frames under `app://ui/*`
* UI assets bundled locally (no remote CDN dependencies in MVP)

## 6. Product Model: Profiles & Workspaces

### 6.1 Profiles (Isolation)

* Profiles represent hard storage isolation:

  * cookies
  * cache
  * site storage
  * permissions (later)

### 6.2 Workspaces (Organization)

* Workspaces are organizational collections of tabs **within a profile**.
* Workspaces do **not** create separate cookie jars.

Outcome:

* **Profiles isolate**.
* **Workspaces organize**.

## 7. State Management: Snapshots + Patches

### 7.1 Source of Truth

* Rust holds canonical state.
* UI renders canonical state.

### 7.2 Revisions

* Rust maintains a monotonically increasing `revision`.
* UI stores `current_revision`.

### 7.3 Messages

Rust → UI:

* `SNAPSHOT { state, revision }`
* `PATCH { ops[], from_revision, to_revision }`

UI → Rust:

* Intents (commands):

  * `UI_READY { ui_version }`
  * `NAVIGATE { tab_id, url }`
  * `NEW_TAB { workspace_id, url?, make_active }`
  * `CLOSE_TAB { tab_id }`
  * `ACTIVATE_TAB { tab_id }`
  * `MOVE_TAB { tab_id, workspace_id, index }`
  * `NEW_WORKSPACE { profile_id, name }`
  * `RENAME_WORKSPACE { workspace_id, name }`
  * `SWITCH_WORKSPACE { workspace_id }`
  * `SWITCH_PROFILE { profile_id }`
  * `PIN_TAB { tab_id, pinned }`
  * `DISCARD_TAB { tab_id }`
  * `SETTING_SET { key, value }`

Robustness:

* If the UI gets out of sync, it requests a resync and Rust sends a full `SNAPSHOT`.

## 8. UI Performance: Virtualized Vertical Tabs

* UI computes virtualization locally (no “visible rows” from Rust).
* Virtual list requirements:

  * Fixed row height (MVP)
  * Minimal DOM per row (favicon, title, close button, small status)
  * Local-only hover/scroll/drag state

Drag/drop:

* UI provides the drag experience locally.
* On drop, UI sends a single `MOVE_TAB` intent.
* Rust responds with authoritative ordering patch.

Search:

* UI maintains a local search index (title + URL) updated on patches.

## 9. Tab Lifecycle: Restore-on-Click

### 9.1 States

Each tab is in exactly one runtime state:

* **Active**: live CEF instance, visible
* **Warm**: live CEF instance, hidden
* **Discarded**: no CEF instance, metadata only (+ optional thumbnail)

### 9.2 Budgets (macOS MVP)

Per active profile:

* Active: 1
* Warm pool: 5–8 total (LRU)
* Discarded: everything else

Warm pool is **profile-scoped** and **global within the profile**, not per workspace.

### 9.3 Workspace Switching

* Instant sidebar switch.
* Content shows last active tab for that workspace:

  * Warm → instant
  * Discarded → thumbnail placeholder + restore

No bulk wake-up of workspace tabs.

### 9.4 Profile Switching

* UI stays constant (UI Context unchanged).
* Content swaps to the target profile’s active tab.
* Optionally shrink/discard warm tabs in the previous profile.

## 10. Deferred Creation Until UI Frame Commit

We defer expensive work for smooth UI.

Flow when activating a discarded tab:

1. UI sends `ACTIVATE_TAB { tab_id }`.
2. Rust updates state immediately:

   * sets active pointers
   * marks tab as `restoring` (sub-state)
   * emits `PATCH` so UI shows thumbnail + spinner
3. UI applies patch and renders the placeholder.
4. UI sends `FRAME_COMMITTED { revision }`.
5. Rust creates/attaches the CEF content view and starts navigation.
6. Rust patches tab to `Active` when ready.

Cancellation:

* If user activates another tab before commit, pending restore is canceled.

## 11. Thumbnails (Perceived Speed)

* Capture thumbnail on transitions:

  * Active → Warm
  * Active → Discarded
* For discarded tab activation:

  * show thumbnail immediately
  * replace with live view when ready
* Maintain storage cap and LRU cleanup.

## 12. Persistence (SQLite)

### 12.1 Goals

* Crash-safe persistence
* Fast startup
* Easy migrations

### 12.2 Schema (Conceptual)

**meta**

* `key` (PK): `schema_version`, `last_revision`, etc.
* `value`

**profiles**

* `id` (PK)
* `name`
* `created_at`
* `last_active_at`
* `content_data_dir` (or derived)
* `active_workspace_id` (nullable)

**workspaces**

* `id` (PK)
* `profile_id` (FK)
* `name`
* `sort_index`
* `created_at`
* `last_active_at`
* `active_tab_id` (nullable)

**tabs** (metadata only)

* `id` (PK)
* `profile_id` (FK)
* `workspace_id` (FK)
* `url`
* `title`
* `favicon_url` (or favicon key)
* `pinned` (bool)
* `muted` (bool)
* `created_at`
* `last_active_at`

**workspace_tabs** (ordering)

* `workspace_id` (FK)
* `tab_id` (FK)
* `sort_index` (int)
* PK: (`workspace_id`, `tab_id`)
* Index: (`workspace_id`, `sort_index`)

**thumbnails** (optional; prefer file-based storage)

* `id` (PK)
* `tab_id` (FK)
* `mime`
* `width`, `height`
* `file_path` (recommended) or `bytes` (BLOB)
* `created_at`
* `last_used_at`

### 12.3 Runtime vs Persistent

Persisted:

* profiles/workspaces
* tab metadata
* tab ordering
* last active pointers

Runtime-only (in-memory):

* warm LRU state
* restoring queue
* live browser instance map (tab_id → view handle)
* loading/audio/canGoBack flags

### 12.4 Write Ordering

For each intent:

1. Apply mutation in Rust
2. Commit SQLite transaction
3. Emit patch

## 13. Startup / Restore Flow (Fast)

1. Load minimal state from SQLite:

   * profiles
   * active profile
   * workspaces for active profile
   * active workspace
   * tab ordering + metadata for active workspace
2. Emit `SNAPSHOT` immediately so UI draws sidebar fast.
3. Instantiate only the active tab’s content view.
4. All other tabs start as Discarded at runtime.

## 14. Minimal CEF→Rust→UI Event Surface

CEF events feeding state updates:

* title changed
* url changed
* favicon changed (optional early)
* loading started/stopped
* audio playing/muted (later)

Rust converts these into state mutations and emits patches.

## 15. MVP Milestones

### Milestone 1: UI shell + one content tab

* macOS window
* UI CEF view loads `app://ui`
* UI → Rust intents wired
* Single content view created on navigation

### Milestone 2: Workspaces + vertical tabs (virtualized)

* Workspaces CRUD
* Tab list rendering for active workspace
* Omnibox + basic navigation

### Milestone 3: Multi-tab + show/hide views

* Create/close/activate tabs
* Patch-driven title/loading updates

### Milestone 4: Restore-on-click + warm pool

* Warm LRU budget
* Discarded tabs as metadata only
* Deferred creation on `FRAME_COMMITTED`

### Milestone 5: Profiles

* One content context per profile
* Profile switching

### Milestone 6: Thumbnails

* capture + display placeholders
* storage cap + cleanup

## 16. Open Questions (Later)

* Tab groups
* Workspace templates / cloning
* Permissions UX and policies
* Crash recovery beyond last committed DB transaction
* Update mechanism and signing/notarization
* Detailed memory pressure signals and heuristics

