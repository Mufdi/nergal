# Changelog

## v0.1.4 — 2026-05-24

* Added per-agent plan capability gating — the right-panel Plan view is now hidden for agents that don't persist plans to disk (OpenCode, Codex, Pi); for those sessions, switching to Plan auto-redirects to Files instead of leaving a dead button
* Changed the release pipeline so that pushing a `v*` tag triggers GitHub Actions to build, sign, and publish `.deb` / `.rpm` / `.AppImage` / `.AppImage.sig` / `latest.json` — v0.1.4 is the first release cut through the signed CI flow, so the in-app updater finally has a real signature to verify against the pinned pubkey
* Fixed the Plans panel showing empty when Claude Code's `plansDirectory` setting pointed somewhere other than `.claude/plans/` — the watcher and on-demand reader both now resolve the directory through `~/.claude/settings.json` < `<cwd>/.claude/settings.json` < `<cwd>/.claude/settings.local.json` (with `~/` expansion and absolute-vs-relative normalisation), so any of the standard CC settings locations is honoured
* Fixed clicks on Claude Code's interactive TUI menus (model picker, AskUserQuestion choices, etc.) being silently dropped — the terminal panel now forwards primary mouse Press/Move/Release events to the backend, which encodes them through wezterm's mouse-reporting modes when the agent enables them. Shift+click still falls through to text selection so copying over an interactive UI keeps working
* Fixed `Ctrl+Shift+B` orphaning focus when the right panel collapsed (next keystroke fell to body instead of the terminal) and not shifting focus into the panel when it expanded — both directions now route focus deterministically: collapse → terminal, expand → panel
* Fixed `Ctrl+ñ` behaving as a focus toggle (blurring the terminal if it was already focused) instead of the documented hard-focus — pressing it from any zone now reliably moves focus into the active terminal
* Fixed the Cleanup session / Finish merge action buttons in the Git panel getting pushed against the banner edge when the description grew — both banners now use proper flex sizing so the action button keeps breathing room
* Removed the legacy `plansDirectory` field from cluihud's own Settings — only the agent's `plansDirectory` setting matters and the cluihud-level field was vestigial; existing `config.json` files keep the field silently ignored

## v0.1.3 — 2026-05-20

* Added per-agent theme sync: switching themes in Settings now propagates the active palette to pi (live, via its custom-theme hot-reload), OpenCode (next-launch, via `~/.config/opencode/themes/cluihud-active.json` + best-effort live API), and Codex (syntax-only, via `~/.codex/config.toml` `tui.theme`)
* Added Settings → About section (Alt+7) with the running version, install source (.deb / AppImage / dev), a single "Update" button that mutates through `Checking → Up to date | Available → Downloading → Downloaded`, and an inline "What's new in v0.1.x" changelog fetched from the current release on GitHub
* Added .deb update flow: when an update is available the new package downloads to `~/Downloads/` (respecting `xdg-user-dir DOWNLOAD`) and offers "Reveal in file manager" — cluihud never prompts for `sudo`; the user runs their own package manager
* Added an amber warning banner in About when running a dev build, so the check still works for testing but the install action is suppressed
* Fixed the "Open on GitHub" icon next to each PR (PR viewer + PRs chip) doing nothing — the Tauri webview silently drops `<a target="_blank">` for security; both call sites now use `@tauri-apps/plugin-shell::open` so the click reliably opens the browser
* Fixed the right panel forgetting its collapsed/expanded state when switching sessions; per-session memory now restores whichever pane state the user left behind
* Fixed the global file picker (Ctrl+Shift+K) only filtering visible tree entries; it now does a recursive backend search (skips `.git`/`node_modules`/`target`/`dist`, caps at 500 results) so any file in the project is one keystroke away
* Fixed the Tasks panel never showing tasks that existed before the session became active — it now hydrates from SQLite on first activation instead of waiting for the next live event
* Fixed Ctrl+B re-opening the new-session modal when toggling the sidebar; the dialog-trigger tracking now survives the sidebar's unmount cycle
* Fixed the Tasks panel getting pushed off-screen with many sessions open (`min-h-0` on the flex column) and added a tooltip on truncated task names so the full subject is one hover away
* Fixed `Ctrl+Shift+E` failing to open files with relative paths on agents whose binaries don't inherit cwd — editor commands now run with the session's working directory
* Fixed the agent-attention pulse using a hardcoded orange; the animation now follows the theme's primary color and the session tab gets the same treatment
* Changed the sidebar focus model to three distinct visual states (active row, keyboard cursor, mouse hover) with `:focus-within`-scoped rules so a focus indicator no longer leaks to the terminal panel
* Changed the Settings theme picker to defer creating a "Custom" variant until the first real edit (lazy fork) so opening and closing the editor with no changes no longer pollutes the picker
* Changed the status bar to display 24-hour time everywhere and clamps the localhost-ports chip stack to `max-w-[16rem] overflow-x-auto` so a busy machine doesn't shove the rest of the status bar off-screen
* Removed the unused "Show completed" toggle from the sidebar header — sessions never reach the `completed` status today, so the toggle was dead code

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

[0.1.3]: https://github.com/Mufdi/nergal/releases/tag/v0.1.3
[0.1.2]: https://github.com/Mufdi/nergal/releases/tag/v0.1.2
[0.1.1]: https://github.com/Mufdi/nergal/releases/tag/v0.1.1
[0.1.0]: https://github.com/Mufdi/nergal/releases/tag/v0.1.0
