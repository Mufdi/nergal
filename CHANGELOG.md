# Changelog

## v0.4.0 — 2026-06-23

* Added Linear integration: an issue panel grouped by state, project, assignee, or cycle, with faithful Linear status glyphs and Linear's own ordering, multiple workspaces (an API key each), background polling, and an "assigned to me" filter
* Added Linear issue actions: send an issue to the agent as a prompt, spawn a worktree session from it, bind or pin it as session context, and write back state, assignee, cycle, comments, and issue closure
* Added one-click status changes from the ClickUp and Linear panels — pick a new status straight from a task/issue row, with an optimistic update that reconciles against the background mirror
* Added a "sync now" button to the ClickUp and Linear panel headers that forces an immediate refresh instead of waiting for the next poll
* Added an agent-coordination layer over an optional, off-by-default MCP server: an agent can see and query your other live Nergal sessions across workspaces (mode, recently touched files, last message), message other sessions (delivery wakes the target's terminal), and request new worktree sessions that you approve at a GUI gate before they spawn — registered into Codex and OpenCode automatically
* Added drag-to-reorder for workspaces in the sidebar: toggle reorder mode next to the "+" button, then drag the handle to reposition; the order persists
* Added a user-remappable keymap editor in Settings, with live capture and collision detection, plus a notification-history popover in the status bar
* Changed the project's internal name from its original `cluihud` code-name to `nergal` everywhere (binary, hook subcommands, config directory, environment variables, IPC paths, deep-link scheme, MCP server). A one-time, non-destructive startup migration moves existing config, database, Claude Code hook entries, the activity sentinel, and stored ClickUp/Linear keyring tokens to the new names, so upgrading installs carry over without losing anything
* Changed "Open in Obsidian" to the bare key `o` while the Obsidian note panel is focused, freeing `Ctrl+Shift+V` for the terminal's own paste
* Fixed the status bar shifting as activity text and rate-limit numbers changed; the columns now hold stable widths and the center cluster stays centered
* Fixed Codex sessions not reporting model, effort, and token usage in the status bar — these now come from the real rollout file, matched to the session by working directory
* Fixed the terminal not copying a plain text selection made over a full-screen TUI, and softened the selection highlight color
* Fixed freeing a listening port not handling Docker — the ports popover is now keyboard-navigable and can stop the owning container, and Docker Compose projects started in a session are stopped when you close Nergal
* Fixed the theme picker not marking a custom theme as the active one
* Fixed `Ctrl+Shift+R` (revise plan / resolve conflict / apply PR annotations) not always firing as a global shortcut

## v0.3.0 — 2026-06-13

* Added ClickUp integration: a panel listing your tasks by project, "assigned to me", and due date — with status/priority icons and a subtask tree — plus desktop notifications when a task is assigned to you, and a task detail you can also open as its own tab (`T`)
* Added ClickUp task actions: send a task to the agent as a prompt, spawn a worktree session from it, pin or bind it as session context, and write back status changes, comments, checklist toggles, due dates and assignee removals
* Added a quake shell (`Ctrl+}`): a drop-down terminal overlay with per-session shells, environment shells seeded by per-workspace suggestions, and a working directory remembered per shell
* Added per-session launch options: choose the agent, permission preset, startup command and environment shells when creating a session
* Added a provider status indicator in the status bar that surfaces active Claude/OpenAI incidents and links to the status page
* Added a per-workspace, configurable OpenSpec specs path that live-refreshes when changes land on disk
* Added an "update available" toast on launch when a newer release is published, and an "Open log file" action in Settings › About for diagnostics
* Changed loading indicators app-wide: a slim progress bar for panels and pulsing dots for inline actions, replacing the old spinners
* Fixed the browser panel turning the whole app gray when its web content crashed; the view now recovers automatically
* Fixed processes started in a session's shells outliving the session — they're now stopped when you close the session or quit Nergal
* Fixed the status bar and Git panel showing the previous branch after the agent created or switched branches
* Fixed clearing or deleting tasks not sticking; the change now persists across reloads instead of reappearing
* Fixed the Obsidian search having Enter and Ctrl+Enter reversed, and results showing inner content instead of the file name
* Fixed `Ctrl+Shift+R` not sending a plan to review when the plan was shown as a panel rather than a tab
* Fixed losing keyboard focus after picking an environment-shell suggestion when creating a session
* Fixed the status chip not re-opening the browser panel after the right panel had been hidden
* Fixed OpenCode failing to start with a theme color error
* Fixed the Claude status badge lingering long after the incident had cleared
* Improved tooltips: every action button now shows its tooltip instantly and with a consistent style

## v0.2.0 — 2026-06-05

* Added Obsidian vault integration, configured per workspace: quick capture into an inbox note, a per-session session log, and automatic MOC notes with backlinks when a session ends
* Added vault search: a global search modal plus an `@@` mention picker in the terminal and scratchpad for referencing vault notes mid-prompt
* Added vault note pinning: pinned notes persist as tabs, reload when the note changes on disk, and their content is injected as agent context at spawn/resume
* Added an Obsidian note reading panel with wikilink rendering and a vault note finder (Ctrl+Shift+Q)
* Added `nergal://` deep links to open files or spawn sessions straight from Obsidian, including when Nergal isn't running yet
* Added Obsidian templates to the command palette: any note in the templates folder can be sent to the prompt, keyboard-navigable
* Added non-git workspaces: any directory works — sessions share it without worktrees, the sidebar shows a badge, and the Git panel offers "Init git" to convert
* Added branch rename from the Git panel header, the status bar, or Ctrl+Alt+R; local-only, so the remote branch and open PRs keep their name
* Added Ctrl+Enter fullscreen terminal; a second press restores the previous layout
* Added a 5-second undo window when deleting a session or workspace; nothing is destroyed until it expires
* Added rename and delete actions to the collapsed sidebar's session dots
* Added per-task delete in the tasks island (hover or `d`), plus Ctrl+Alt+X to clear all completed tasks
* Added a system-tools health check in About with a suggested install command; the `.deb`/`.rpm` now declare `xdg-utils` as a dependency
* Added Ioskeley Mono Term as a terminal font option
* Fixed "Reveal in file manager" after downloading an update doing nothing; the file manager now opens with the file highlighted
* Fixed update downloads restarting when revisiting About; an already-downloaded `.deb` is detected and offered instead
* Fixed pressing `d` to drop a stash opening the delete-session modal instead
* Fixed the expand button in the Git panel doing nothing while the Conflicts view was active
* Fixed the Ship modal showing only its action buttons, with no file list or title/body fields
* Fixed the right panel re-opening on session switch after being explicitly hidden when a plan review was pending
* Fixed focus after switching session tabs landing on restored panels or open editors instead of the terminal prompt
* Fixed Obsidian settings paths failing on wrong capitalization; paths now resolve against the on-disk spelling
* Fixed Obsidian templates not appearing in the command palette unless filenames started with `template-`
* Fixed branch renames not persisting: the new name now survives refreshes and is used by Ship and cleanup
* Fixed deleted sessions leaving stale entries in `git worktree list`
* Fixed the terminal canvas remaining visible after soft-closing the last session tab
* Fixed a nested-button warning in the sidebar session rows
* Improved the scratchpad with the same focused accent border the docked panels use

## v0.1.4 — 2026-05-24

* Added per-agent gating for the Plan view; it's now hidden on agents that don't persist plans to disk (OpenCode, Codex, Pi), and switching to Plan on those sessions auto-redirects to Files
* Fixed the Plans panel showing empty when Claude Code's `plansDirectory` setting pointed outside `.claude/plans/`; the watcher and reader now honour `~/.claude/settings.json`, `<cwd>/.claude/settings.json`, and `<cwd>/.claude/settings.local.json` (in that precedence) so any standard CC location works
* Fixed clicks on Claude Code's interactive TUI menus (model picker, AskUserQuestion choices) being silently dropped; the terminal panel now forwards primary mouse events to the agent, and Shift+click still falls through to text selection so copying over an interactive UI keeps working
* Fixed `Ctrl+Shift+B` orphaning focus when the right panel collapsed and not capturing focus when it expanded; both directions now route focus deterministically (collapse → terminal, expand → panel)
* Fixed `Ctrl+ñ` blurring the terminal when it was already focused instead of acting as a hard-focus; pressing it from any zone now reliably moves focus into the active terminal
* Fixed the Cleanup session / Finish merge buttons in the Git panel getting pushed against the banner edge when the description grew — both banners now use proper flex sizing so the action keeps breathing room
* Removed the legacy `plansDirectory` field from nergal's own Settings; only the agent's `plansDirectory` matters, and existing `config.json` files keep the field silently ignored

## v0.1.3 — 2026-05-20

* Added per-agent theme sync: switching themes in Settings now propagates the active palette to pi (live, via its custom-theme hot-reload), OpenCode (next-launch, via `~/.config/opencode/themes/nergal-active.json` + best-effort live API), and Codex (syntax-only, via `~/.codex/config.toml` `tui.theme`)
* Added Settings → About section (Alt+7) with the running version, install source (.deb / AppImage / dev), a single "Update" button that mutates through `Checking → Up to date | Available → Downloading → Downloaded`, and an inline "What's new in v0.1.x" changelog fetched from the current release on GitHub
* Added .deb update flow: when an update is available the new package downloads to `~/Downloads/` (respecting `xdg-user-dir DOWNLOAD`) and offers "Reveal in file manager" — nergal never prompts for `sudo`; the user runs their own package manager
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
