# Design — shortcuts restructure v2

Source design doc: vault `Obsidian23/Projects/nergal/Shortcuts restructure proposal.md` (v2, iterated with the user 2026-07-01→03). This file records the decisions and their alternatives; the vault doc holds the full CLI-audit evidence (Apéndice B) and per-OS caveats.

## D1. Leader = `Ctrl+Space` (code `Space`)

- **Chosen**: `Ctrl+Space`, locked, two discrete presses, 2000 ms timeout, `Esc`/re-tap cancels.
- **Alternatives considered**:
  - `Ctrl+G` (first candidate from chord research) — rejected: CC binds it (external editor), Codex and pi too.
  - `Ctrl+Ñ` — rejected by the user: consolidated muscle memory for Focus Terminal; stays untouched.
  - `Ctrl+{` (Quote, neighbor of Ñ, pairs with `Ctrl+}` quake) — kept as documented fallback.
- **Why Space**: easiest key to hit (thumb), vim space-leader association, most popular tmux prefix remap, `code: "Space"` identical across layouts, unbound in all 4 CLIs. Per-OS pre-flight required (macOS input-source switch, Windows CJK IME) — verified free on the dev machine (IBus/GNOME use `Super+Space`).
- **Timeout 2 s** = OpenCode leader parity (Zed's 1 s is less forgiving, and nothing competes for the key). **Which-key delay 150 ms** = which-key.nvim convention: fluent users never see it.

## D2. Passthrough semantics — explicit raw key, no implicit forwarding

**Revision of vault §1.4.** The original draft auto-forwarded any pending `Ctrl+combo` that wasn't a continuation. That has two failure modes discovered during discovery:

1. **Sloppy chording**: users often keep Ctrl held from the leader press (VS Code chord habit). Under auto-forward, `Ctrl+Space` + still-held `Ctrl+p` would send `Ctrl+P` to the PTY (CC history-prev) instead of firing Push.
2. **Misclick hazard** (raised by the user for zen-on-`z`): an accidental `Ctrl+Z`/`Ctrl+D` after the leader would suspend/EOF the agent CLI. Exactly the class of accident the restructure must eliminate.

- **Chosen**:
  - plain key → continuation;
  - `Ctrl+<continuation>` → same continuation (sloppy-chording tolerance);
  - `Ctrl+<non-continuation>` → safe no-op + which-key hint (never reaches the PTY);
  - `leader` → `.` → **raw mode**: next keystroke forwarded verbatim to the active PTY (agent session or quake shell by focus zone). Which-key advertises it as its own group ("send to terminal").
- **Alternatives**:
  - *Strict tmux send-prefix* (`Ctrl+x` always forwards): complete coverage in 2 keystrokes but reintroduces both failure modes above.
  - *Auto-forward only non-continuations*: leaves every agent combo whose letter IS a continuation (`Ctrl+P/B/O/R/T/W/E/A/N`…) unreachable — breaks the "no agent shortcut is ever inaccessible" guarantee.
- Cost of the chosen model: rare agent combos take 3 keystrokes (`leader . combo`). Acceptable — the target user runs 99 % nergal shortcuts (which is also why `keyboard_ownership` defaults to `nergal`).

## D3. Keyboard ownership switch

- Config `keyboard_ownership: "nergal" | "agent"`, default `"nergal"` (user's flow). Identical binding map in both modes; the switch only gates the `scope: "app"` entries (Zed context-tree model over the existing `focusZoneAtom`). Zellij precedent: shipping both presets instead of imposing one.
- Frontend-owned config key: plain field on `Config`, must NOT enter `BACKEND_OWNED_CONFIG_KEYS` (see [[feedback_frontend_config_stale]] — backend-owned keys get stripped on `save_config`).
- Onboarding: one step in the future first-run wizard (`setup-wizard` change, amended by this change); until that ships, Settings → Keymap is the only surface.

## D4. Digit rows — sessions vs projects

- **Chosen**: `Ctrl+1..9` = sessions (absorbing the focused-workspace preference already implemented by `numericTargetWorkspace`); `Ctrl+Shift+1..9` = jump to project N. The `focused-session-*` entries are removed; the **`Ctrl+Alt` tier dies entirely**.
- Flow: `Ctrl+Shift+3` (project) → `Ctrl+2` (session 2 there). Requires jump-to-project to leave focus in the sidebar with `focusedWorkspaceIdAtom` set (verify — it already sets the atom; the focus-zone part must be asserted in tests).
- Wins: frees 9 global bindings, removes the whole OpenCode `Ctrl+Alt` collision class, and dissolves the Windows `AltGr ≡ Ctrl+Alt` caveat (no `Ctrl+Alt` bindings remain at all).
- Alternative (keep `Ctrl+Alt+1..9`): rejected — digits were the tier's only survivor; killing the tier entirely simplifies the model and the docs.

## D5. `Ctrl+B` sidebar + `Ctrl+Shift+B` right panel (user decision)

- The v2 draft moved sidebar to `Ctrl+Shift+B` and demoted the right-panel toggle. The user vetoed: `Ctrl+Shift+B` (right panel) is high-frequency muscle memory.
- **Chosen**: `Ctrl+B` keeps Toggle Sidebar but becomes `scope: "app"`; `Ctrl+Shift+B` keeps Toggle Right Panel (global). In `nergal` mode both behave exactly as today; in `agent` mode CC recovers `Ctrl+B` (background command) while focus is in the terminal — sidebar still reachable via `Alt+←` + `Ctrl+B`, the palette, or nergal mode.
- **`Ctrl+Shift+O` = Obsidian panel** (user swap 2026-07-03, second round): `O` recovers its true mnemonic for a daily surface (Obsidian was stuck on the forced `Q`), and the **ports popover takes `leader o`** (frequent but popover-shaped — a natural leader citizen). The `Ctrl+Shift+O` shadow (pi's `shift+ctrl+o` tree-filter-cycle-backward, niche) now belongs to the Obsidian binding; `Ctrl+Shift+Q` is freed.

## D6. Zen on `leader 0`

User decision, and consistent with D2: `z` neighboring Ctrl invites `Ctrl+Z`-class misclicks; `0` carries the existing `Ctrl+Shift+0` association. `z` returns to the free pool.

## D7. Flat leader in v1; prefix trees deferred

The leader dispatcher is a generic prefix state machine, but v1 ships a **flat** namespace: git actions are `leader p / m / r` directly (NOT `leader g p`). Depth-2 groups (`leader g` → git submenu) are a documented evolution for when a family outgrows the flat level; rules already fixed: a prefix is never also an instant action, max depth 2.

## D8. `docs/shortcuts.md` — bounded maintenance contract

User asked whether recording banned combos + agent versions creates permanent debt. **Chosen**: split by rate of change.

- **Repo (`docs/shortcuts.md`)**: the *derived rules* — layer model, allocation conventions (new features are born in the leader; promotion to `Ctrl+Shift` requires daily-use evidence), the never-bind list (grouped by stable classes: terminal signals, IME/DE reservations, readline families, CLI leaders), deliberate shadows + mitigations, and a provenance stamp (audit date 2026-07-02 + CLI versions) marking it as a snapshot.
- **Vault**: the full per-CLI keymap inventories (the part that actually rots).
- **Maintenance hook**: the existing CC release-review sweep protocol gains a "new keybindings?" checklist item, making re-verification event-driven instead of a standing chore. The never-bind classes are stable across CLI releases; per-key drift only matters when adding new bindings, and `validateCombo` + the docs file are consulted at that moment.

## D9. Registry/keymap mechanics

- Chord syntax in `keys`: `"leader n"`, `"leader shift+w"` — the literal token `leader` resolves to the leader's effective binding. `comboSignature` gains a two-segment form (`leader|:KeyN`) so collision checks work per-namespace.
- Palette-only entries: `keys: ""` — dispatcher skips, palette lists without badges, keymap editor can bind later.
- `validateCombo`: leader continuations don't require Ctrl/Alt (the prefix isolates them); global namespace keeps the current rule.
- `LOCKED_SHORTCUT_IDS`: + `leader`, − `focused-session-*` (removed ids). **Lock semantics resolved (iprev R1-F6)**: the lock is UI-only for `leader` — no Rebind button, but `resolvedShortcutsAtom` honors `keymap_overrides["leader"]` (today it ignores overrides for ALL locked ids, `shortcuts.ts:839-845` — carve out `leader`). That keeps the OS-reserved escape hatch (macOS input sources / CJK IME → fallback `Ctrl+{`/`Ctrl+.`) real instead of dead config.
- **Dispatcher ordering (iprev R1-F1 — the critical one)**: *resolution* and *activation* split.
  - **Resolution first**: the pending-state handler runs immediately after the keymap-capture bail, BEFORE Tab forwarding, quake overrides, the hardcoded `Ctrl+K`, and the scratchpad hijack. Otherwise raw-mode `Ctrl+K` opens the palette instead of reaching the PTY, a bare Tab during pending gets forwarded by the Tab block (leaks + desyncs the machine), and quake-focused `Ctrl+W` during pending closes a shell. While pending, the machine owns every keydown.
  - **Activation last**: the `Ctrl+Space` match that *starts* the pending state sits with the registry loop, after the palette/dialog/zen guards — the leader must not activate under a dialog. If a dialog opens mid-pending, the pending state self-cancels.
  - The `Ctrl+K` hardcode folding into the registry loop is not a goal of this change.
- Provider-status palette entry: `StatusBar` owns the popover with local state (`openProvider`), so the palette handler dispatches a `nergal:open-provider-status` CustomEvent (same pattern as `nergal:save-file` / `nergal:toggle-annotations-drawer`).

## Risks

- **Muscle-memory breakage** is deliberate and user-approved; the which-key popover is the retraining tool.
- **Leader swallows typing** if a user triggers it accidentally mid-prompt: mitigated by the visible breadcrumb + 2 s auto-cancel + `Esc`.
- **`Ctrl+Space` reserved on some systems** (macOS input sources, CJK IME): per-OS pre-flight at implementation time + documented fallbacks. Resolved (D9): UI-locked, override honored for `leader` specifically.
- **Iframe blind spot (iprev R1-F5)**: while the browser panel's cross-origin iframe holds focus, no registry shortcut — leader included — can fire; only the Tauri-level `RESERVED_SHORTCUTS` (`browser.rs:31-43`) work there. `ctrl+shift+0 → browser:toggle-mode` stays in that set as an iframe-scoped escape hatch even though global zen moves to `leader 0` — deliberate exception, documented in `docs/shortcuts.md`, no Rust change.
- **Quake tab-management exception (iprev R1-F12)**: the hardcoded quake `Ctrl+W`/`Ctrl+Shift+T` overrides stay nergal-owned in both ownership modes (quake shells are nergal UI hosting a plain shell, not the agent); the shell's readline `Ctrl+W` is reachable via `leader .`.
- **WebKitGTK**: all matching stays on `event.code` (`Space`); no `key`-based matching anywhere new.
