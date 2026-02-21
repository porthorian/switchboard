# agents.md

This document defines the roles, responsibilities, boundaries, and constraints for AI agents contributing to this project.

Project: Chromium-based Browser (CEF) — macOS-first, Rust Core

---

# 1. Purpose

Agents operating in this repository must:

* Preserve architectural integrity
* Follow the Rust-first + CEF engine model
* Respect performance constraints (50–200 tabs target)
* Avoid introducing state duplication or UI-authoritative logic
* Must read all docs in docs/*

Agents are assistants, not autonomous architects. All major architectural changes must align with the design document.

---

# 2. Core Architecture Principles

## 2.1 Engine Model

* Chromium/CEF is the rendering engine.
* Rust is the orchestration layer (state, lifecycle, persistence, policy).
* UI is rendered in CEF via `app://ui`.

Agents must NOT:

* Move canonical state into the UI layer
* Introduce business logic inside CEF-rendered UI
* Bypass the snapshot + patch system

## 2.2 State Ownership

* Rust is the single source of truth.
* UI renders derived state only.
* All state changes must originate from an Intent → Mutation → Patch flow.

Agents must not introduce:

* Direct state mutation from the UI
* UI-side authoritative ordering logic

---

# 3. Performance Constraints

The browser must remain stable and responsive with:

* 50–200 tabs
* Multiple workspaces
* Multiple profiles

Agents must preserve:

* Restore-on-click model
* Warm pool budget enforcement
* Virtualized vertical tab rendering
* Deferred CEF creation until UI frame commit

Agents must not:

* Instantiate hidden CEF instances unnecessarily
* Add background polling loops without justification
* Introduce large synchronous blocking operations on UI or main thread

---

# 4. Security Boundaries

## 4.1 Context Separation

* UI Context is privileged and isolated.
* Content Context is per-profile and unprivileged.

Agents must never:

* Expose privileged Rust APIs to content pages
* Allow navigation from content pages to `app://` schemes
* Load remote scripts into privileged UI

## 4.2 Bridge Policy

* Rust bridge must be capability-based.
* Only allow whitelisted commands.
* No arbitrary code execution pathways.

---

# 5. Persistence Rules

* SQLite is the persistence layer.
* Schema migrations must increment `schema_version`.
* All writes must occur before emitting patches.

Agents must not:

* Introduce state that is not persisted intentionally
* Add ad-hoc JSON files without architectural justification

---

# 6. Tab Lifecycle Rules

Valid states:

* Active
* Warm
* Discarded

Agents must:

* Maintain LRU warm pool limits
* Discard non-active workspace tabs first
* Prevent duplicate live CEF instances for the same tab

Agents must not:

* Break restore-on-click guarantees
* Restore all tabs in a workspace eagerly

---

# 7. Snapshot + Patch Contract

## Rust → UI

* SNAPSHOT
* PATCH

## UI → Rust

* Intents only

Agents must:

* Maintain revision integrity
* Ensure patches are minimal and deterministic
* Provide resync path if revision mismatch occurs

Agents must not:

* Stream high-frequency micro events unnecessarily
* Send UI-specific layout data from Rust

---

# 8. Agent Roles

## 8.1 Architecture Agent

* Ensures alignment with design document
* Guards separation of concerns
* Reviews lifecycle correctness

## 8.2 Performance Agent

* Evaluates memory impact
* Ensures warm pool budget compliance
* Reviews render and patch frequency

## 8.3 Security Agent

* Audits bridge exposure
* Reviews scheme access controls
* Ensures context isolation

## 8.4 Persistence Agent

* Maintains schema consistency
* Designs migrations
* Reviews transaction safety

---

# 9. Agent Restrictions (Editable Section)

This section is intentionally left for project-specific constraints and evolving limitations.

* Cannot run any commands without explicit approval
* Cannot run any git commands
* Can run readonly commands related to the filesystem, but can never exit the workspace.
* No third-party runtime dependencies without review
* No dynamic code execution features
* No telemetry collection without explicit approval
* No background network calls in UI context

---

# 10. Change Policy

Major architectural changes must:

* Update the design document
* Update this agents.md if relevant
* Include reasoning about performance, security, and lifecycle impact

Minor feature additions must:

* Preserve snapshot + patch integrity
* Respect tab lifecycle rules

---

# 11. Guiding Principle

Stability > Features
Determinism > Magic
Explicit State > Implicit Behavior

Agents should optimize for clarity, isolation, and long-term maintainability over short-term convenience.

