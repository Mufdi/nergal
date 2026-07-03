# Architect brief — shortcuts-restructure

**Project mission**: Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. Nergal runs *around* the agent CLI, never in its place.

**Context**: full redesign of the global keymap. The window-capture dispatcher steals every bound combo from the agent CLI; the four installed CLIs occupy nearly all bare `Ctrl+letter` and OpenCode owns `Ctrl+Alt+letter`. Design source of truth: vault doc `Obsidian23/Projects/nergal/Shortcuts restructure proposal.md` (v2, user-iterated). This change is the openspec landing of that design plus the user's discovery decisions of 2026-07-03 (see `.work-modules.json.interview_answers`).

**Control metadata**: tier L · ceremony deep · risk medium (no auth/schema; the failure class that matters is swallowing keystrokes meant for the PTY) · ~12+ files, frontend-only + one Rust config field · spec targets: `keyboard-shortcuts` (major delta + stale re-baseline), `command-palette` (badges).

**Dependencies / blockers**:
- Amends `openspec/changes/setup-wizard/` (ownership onboarding step) — that change is documented, not implemented; no runtime dependency either way.
- `keymap_overrides` migration must land in the same release as the remap (stale overrides reference removed ids).
- Per-OS leader pre-flight (`Ctrl+Space` vs macOS input-source switch / Windows CJK IME) is an implementation-time check, documented in `docs/shortcuts.md`.

**Gating decision**: Mode A only — artifacts landed, `openspec validate` green, implementation deferred to a future `/work shortcuts-restructure` (Mode B). At Mode B: builder sonnet, single-reviewer sequential (code-quality), gates = verify (tsc + cargo check + unit tests) + manual walks in tasks §7.
