# Hook integration

Nergal observes the agent CLI through its hook pipeline. The CLI calls `nergal hook ...` subcommands, which write to a Unix socket the GUI listens on. Two hooks are blocking and use named FIFOs for the round-trip decision.

All endpoints live in a per-user IPC directory resolved by `ipc_dir()` (`src-tauri/src/platform/mod.rs`): on Linux, `/run/user/<uid>/nergal/` (systemd-managed, un-squattable) with a `~/.local/share/nergal/ipc/` fallback — it never falls back to a guessable path under shared `/tmp`. On macOS the equivalent is `temp_dir()/nergal/` (already per-user `0700`). On Windows there is no filesystem path — endpoints are per-user named pipes (`\\.\pipe\nergal-<user-SID>-<endpoint>`).

## CLI surface

| Subcommand | Mode | Purpose |
|---|---|---|
| `nergal hook send <event>` | async | Forward an event payload to the hook socket (`hook.sock` in the per-user IPC dir). |
| `nergal hook inject-edits` | sync | Modify the prompt before submission (used on `UserPromptSubmit`). |
| `nergal hook plan-review` | blocking, FIFO | Block on the plan-review FIFO (`plan-{pid}.fifo` in the per-user IPC dir) until the GUI returns `allow` / `deny`. |
| `nergal hook ask-user` | async (notifier) | Fire-and-forget signal that AskUserQuestion is pending. CC's TUI owns the question; nergal only blinks the session tab. |
| `nergal hook setup` | one-shot | Configure the hook entries and the `statusLine` in `~/.claude/settings.json`, merging with existing user hooks and saving a `settings.json.bak`. The app offers the same action at launch when the hooks are missing (`HooksSetupDialog`). |

Source: `src-tauri/src/hooks/{cli,server,events,state}.rs`, `src-tauri/src/setup.rs`.

## Conditional wrapper

A user-installed shell wrapper at `~/.claude/hooks/nergal-conditional.sh` inspects the `~/.nergal-active` sentinel before invoking the binary. If the GUI is not running, it exits 0 without forwarding — zero noise, zero latency for sessions outside the GUI.

## Plan review flow (blocking via PermissionRequest)

1. Claude calls `ExitPlanMode`. The `PermissionRequest[ExitPlanMode]` hook fires.
2. `nergal hook plan-review` blocks on `plan-{pid}.fifo` in the per-user IPC dir.
3. The GUI loads the plan in `AnnotatableMarkdownView`. The user can add inline annotations while `planReviewStatusMapAtom` is in `pending_review`.
4. Accept → GUI writes `allow` to the FIFO → Claude proceeds.
5. Reject → GUI writes `deny` with a Plannotator-style message that points Claude back to the edited plan file → Claude re-reads and re-plans.

State machine: `idle → pending_review → submitted` in `src/stores/plan.ts`.

## AskUserQuestion attention

- `PreToolUse[AskUserQuestion]` invokes `nergal hook ask-user`, which only emits a socket message and exits — CC's TUI renders the prompt natively in the terminal.
- GUI emits `ask:user-pending` → session tab blinks twice in primary color and stays tinted until `PostToolUse[AskUserQuestion]` clears it via `ask:user-resolved`.
- `AskUserModal` is retained as `AskUserModalLegacy` in `src/components/session/AskUserModal.tsx`. The exported component returns `null`; restore it by swapping the bodies if we ever want the modal flow back.

## Hook entries

The registered entries are the `HOOKS` table in `src-tauri/src/setup.rs` (plus `STATUSLINE_COMMAND`); `OBSOLETE_HOOKS` lists the ones setup removes. There are two ways to write them:

- **At launch**: when `setup_hooks_status` reports them missing, Nergal opens a dialog. "Configure" writes them; "Skip" asks again on the next launch. The app never writes the file without that click. On Unix the entries call bare `nergal`, so "Configure" stays disabled when no `nergal` outside the AppImage mount is on `PATH` (AppImage-only installs).
- **From a terminal**: `nergal hook setup`.

Re-run setup after editing `HOOKS` in `src-tauri/src/setup.rs` and reinstalling the binary via `pnpm tauri build && sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb` — NOT `cargo install --path src-tauri --force`, which puts a binary in `~/.cargo/bin/` that shadows `/usr/bin/nergal` for the GNOME launcher and skips the Tauri frontend bundling step.
