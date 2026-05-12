# Hook integration

Nergal observes the agent CLI through its hook pipeline. The CLI calls `cluihud hook ...` subcommands, which write to a Unix socket the GUI listens on. Two hooks are blocking and use named FIFOs for the round-trip decision.

## CLI surface

| Subcommand | Mode | Purpose |
|---|---|---|
| `cluihud hook send <event>` | async | Forward an event payload to `/tmp/cluihud.sock`. |
| `cluihud hook inject-edits` | sync | Modify the prompt before submission (used on `UserPromptSubmit`). |
| `cluihud hook plan-review` | blocking, FIFO | Block on `/tmp/cluihud-plan-{pid}.fifo` until the GUI returns `allow` / `deny`. |
| `cluihud hook ask-user` | async (notifier) | Fire-and-forget signal that AskUserQuestion is pending. CC's TUI owns the question; cluihud only blinks the session tab. |
| `cluihud setup` | one-shot | Auto-configure hook entries in `~/.claude/settings.json` and per-agent equivalents (e.g., `~/.codex/hooks.json`), conservatively merging with existing user hooks. |

Source: `src-tauri/src/hooks/{cli,server,events,state}.rs`, `src-tauri/src/setup.rs`.

## Conditional wrapper

A user-installed shell wrapper at `~/.claude/hooks/cluihud-conditional.sh` inspects the `~/.cluihud-active` sentinel before invoking the binary. If the GUI is not running, it exits 0 without forwarding — zero noise, zero latency for sessions outside the GUI.

## Plan review flow (blocking via PermissionRequest)

1. Claude calls `ExitPlanMode`. The `PermissionRequest[ExitPlanMode]` hook fires.
2. `cluihud hook plan-review` blocks on `/tmp/cluihud-plan-{pid}.fifo`.
3. The GUI loads the plan in `AnnotatableMarkdownView`. The user can add inline annotations while `planReviewStatusMapAtom` is in `pending_review`.
4. Accept → GUI writes `allow` to the FIFO → Claude proceeds.
5. Reject → GUI writes `deny` with a Plannotator-style message that points Claude back to the edited plan file → Claude re-reads and re-plans.

State machine: `idle → pending_review → submitted` in `src/stores/plan.ts`.

## AskUserQuestion attention

- `PreToolUse[AskUserQuestion]` invokes `cluihud hook ask-user`, which only emits a socket message and exits — CC's TUI renders the prompt natively in the terminal.
- GUI emits `ask:user-pending` → session tab blinks twice in primary color and stays tinted until `PostToolUse[AskUserQuestion]` clears it via `ask:user-resolved`.
- `AskUserModal` is retained as `AskUserModalLegacy` in `src/components/session/AskUserModal.tsx`. The exported component returns `null`; restore it by swapping the bodies if we ever want the modal flow back.

## Project `settings.json` snippet

```json
{
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "cluihud hook send session-start", "async": true }] }],
    "PreToolUse": [{ "matcher": "ExitPlanMode", "hooks": [{ "type": "command", "command": "cluihud hook send plan-ready", "async": true }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": "cluihud hook send tool-done", "async": true }] }],
    "TaskCompleted": [{ "hooks": [{ "type": "command", "command": "cluihud hook send task-done", "async": true }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "cluihud hook send stop", "async": true }] }],
    "SessionEnd": [{ "hooks": [{ "type": "command", "command": "cluihud hook send session-end", "async": true }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "cluihud hook inject-edits" }] }]
  }
}
```

`cluihud setup` writes this for you. Re-run after editing `src-tauri/src/hooks/cli.rs` and reinstalling the binary (`cargo install --path src-tauri --force`).
