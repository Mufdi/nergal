# Changelog

## v0.1.2 — 2026-05-11

* Fixed Shift+Enter sending the same `\r` as Enter — bypasses the wezterm encoder and writes a literal LF so multi-line input lands in CC's TUI without relying on the Kitty keyboard protocol opt-in
* Fixed terminal selection staying pinned to the same viewport row on scroll; now follows the underlying text in both the primary screen (via scrollOffset delta) and alt screen (via row content match), and reappears when off-screen content scrolls back into view
* Fixed scratchpad selection rendering as near-white-on-white because CodeMirror's higher-specificity focused rule was winning; themed orange now survives focus with `color: inherit` so text stays readable
* Fixed the right panel leaving a gap below file content — CodeMirror is now pinned to `height: 100%` and the AnnotationsDrawer empty placeholder no longer reserves 28 px
* Fixed `Ctrl+N` getting silently dropped when fired before the workspace list resolved; the effect now replays once workspaces arrive
* Fixed the sidebar focus indicator staying behind after selecting / renaming / creating a session — the accent border now follows the cursor into the terminal panel
* Added double-click on a terminal word to select it and auto-copy
* Added sidebar shortcuts: hover a row (mouse or keyboard nav) and press `d` to delete or `r` to rename — works regardless of which zone has focus
* Changed AskUserQuestion flow: the blocking modal is hidden and CC's TUI handles the question prompt natively in the terminal; the session tab blinks twice in the theme's primary color and stays tinted until you answer

## v0.1.1 — 2026-05-10

* Fixed agent picker silently auto-resolving to Claude Code on the installed `.deb` because Linux desktop launchers hand the binary a minimal `PATH` (no `nvm`, no `~/.opencode/bin`); runtime now augments `PATH` from an interactive `$SHELL` before adapter detection
* Fixed AskUser modal being dismissible without answering, which hung the agent process on its FIFO read; dialog is now non-dismissible until Send
* Fixed AskUser feedback never reaching Claude — was packed as `_feedback` answer key, now flows through `hookSpecificOutput.additionalContext`
* Fixed dialog content stretching vertically beyond its content in AgentPicker / AskUser / Commit (`grid` → `flex flex-col` in the base DialogContent)
* Fixed SweetAlert2 confirm dialogs causing a vertical void below the status bar (`heightAuto: false`)
* Fixed background panel listeners (`FilesChip`, `PrViewer`, `DiffView`) hijacking Space/Enter/arrows while a modal was open
* Added arrow-key navigation across AskUser and Ship modals — auto-focus on first focusable, wraparound, and field-to-field walk
* Added Workspace close button on hover with swal confirm
* Changed `Ctrl+V` to smart paste — text in clipboard goes through bracketed paste, empty/image clipboard forwards `\x16` for Claude Code's native image paste; `Ctrl+Shift+V` kept as alias
* Removed standalone Commit modal and `Ctrl+Shift+C` flow; commit now lives entirely in Ship (`Ctrl+Shift+Y`)

## v0.1.0 — 2026-05-09

* First public preview

[0.1.2]: https://github.com/Mufdi/nergal/releases/tag/v0.1.2
[0.1.1]: https://github.com/Mufdi/nergal/releases/tag/v0.1.1
[0.1.0]: https://github.com/Mufdi/nergal/releases/tag/v0.1.0
