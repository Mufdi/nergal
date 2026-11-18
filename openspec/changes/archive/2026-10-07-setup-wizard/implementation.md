# Implementation Plan: setup-wizard

> Re-grounded 2026-10-05 after re-validation and plan review round 1. The 2026-06-29 plan (multi-step wizard) is superseded; see `design.md`.

## Verified codebase facts (do not re-assume)

**Backend (`src-tauri/src/`):**
- `setup::run()` — `setup.rs:155`. Steps, in order: `settings_path()` → `load_settings()` → take/insert the `hooks` object (bails if not an object) → `remove_hook()` for each `OBSOLETE_HOOKS` entry → `detect_wrapper_prefix()` → `merge_hook()` for each `HOOKS` entry → `merge_statusline()` on the root map. Early-returns `Ok(())` when nothing was added, removed or inserted; otherwise `backup_settings()` + `save_settings()`. Prints a report to stdout (CLI-oriented).
- `merge_hook(...) -> bool` — `setup.rs:364`. Returns `true` when it **adds** an entry **or upgrades the matcher in place** of an existing one (`setup.rs:389-404`); `run()` counts both as `added`. Returns `false` when the event value is not an array (`setup.rs:373-375`) — the silent skip that D7 turns into an error.
- `OBSOLETE_HOOKS` — `setup.rs:94`. `remove_hook(&mut Map, &ObsoleteHook) -> bool` — `setup.rs:330`. `detect_wrapper_prefix(&Map) -> Option<String>` — `setup.rs:262`. `merge_statusline(&mut Map) -> bool` — `setup.rs:447` (true only when the key was absent; never replaces a user value).
- `platform_command` — `setup.rs:305`. On Unix it returns the bare `nergal hook …` string (`setup.rs:313-316`); the doc comment states the `.deb`/`.rpm` put `nergal` on `PATH`.
- `settings_path() -> Result<PathBuf>` — `setup.rs:461` (`~/.claude/settings.json` via `dirs::home_dir()`).
- `load_settings(&PathBuf)` — `setup.rs:466`. **On `NotFound` it runs `create_dir_all(parent)`** (`setup.rs:476-480`). The probe must not call it (design D3). `backup_settings` (`setup.rs:486`) and `crate::atomic_write::write_atomic` (also creates the parent dir, `atomic_write.rs:30`) are the other disk writers; the probe calls neither.
- Test module — `setup.rs:501`, already covers `matches_hook_command`, the wrapper-form `merge_hook` cases (`setup.rs:597`, `:636`) and `merge_statusline` (`:822`, `:838`, `:853`).
- `setup_hooks` Tauri command — `commands/workspaces_sessions.rs:87`; maps errors with `e.to_string()` (`:88`), which keeps only the outermost `anyhow` context. Registered at `lib.rs:485` under `// Setup command`.
- The only other caller of `setup::run()` is the CLI — `main.rs:117`. No frontend caller exists.
- `which = "6"` is already a dependency (`Cargo.toml:69`), used e.g. at `agents/claude_code/adapter.rs:133`.
- The app augments its `PATH` from the user's interactive shell at startup on Linux (`augment_path_from_user_shell`, `lib.rs:198`). PTY children (Claude Code) inherit it, so `which::which("nergal")` in the app sees what the hooks will see.
- Legacy `cluihud hook` entries are migrated synchronously before the window exists (`migrate_legacy::run()`, `lib.rs:280`); the probe never sees them.

