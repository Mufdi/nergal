## Context

See proposal.md. Verified: wezterm's `TerminalState::palette()` returns `self.palette` when an app set colors via OSC, and otherwise calls `config.color_palette()` on each use (`~/.cargo/git/checkouts/wezterm-*/5046fc2/term/src/terminalstate/mod.rs:660-665`). Default fg/bg cells render as `None` in the snapshot (`session.rs:395-399`), so the palette does not change what the canvas paints.

## Goals / Non-Goals

**Goals:** true OSC 10/11 answers; no half frames from synchronized output.
**Non-Goals:** the 16 ANSI colors (the canvas uses the backend palette for them already); a real DECRQM state for 2026; OSC 12 (cursor).

## Decisions

### D1. A shared default-colors cell, injected into each config
- `config.rs` defines `ColorCell = Arc<RwLock<Option<DefaultColors>>>` and a process-wide `shared_colors()` cell. `NergalTerminalConfig::new()` holds a clone of the shared cell; `with_colors(cell)` takes a private one, so tests never touch the shared cell.
- `color_palette()` starts from `ColorPalette::default()` and overrides `foreground` and `background` when the cell is set. A poisoned lock reads as "not set"; no `unwrap`.
- The command `set_terminal_default_colors(fg, bg)` writes the shared cell. No generation bump: wezterm reads the config palette on each use unless an app set its own colors (`mod.rs:660-675`).
- **Alternative:** push a palette into each session. Rejected: it needs a registry walk and misses sessions spawned between the walk and the next theme change.
- The session's cached `palette` (`session.rs:48`) is used only for indexed colors; default cells stay `None` in the snapshot.

### D2. Mode 2026 tracked by a literal byte matcher
- `advance_bytes` feeds a matcher for the exact sequences `ESC [ ? 2 0 2 6 h` and `ESC [ ? 2 0 2 6 l` (8 bytes each). It carries at most 7 bytes of partial match into the next chunk. `ESC c` (RIS) also turns the mode off.
- `sync_since` is set only on the off-to-on edge; a repeated `h` keeps the first instant, so a program cannot extend the hold by repeating it. `l` and RIS clear it.
- A combined form such as `CSI ? 1 ; 2026 h` is not tracked. A miss means the old tearing risk, never a stall.
- The state lives in its own `Arc<Mutex<Option<Instant>>>` (`SyncState`), shared by the session and the emitter. The emitter never waits on the session mutex that the reader holds during `advance_bytes`.
- **Alternative:** patch wezterm-term to honor the mode. Rejected: it is a git dependency, and the GUI mux owns that state upstream on purpose.

### D3. The emitter holds the frame with a bounded wait
- After the 8 ms coalescing sleep, while `sync_since` is set and younger than 150 ms, the emitter waits on `notify` with a timeout of the time left. A `l` chunk wakes it at once. Each wake re-checks `shutdown`.
- After the cap, the emitter snapshots even if the mode is still on, and does not hold again until the next off-to-on edge.
- Out of scope: the full grid sent on `terminal_attach` (`pty.rs:1963-1972`) and user-driven scroll or resize snapshots. They are one-off and not part of a streaming frame.

### D4. The frontend normalizes the CSS color
- A pure `cssColorToRgb(value)` accepts `#rgb`, `#rrggbb` and `rgb()`/`rgba()`. Anything else returns `null` and nothing is sent.
- The frontend sends the colors at module load and after each theme flip, and logs a rejected `invoke`. A session that queries before the first send gets the default palette; sessions spawn on user action, after the module loads.

## Risks / Trade-offs

- [A theme token in another CSS color syntax (`oklch`) is not sent.] → Today's theme tokens are hex (`src/styles/globals.css`); the normalizer test pins the accepted forms.
- [A program that never closes mode 2026 delays one frame by 150 ms.] → After the cap the emitter stops holding until the next off-to-on edge.
- [An app that sets `OSC 10`/`11` itself stops following theme changes.] → wezterm behavior, pinned by a scenario.
- [Timing tests can flake.] → Wide bounds and explicit sequencing of `advance_bytes` calls.
