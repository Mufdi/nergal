# Design — setup-wizard

## Context

Nergal's value comes from augmenting the agent loop via the hook pipeline. The registration that wires that pipeline (`setup::run()` in `src-tauri/src/setup.rs:150`, exposed as the `nergal hook setup` CLI action and the `setup_hooks` Tauri command at `src-tauri/src/commands.rs:606`, registered in `src-tauri/src/lib.rs:432`) is **never invoked** — no frontend call, no startup hook, no docs. A fresh install therefore has dead panels until the user discovers a hidden CLI command. There is also no surface reporting the other hard precondition: at least one agent CLI on PATH (`agents::registry::AgentRegistry::scan` → `DetectionResult.installed`, `src-tauri/src/agents/registry.rs:56` / `src-tauri/src/agents/mod.rs:274`). The decisions below were crystallized with the user (2026-06-29): **first-run + health-gate** trigger, **one-click consented** hooks config, **skippable / non-blocking**.

## Goals / Non-Goals

**Goals:**
- A fresh install reaches a working state (live panels, an agent available) without any hidden CLI step.
- One readiness probe as the single source of truth for both the wizard and the health-gate.
- Out-of-the-box: detect + one-click fix / suggestions, in the app's existing visual style.
- Re-surface critical regressions (hooks removed / no agent) non-blockingly after first run.
- Reuse existing primitives (`setup_hooks`, agent registry, `resolve_pty_shell`, `installSource`); reimplement nothing.

**Non-Goals:**
- Not a general preferences editor — Settings already owns that; the wizard links to it.
- Not installing agents for the user (we detect + guide; we do not download Claude Code).
- No telemetry, no network calls, no SQLite schema change.
- Not blocking app use — the wizard is always skippable.

## Decisions

### D1 — Trigger: first-run flag + persistent health-gate
**Chosen:** a persisted `onboarding_completed: bool` (default `false`) drives the **full wizard** on first launch; after that, a **non-blocking status-bar indicator** re-opens the wizard when a *critical* check regresses.
- *Alternatives:* (a) first-run-only — no recovery if hooks are later wiped; (b) banner-only, no guided wizard — less hand-holding for the fresh-install case that motivated this. The flag is what distinguishes "never onboarded" (show full wizard) from "onboarded, but a check regressed" (show banner) — both modes need it.

### D2 — Onboarding flag lives on `Config`
**Chosen:** add `onboarding_completed` to the Rust `Config` struct (`config.rs:115`), persisted through the existing config file. 
- *Alternative:* a sentinel file (like `~/.nergal-active`) — rejected: config is already the persistence layer and is mirrored to the frontend, so the wizard reads/writes it through the existing path.
- *Trade-off / gotcha:* the frontend `configAtom` carries a stale copy and `save_config` drops backend-owned keys (`BACKEND_OWNED_CONFIG_KEYS`, `commands.rs`). The flag must be writable from the frontend (the wizard sets it on completion), so it is a **frontend-writable** config key (NOT added to `BACKEND_OWNED_CONFIG_KEYS`) — set it via the normal config save path. See `[[feedback_frontend_config_stale]]`.

### D3 — Single `setup_status` probe command
**Chosen:** one Tauri command returns the whole readiness snapshot (`hooks_registered`, `agent_detected` + per-agent list, `default_agent`, `shell`, `transcripts_dir`).
- *Alternative:* N separate calls (is-hooks, list-agents, get-shell) — rejected: the wizard and the health-gate both want an atomic snapshot; one command avoids frontend orchestration and race between partial reads. Reuses `setup`'s settings-path probe, the agent registry scan, and `resolve_pty_shell`.

### D4 — Hooks config: one-click consented
**Chosen:** detect state; when missing, show a prominent "Configure" button that calls `setup_hooks` (idempotent, `setup::run()`), then re-probe.
- *Alternatives:* (a) fully automatic silent registration on first launch — rejected: it mutates the user's **global** `~/.claude/settings.json` without consent; (b) instructions-only — rejected: keeps the very friction we are removing. One-click keeps it out-of-the-box while showing what is touched.

### D5 — Critical vs informational checks
**Chosen:** only `hooks_registered` + `agent_detected` gate the "all set" state and drive the health-gate banner. Shell, transcripts, default-agent are suggestions that never block.
- *Rationale:* those two are the actual preconditions for the loop; the rest are quality-of-life and already have sensible platform-aware defaults (`resolve_pty_shell`, `~/.claude/projects`).

### D6 — Health-gate in the status-bar surface
**Chosen:** a non-blocking indicator in the existing status-bar surface, one click re-opens the wizard.
- *Alternatives:* a recurring modal nag (intrusive) or a transient toast (missable). A persistent, quiet status-bar affordance matches the app's existing indicator conventions and respects "non-blocking". *(Exact component + insertion point pinned in `implementation.md`.)*

### D7 — Wizard UI reuses the app's overlay pattern
**Chosen:** render the wizard with the app's existing modal/overlay mechanism (atom-driven overlay, matching the command palette / ask-user dialog conventions) rather than a bespoke full-screen route, so it inherits the design system (R0162 tokens, focus zones, keyboard nav). *(Exact pattern + mount point pinned in `implementation.md` from the codebase map.)*
- *Alternative:* a dedicated route/page — rejected: heavier, diverges from existing overlay conventions, and the wizard is transient.

### D8 — Existing-install migration nicety
**Chosen:** on first load after upgrade, if hooks are already registered, seed `onboarding_completed = true` so existing users do not get the wizard popped at them. New/fresh installs (no hooks) keep `false` → wizard opens.
- *Trade-off:* a one-time check at load; avoids a surprise overlay for every current user.

## Risks / Trade-offs

- **Mutating `~/.claude/settings.json`** (global CC config): mitigated by reusing the existing idempotent `setup::run()` (it merges, removes only obsolete Nergal hooks, never clobbers unrelated keys), gated behind explicit consent (D4), and reversible.
- **`configAtom` staleness** (`[[feedback_frontend_config_stale]]`): the new flag is frontend-writable; ensure the wizard writes through the normal `save_config` path and that the read-back reflects it. Do **not** add it to `BACKEND_OWNED_CONFIG_KEYS`.
- **Existing installs seeing the wizard once**: mitigated by D8 (seed `true` when hooks already present). If D8 is skipped, the fallback is benign — their checks pass and they skip.
- **Agent detection timing**: the agent store is populated asynchronously via `agents:detected`; the wizard must reflect the probe result and update on that event rather than reading a possibly-empty initial snapshot (reuse the existing store, do not re-implement detection).
- **Scope creep into a preferences editor**: bounded by D5/Non-Goals — the wizard shows suggestions and links to Settings; it does not become a second settings surface.