**Frontend (`src/`):**
- Layout analog: `ProjectBootstrapPrompt` — `components/session/ProjectBootstrapPrompt.tsx`: `Dialog`/`DialogContent`/`DialogHeader`/`DialogTitle`/`DialogDescription`/`DialogFooter`; footer "Skip" `Button variant="secondary"` + `<Kbd keys="esc">`, primary + `<Kbd keys="enter" tone="onPrimary">` (`:138-145`); `busy` blocks close (`:41-44`); toasts via `toastsAtom` (`@/stores/toast`); `invoke` from `@/lib/tauri`.
- Keyboard analog: `ConfirmHost` — `components/ui/ConfirmHost.tsx`. Focuses the popup in a `requestAnimationFrame` (`:40-49`); `DialogContent showCloseButton={false} tabIndex={-1}` with `onKeyDownCapture` that catches Enter (`preventDefault` + `stopPropagation`) (`:62-73`); both footer buttons carry `focus-visible:ring-0 focus-visible:border-transparent` (BUG-25, `:90-100`).
- Mount point: `src/App.tsx:231`, `<WorktreeGate />` inside `ErrorBoundary`.
- `src/main.tsx:21` wraps the app in `StrictMode`: mount effects run twice in dev.
- vitest runs `src/**/*.test.ts` with no DOM environment (`vitest.config.ts:13`).
- UI copy in this app is English (e.g. toasts in `App.tsx:139-142`).

## Execution order

1. `setup.rs`: characterization tests, extract `apply_setup`, malformed-value error, probe + tests.
2. `commands/workspaces_sessions.rs` + `lib.rs`: `setup_hooks_status`; full error chain in `setup_hooks`.
3. `src/lib/types.ts` + `src/lib/hooksSetup.ts` (+ test).
4. `src/components/setup/HooksSetupDialog.tsx` + mount in `App.tsx`.
5. Full check + manual walk.

## Per-section plan

### 1. `setup.rs`
- Extract from `run()` a pure `apply_setup(settings: &mut Map<String, Value>) -> Result<SetupChanges>` holding every step between loading and saving. `SetupChanges { removed: Vec<&'static str>, added: Vec<&'static str>, skipped: Vec<&'static str>, statusline_added: bool }` with `is_empty()` (no added, no removed, no statusline).
- In `apply_setup`, before merging, return an error naming the event when a key Nergal manages (`HOOKS[*].event` or `OBSOLETE_HOOKS[*].event`) holds a non-array value (D7, Revision 1). Other keys are never inspected.
- Extract `run_at(path: &Path) -> Result<()>`; `run()` = `run_at(&settings_path()?)`. Output and early-return unchanged.
- Factor the parse step (string → `Map`, root-not-object bail) into a helper shared by `load_settings` and the probe.
- `#[derive(Serialize)] pub struct HooksStatus { pub registered: bool, pub settings_path: String, pub binary_on_path: bool }`.
- `hooks_status_at(path: &Path) -> Result<HooksStatus>`: read without side effects (`NotFound` → empty map, no `create_dir_all`) → parse → clone → `apply_setup` → `registered = is_empty()`; `binary_on_path` via a pure `binary_usable(resolved: Option<&Path>, appdir: Option<&Path>) -> bool` (true when resolved exists and is not under appdir), fed by `which::which("nergal").ok()` and `std::env::var_os("APPDIR")`; `#[cfg(windows)]` always true (design D6, Revision 1).
- `pub fn hooks_status() -> Result<HooksStatus>` = `hooks_status_at(&settings_path()?)`.

### 2. Commands
- `#[tauri::command] pub fn setup_hooks_status() -> Result<setup::HooksStatus, String>` next to `setup_hooks` (`commands/workspaces_sessions.rs:87`), errors as `format!("{e:#}")`.
- Change `setup_hooks` to map errors with `format!("{e:#}")` so the dialog shows the OS reason.
- Register `commands::setup_hooks_status,` after `commands::setup_hooks,` at `lib.rs:485`.

### 3. Types + pure helper
- `export interface HooksStatus { registered: boolean; settings_path: string; binary_on_path: boolean; }` in `src/lib/types.ts`.
- `src/lib/hooksSetup.ts`: `shouldOpenHooksDialog(status: HooksStatus | null): boolean` (null = probe error → false) and `canConfigure(status: HooksStatus): boolean` (`binary_on_path`). Test in `src/lib/hooksSetup.test.ts`.

