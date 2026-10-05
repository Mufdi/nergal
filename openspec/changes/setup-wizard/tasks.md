# Tasks — setup-wizard

> Read `implementation.md` first: it carries the verified line refs, both analog components and the execution order. Scope re-validated 2026-10-05 (minimal hooks dialog only).

## 1. Backend — hooks status probe

- [x] 1.1 Test (Red): in `src-tauri/src/setup.rs` tests, write the characterization tests for `apply_setup` (empty map, idempotent second run, `SessionStart` removed, outdated `tool-done` matcher, obsolete hook, `statusLine` absent, user-owned `statusLine`, wrapper form), the new-behavior tests (`"SessionStart": {}` errors naming the event; `hooks_status_at` on a missing dir creates nothing, proven red once against a stub calling `load_settings`), invalid JSON (error, bytes unchanged) and `run_at` idempotency on a temp dir.
- [x] 1.2 Extract `apply_setup(&mut Map) -> Result<SetupChanges>` and `run_at(&Path)` from `setup::run()` (`setup.rs:155`); `run()` keeps its current output and early-return.
- [x] 1.3 Make `apply_setup` error on a non-array value under any `hooks` event key, naming the event.
- [x] 1.4 Share the settings parse step between `load_settings` and the probe; add a side-effect-free read for the probe (no `create_dir_all`).
- [x] 1.5 Add `HooksStatus { registered, settings_path, binary_on_path }`, `hooks_status_at(&Path)` (`binary_on_path` = no `APPIMAGE` env var and `which::which("nergal")` resolves) and `pub fn hooks_status()`.

## 2. Backend — Tauri commands

- [x] 2.1 Add `setup_hooks_status` next to `setup_hooks` in `src-tauri/src/commands/workspaces_sessions.rs:87`, errors as `format!("{e:#}")`.
- [x] 2.2 Change `setup_hooks` to map errors with `format!("{e:#}")`.
- [x] 2.3 Register `commands::setup_hooks_status` after `commands::setup_hooks` at `src-tauri/src/lib.rs:485`.

## 3. Frontend — probe helpers

- [x] 3.1 Test (Red): `src/lib/hooksSetup.test.ts` for `shouldOpenHooksDialog` (registered true → false, false → true, null → false) and `canConfigure` (`binary_on_path` true/false).
- [x] 3.2 Add the `HooksStatus` interface to `src/lib/types.ts` and implement `src/lib/hooksSetup.ts`.

## 4. Frontend — hooks setup dialog

- [x] 4.1 Create `src/components/setup/HooksSetupDialog.tsx`: layout from `ProjectBootstrapPrompt`, keyboard from `ConfirmHost` (rAF focus via a `ref` on its own `DialogContent`, capture-phase Enter always consumed, `showCloseButton={false}`, BUG-25 button classes); probe once with a `cancelled` guard; Configure disabled with a `PATH` note when `!canConfigure`; success closes + toast; error stays open with the message inline.
- [x] 4.2 Mount `<HooksSetupDialog />` after `<WorktreeGate />` in `src/App.tsx:231`.

## 5. Revision 1: post-review fixes

- [x] 5.1 Test (Red): in `setup_status_tests.rs`, add: `"MyEvent": null` with no Nergal hooks → `apply_setup` Ok, `MyEvent` still null; `run_at` on a file with `"SessionStart": {}` → Err and bytes unchanged; `binary_usable` cases (None → false; `/usr/bin/nergal` with appdir `/tmp/.mount_x` → true; `/tmp/.mount_x/usr/bin/nergal` with that appdir → false; any path with no appdir → true).
- [x] 5.2 Scope the D7 check to the events in `HOOKS` and `OBSOLETE_HOOKS`.
- [x] 5.3 Add `binary_usable` and use it in `hooks_status_at`; `#[cfg(windows)]` → `true`.
- [x] 5.4 Dialog: leave Enter on a focused `<button>` to the button; show the `enter` chip only when Configure is enabled; `role="alert"` on the inline error; "would be added" copy when not configurable.
- [x] 5.5 Docs: `docs/hooks.md` and `README.md` point to `nergal hook setup` and the launch dialog; replace the obsolete snippet with a pointer to `src-tauri/src/setup.rs` `HOOKS`.

## 6. Verification

- [x] 6.1 Full check: `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit && pnpm test`.
- [ ] 6.2 Manual walk, fresh state (temp `HOME` without `.claude/`, `nergal` on `PATH`): the dialog opens and shows the path; no file or dir exists before Configure; Enter configures (not Skip); Tab to Skip + Enter skips; the dialog closes with a toast; a new Claude Code session reports to Activities.
- [ ] 6.3 Manual walk, Skip: Esc closes, no file is written; the next launch shows the dialog again.
- [ ] 6.4 Manual walk, configured install (the dev machine's real `~/.claude/settings.json`): no dialog at launch.
- [ ] 6.5 Manual walk, Configure error (temp `HOME` with an empty `.claude/` set to `chmod 555`, no `settings.json`): the dialog stays open showing the permission error; the directory stays empty.
- [ ] 6.6 Manual walk, AppImage (launch the real `.AppImage` with hooks absent and no other `nergal` on `PATH`): Configure is disabled and the `PATH` note shows; with the `.deb` also installed, Configure is enabled.
