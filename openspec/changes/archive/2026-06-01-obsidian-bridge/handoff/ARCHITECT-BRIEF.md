# ARCHITECT-BRIEF — obsidian-bridge M3 (Passive Growth)

**Status:** GATED — NOT STARTED. M3 must not begin until M2 (#7 + #I) adjustments are finalized (user is testing). This brief readies M3 at the execute gate.

**Project mission (goal ancestry):** Nergal — Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. Nergal runs *around* the agent, augmenting the loop — it does not reimplement agent primitives.

**Tier:** L · **Risk:** medium (app-startup refactor + detached process + user vault writes; no schema/auth/billing).

---

## Build contract

### Qué construyo
M3 = passive growth via session lifecycle. Three features over one infra piece:

1. **post-session-runner** (infra, new capability) — `cluihud post-session` detached subcommand. Global advisory lock `~/.config/cluihud/post-session.lock` (`fs2::try_lock_exclusive`), drains markers from `~/.config/cluihud/pending-mocs/<sid>.json`, builds MOC + propagates backlinks per marker, deletes marker on success. Recovery scan on launch for markers >10min. Rotating log (5MB, 3 gens).
2. **#2 — Continuous session log** (`obsidian-session-log`) — in-process `O_APPEND` to the per-workspace `session_log` channel on every relevant hook event. Crash-safe by construction.
3. **#11 — MOC snapshot** (`obsidian-session-moc`) — per-session snapshot built by the runner: frontmatter + Activity timeline + Files touched + Decisions + Links, from log block + DB state + `git diff --stat`.
4. **N1 — Reverse backlinks** (`obsidian-session-moc` cont.) — runner walks MOC wikilinks, updates `<!-- nergal-backlinks-start/end -->` region in each target vault note. Gated by `backlinks_enabled`.

### Phasing (3 commits, ascending blast radius)
- **Phase A** — infra skeleton + #2 session log (in-process, testable in isolation). Tasks 4.2.1-4.2.5 + 4.1.1-4.1.5 (MOC/backlink stubbed in `run()`).
- **Phase B** — #11 MocBuilder + N1 BacklinkUpdater + wire into `run()`. Tasks 4.3.x + 4.4.x + 4.1.2 + 4.1.10.
- **Phase C** — lifecycle triggers + recovery (riskiest: `.run` → `.build().run(cb)` for CloseRequested). Tasks 4.1.6-4.1.9.

### Verified anchors (do not trust tasks.md line numbers — these are re-confirmed 2026-05-29)
| Concern | Real location |
|---|---|
| CloseRequested refactor | `lib.rs:588` `.run(generate_context!())` → `.build(...)?.run(\|app, event\| {...})` |
| Recovery scan | `lib.rs:232`, right after `reconcile_worktrees` |
| `pub fn run()` entry | `lib.rs:182` |
| SessionEnd (end_session + marker + spawn) | `server.rs:384` (process_event arm) |
| SessionStart (log header) | `server.rs:364` (process_event arm) |
| delete_workspace marker hook | `commands.rs:668` |
| delete_session marker hook | `commands.rs:789` |
| infra deps | `Cargo.toml:92` `fs2 = "0.4"`, `:98` `libc = "0.2"` (present) |
| CLI subcommand enum | `main.rs` `Commands` (currently `Hook { action }`) |

### Cómo verifico
- Per phase: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`
- Rust unit tests: MocBuilder (empty block / N tool calls / annotations / no git diff) + BacklinkUpdater (new note / existing region / 50-entry rotation / target outside vault / nonexistent target). Tasks 4.3.3 + 4.4.3.
- Manual M3 walk (task 6.4): set `session_log_path` + `moc_path`, run a session with tool calls, end it, verify log populated + MOC generated. Close Nergal mid-flight → verify bg process finishes the MOC.
- Reinstall walk (task 6.5): `pnpm tauri build && sudo dpkg -i ...Nergal_*.deb`, repeat — verifies the bundled CLI gained `post-session`.

### Criterio de done
- All `post-session-runner` spec scenarios pass (first-runner / concurrent-lock / app-close markers / vault-less skip / detached survives exit / stale-marker recovery / failure surfaces toast).
- #2: every relevant hook event appends one line; SIGKILL preserves history up to last event.
- #11: MOC idempotent (same slug overwrites); reflects real DB state + git stats.
- N1: backlink region created/updated idempotently; targets outside vault skipped; 50-entry cap rolls into `<details>`.
- `openspec validate obsidian-bridge` clean (task 6.6).

### Estimated scope
- files_estimate: 8
- risk_tier: medium
- tags: [feature]
- visibility: public (ships in the bundled CLI)
- spec_target: post-session-runner, obsidian-session-log, obsidian-session-moc

---

## Closed decision
- **4.3.2 agent status snapshot** → **persist-on-SessionEnd**, no dedicated migration 009. The latest hook event already passes through the dispatcher; persist it to DB on SessionEnd and have MocBuilder read it from there.

## Must-handle during execute (not optional)
1. **Spec/tasks divergence** — `post-session-runner/spec.md:30` lists a **5th** marker trigger: *PTY-side session termination (worker observes EOF on PTY)*. `tasks.md §4.1.6-4.1.8` only wires 4 (SessionEnd, CloseRequested, delete_session, delete_workspace). Reconcile in Phase C: either add the PTY-EOF trigger or amend the spec. Spec is source of truth → default to implementing the 5th trigger unless user de-scopes it.
2. **Risk #1 (hardened-distro spawn failure)** — implement the **PID check 200ms post-spawn**; on failure, fall back to synchronous flush on close + Sileo toast "bg processing disabled". Difference between M3 working and failing silently.
3. **`.run` → `.build().run(cb)`** — isolate in Phase C, confirm cold start still boots *before* adding callback logic. Highest mechanical-break risk in the milestone.
4. **Multi-instance** — runner uses global lock + `pending-mocs/`. Manual E2E requires closing the installed `.deb` (shared `/tmp` + `~/.config/cluihud/` resources; see session note on socket/DB collision).

## Gate / next action
- iterative-plan-review **RECOMMENDED-PENDING** (files_estimate ≥ 5 triggers it). Held: user reviews this brief first; offer a cross-model run on the bg-runner architecture before execute.
- **Do not spawn Builder.** Resume at Phase A once: (a) M2 adjustments finalized + user OK, and (b) iprev decision made.
