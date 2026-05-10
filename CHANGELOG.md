# Changelog

All notable changes to Nergal are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.1] — 2026-05-10

### Fixed
- **Agent picker silently fell back to Claude Code** on the installed `.deb` because Linux desktop launchers hand the binary a minimal `PATH` (no `nvm`, no `~/.opencode/bin`). The runtime now augments `PATH` from an interactive `$SHELL` instance before adapter detection, so Pi, OpenCode, and Codex appear when installed.
- **AskUser modal could be dismissed without answering**, leaving the agent process hung on its FIFO read. The dialog is now non-dismissible — only the Send action (button or Ctrl+Enter) closes it after writing answers.
- **AskUser feedback never reached Claude.** The textarea text was being packed as `_feedback` inside the answers map, which Claude treated as the answer to a phantom question. Feedback is now forwarded via `hookSpecificOutput.additionalContext`, the documented hook channel into Claude's context window.
- **Dialog content stretched vertically beyond its content** in AgentPicker / AskUser / Commit modals. Replaced the `grid` container with `flex flex-col` so children stack at their natural height.
- **SweetAlert2 confirm dialogs caused a vertical void below the status bar** while open. Opted out of SweetAlert2's `heightAuto` so the `h-full` cascade from `#root` keeps working.
- **Background panel listeners (FilesChip, PrViewer, DiffView) reacted to Space/Enter/arrows while a modal was open**, hijacking keys destined for the modal. Each listener now bails when a dialog is mounted.

### Added
- **Keyboard navigation across all modals.** AskUser and Ship modals auto-focus their first interactive element and route arrow keys through every option / field. From the last option of an AskUser question, ArrowDown reaches the feedback textarea; from feedback, ArrowUp at cursor 0 returns to the options. Inside Ship, ArrowDown at the last staged file escapes to the next field.
- **Ship BranchPicker now also commits with Space**, not only Enter.
- **Workspace close action.** A subtle "×" appears on a workspace row hover and triggers a swal confirm before removing the workspace + its worktrees.

### Changed
- **Removed the standalone Commit modal and `Ctrl+Shift+C` flow.** Committing now lives entirely in Ship (`Ctrl+Shift+Y`), which already covers stage + commit + push + PR with the StagePicker for unstaged files.
- **Ctrl+V now does smart paste.** Text in clipboard goes through bracketed paste; if the clipboard has no text, `\x16` is forwarded to the PTY so Claude Code can handle image paste natively. Ctrl+Shift+V kept as an alias.

## [0.1.0] — 2026-05-09

First public preview. Linux desktop HUD that wraps AI coding-agent CLIs.

### Added
- Multi-session PTY terminal — real `claude` (or any registered agent CLI) running in a PTY, multiple sessions across multiple workspaces.
- Plan review with inline annotations — blocks `ExitPlanMode` via the hook pipeline; approve, or reject with structured feedback.
- Live task tracking — `TodoWrite` events stream into a session-scoped task panel.
- Multi-agent support — adapter foundation for Claude Code, Codex, OpenCode, and Pi.
- Git panel with chip tabs (Files / History / Stashes / PRs / Conflicts).
- Atomic ship-flow — single action composes commit + push + open PR.
- Three-pane conflict resolution (ours / theirs / merged + "ask agent to resolve").
- Side-by-side keyboard-navigable diff viewer.
- File panel with a CodeMirror 6 quick editor.
- Read-only OpenSpec viewer with the same annotation engine as plans.
- Live preview browser + localhost port scanner.
- Floating scratchpad anchored to a configurable directory.
- Activity timeline + interactive DAG of tool calls.
- 13 built-in themes plus a custom theme editor with live preview.

[0.1.1]: https://github.com/Mufdi/nergal/releases/tag/v0.1.1
[0.1.0]: https://github.com/Mufdi/nergal/releases/tag/v0.1.0
