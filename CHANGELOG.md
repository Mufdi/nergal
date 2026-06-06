# Changelog

## v0.2.0 — 2026-06-05

* Added a full Obsidian vault integration, configured per-workspace in Settings → Obsidian (with an optional global TOML override): quick capture into an inbox note (Ctrl+Alt+Q), a per-session session-log channel, and automatic MOC notes with reverse backlinks written when a session ends
* Added vault search: a global search modal over the vault (with optional subdirectory scoping) plus an `@@` mention picker in the terminal and scratchpad for inserting vault note references mid-prompt
* Added vault note pinning with agent context injection: pin notes to a session and their content is injected as agent context at spawn/resume (Claude Code and Pi via system-prompt file, OpenCode and Codex via launch prompt); pinned notes persist as pinned tabs and hot-reload when the note changes on disk
* Added an Obsidian note reading panel with wikilink rendering and a vault note finder in the right panel (Ctrl+Shift+Q)
* Added `cluihud://` deep links: open a file or spawn a session in Nergal straight from Obsidian or any other app, including cold-start when Nergal isn't running
* Added Obsidian templates to the command palette: every note in the configured templates folder can be sent to the agent prompt, fully keyboard-navigable
* Added first-class non-git workspaces: any directory can now be a workspace — sessions share the directory (no worktrees), the sidebar shows a non-git badge, and the Git panel offers an "Init git" button to convert in place
* Added branch rename without leaving the app: inline from the Git panel header, from the pencil next to the branch in the status bar, or globally via Ctrl+Alt+R; local-only by design, so the remote branch and any open PR keep their name
* Added a Ctrl+Enter fullscreen terminal: the terminal takes over the whole screen (OS fullscreen included) and a second press restores the previous layout
* Added an undo window for destructive deletes: removing a session or a workspace shows a 5-second countdown toast with Undo, and the PTY, DB row and worktree are only destroyed after the window expires
* Added session actions to the collapsed sidebar: hovering a session dot now offers rename and delete
* Added per-task delete in the sidebar tasks island (hover the row or press `d`), plus Ctrl+Alt+X to clear all completed tasks at once
* Added an install health check in About: missing system tools (git, xdg-utils) surface as an amber banner with the suggested install command, and the `.deb`/`.rpm` packages now declare xdg-utils as a dependency
* Added Ioskeley Mono Term as a terminal font option
* Fixed "Reveal in file manager" after downloading an update doing nothing: the reveal now uses the freedesktop FileManager1 D-Bus interface so the file-manager window actually raises with the file highlighted, falling back to xdg-open on other desktops
* Fixed update downloads restarting from scratch when revisiting the About section; a fully staged `.deb` in Downloads is detected and offered for reveal instead of re-downloading
* Fixed pressing `d` to drop a stash opening the delete-session modal instead — the sidebar's hover shortcut no longer hijacks keys based on a stale row selection
* Fixed the expand-to-Zen button in the Git panel doing nothing while the Conflicts chip was active; the button now routes by active chip exactly like Ctrl+Shift+0
* Fixed the Ship modal rendering only its action buttons with no stage list or title/body fields — a WebKitGTK flex-layout quirk collapsed the modal body to zero height
* Fixed the right panel re-opening on session switch over an explicit hide when a plan review was parked pending
* Fixed focus after switching session tabs: it now always lands on the terminal prompt — restored panels (Obsidian finder, file browser, plan/spec/diff pickers, open file editors, pinned-note tabs) used to steal focus or silently swallow keystrokes
* Fixed Obsidian settings paths failing on wrong capitalization: paths now resolve case-insensitively against the on-disk spelling, both at save time and in the live validation
* Fixed Obsidian templates not appearing in the command palette unless their filenames carried an undocumented `template-` prefix; every `.md` note in the folder now counts, matching Obsidian's own Templates plugin
* Fixed branch renames appearing to succeed without persisting: the rename now also updates the session record that the UI, Ship and cleanup flows read from
* Fixed deleted sessions leaving stale entries in `git worktree list`: cleanup falls back to removing the orphaned directory and pruning the registration when git refuses
* Fixed the terminal canvas remaining visible after soft-closing the last session tab
* Fixed a React nested-button warning in the sidebar session rows
* Improved the scratchpad with the same focused-panel accent border the docked panels use

## v0.1.4 — 2026-05-24

* Added per-agent gating for the Plan view; it's now hidden on agents that don't persist plans to disk (OpenCode, Codex, Pi), and switching to Plan on those sessions auto-redirects to Files
* Fixed the Plans panel showing empty when Claude Code's `plansDirectory` setting pointed outside `.claude/plans/`; the watcher and reader now honour `~/.claude/settings.json`, `<cwd>/.claude/settings.json`, and `<cwd>/.claude/settings.local.json` (in that precedence) so any standard CC location works
* Fixed clicks on Claude Code's interactive TUI menus (model picker, AskUserQuestion choices) being silently dropped; the terminal panel now forwards primary mouse events to the agent, and Shift+click still falls through to text selection so copying over an interactive UI keeps working
* Fixed `Ctrl+Shift+B` orphaning focus when the right panel collapsed and not capturing focus when it expanded; both directions now route focus deterministically (collapse → terminal, expand → panel)
* Fixed `Ctrl+ñ` blurring the terminal when it was already focused instead of acting as a hard-focus; pressing it from any zone now reliably moves focus into the active terminal
* Fixed the Cleanup session / Finish merge buttons in the Git panel getting pushed against the banner edge when the description grew — both banners now use proper flex sizing so the action keeps breathing room
* Removed the legacy `plansDirectory` field from cluihud's own Settings; only the agent's `plansDirectory` matters, and existing `config.json` files keep the field silently ignored

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
