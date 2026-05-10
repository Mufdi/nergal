# Changelog

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

[0.1.1]: https://github.com/Mufdi/nergal/releases/tag/v0.1.1
[0.1.0]: https://github.com/Mufdi/nergal/releases/tag/v0.1.0