### 4. Dialog
- `HooksSetupDialog`: mount effect with a `let cancelled = false` guard; `invoke<HooksStatus>("setup_hooks_status")`; on resolve (not cancelled) store the status and open when `shouldOpenHooksDialog`; on reject `console.warn` and stay closed.
- Body: title "Connect Nergal to Claude Code"; description naming the panels that need the hooks (Activities, Tasks, plan review, questions); the `settings_path` in a `<code>`; a note that a backup is saved as `settings.json.bak`. When `!canConfigure`: a warning line that `nergal` is not on `PATH` (AppImage installs) and the `.deb` or a `PATH` entry fixes it.
- Keyboard from `ConfirmHost`: initial focus on the popup through `initialFocus={contentRef}` on this dialog's own `DialogContent` (Revision 2; a rAF focus loses to Base UI's own initial focus) (not the document-wide `[data-slot="dialog-content"]` query: a launch-time stack would focus the wrong dialog); `showCloseButton={false}`, `tabIndex={-1}`; `onKeyDownCapture` **always** consumes Enter (`preventDefault` + `stopPropagation`) and calls `handleConfigure()` only when `canConfigure && !busy`, so Enter never falls through to Skip. Enter whose target is a `<button>` is left alone (native activation of that button); the `enter` `Kbd` chip renders only when Configure is enabled; the inline error has `role="alert"`; with `!canConfigure` the path line reads "Hooks would be added to…". Esc goes through `onOpenChange` → skip handler (blocked while `busy`). Both buttons get `focus-visible:ring-0 focus-visible:border-transparent`.
- "Configure": `busy` → `invoke("setup_hooks")` → close + success toast ("Hooks configured", description: new Claude Code sessions will report to Nergal). On reject: stay open, show `String(err)` inline (`text-destructive`), clear `busy`. Disabled when `!canConfigure`.
- Mount `<HooksSetupDialog />` after `<WorktreeGate />` at `App.tsx:231`.

## Tests (Red first)

Rust, `setup.rs` tests. Most of these are **characterization tests** for the `apply_setup` extraction: they describe current behavior and go green as soon as the code compiles. They lock the refactor. The red-for-the-right-reason tests are marked **(new behavior)**.
- empty map → changes not empty;
- map after one `apply_setup` → a second run is empty (idempotent / all hooks present);
- `SessionStart` entry removed → not empty;
- `tool-done` entry with matcher `"Write|Edit"` → not empty (matcher upgrade);
- `OBSOLETE_HOOKS[0]` entry added to a fully set-up map → not empty;
- `statusLine` key removed → not empty;
- `statusLine` replaced by a user command → empty;
- fully set-up map in wrapper form → empty;
- **(new behavior)** `"SessionStart": {}` → `apply_setup` errors and the error names `SessionStart`;
- **(new behavior)** `hooks_status_at` on a missing file in a missing dir → `registered: false`, neither dir nor file exists afterward. Prove it can go red: run it once against a stub of `hooks_status_at` that calls `load_settings`;
- invalid JSON → `hooks_status_at` errors, file bytes unchanged;
- `run_at` on a temp dir → writes the file; a second `run_at` leaves the bytes unchanged.

Frontend, `src/lib/hooksSetup.test.ts` **(new behavior)**: `shouldOpenHooksDialog` for `registered: true` → false, `registered: false` → true, `null` → false; `canConfigure` for `binary_on_path` true/false.

The dialog's DOM behavior (Enter, Esc, focus, error display) is covered by the manual walk.

## Per-phase risk

- **[Refactor changes `run()` behavior]** → characterization tests + `run_at` temp-dir test before and after the extraction.
- **[D7 breaks a user whose settings have a non-array event]** → the CLI now errors with the event name instead of silently skipping. That user's hook was already dead; the error tells them which key to fix.
- **[Probe creates `~/.claude/`]** → dedicated read path (D3) + the missing-dir test.
- **[Enter activates Skip]** → `ConfirmHost` capture-phase handler; walk step checks it.
- **[Dialog steals focus from the terminal at launch]** → opens only on fresh installs or after the hooks were removed; `Dialog` returns focus on close.
