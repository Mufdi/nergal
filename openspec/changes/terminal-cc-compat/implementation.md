## Verified codebase facts

- `NergalTerminalConfig` (`src-tauri/src/terminal/config.rs:18-63`): `color_palette()` returns a clone of a construction-time `ColorPalette::default()`.
- `TerminalSession::with_config` (`session.rs:69-97`) passes `Arc::new(config)` to `Terminal::new` and caches `palette`; `advance_bytes` at `session.rs:101-103`.
- Production sessions: `pty.rs:143` builds `TerminalSession::with_config(…, NergalTerminalConfig::new())`; the reader thread at `pty.rs:488-510` calls `advance_bytes` then `notify_one`.
- Emitter loop: `emitter.rs:93-145`, 8 ms coalescing sleep, then `grid_snapshot` and `compute_update`. Test helpers in `emitter.rs` tests (`attached_channel_receives_session_updates`).
- Existing answer tests: `session.rs:785-799` (`answer_to`, OSC 11 test, 2026 DECRQM test).
- Frontend theme refresh: `refreshTermTheme` in `src/components/terminal/theme.ts:20-31`; called at `terminalService.ts:117` (module load) and `:141` (MutationObserver on `data-theme`, around `:177`).
- Commands live in `src-tauri/src/commands/*.rs` and are registered in `src-tauri/src/lib.rs` `invoke_handler`.

## Locked interfaces

```rust
// src-tauri/src/terminal/config.rs
pub type DefaultColors = ([u8; 3], [u8; 3]); // (fg, bg)
pub type ColorCell = std::sync::Arc<std::sync::RwLock<Option<DefaultColors>>>;
pub fn shared_colors() -> ColorCell;                 // process-wide cell (LazyLock)
pub fn set_colors(cell: &ColorCell, fg: [u8; 3], bg: [u8; 3]);
impl NergalTerminalConfig {
    pub fn new() -> Self;                            // uses shared_colors()
    pub fn with_colors(cell: ColorCell) -> Self;     // tests: private cell
}

// src-tauri/src/terminal/session.rs
pub type SyncState = std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>;
impl TerminalSession {
    pub fn sync_output_since(&self) -> Option<std::time::Instant>;
    pub fn sync_state(&self) -> SyncState;           // shared with the emitter
}

// src-tauri/src/terminal/emitter.rs
pub const SYNC_OUTPUT_CAP: std::time::Duration = std::time::Duration::from_millis(150);

// command
#[tauri::command] pub fn set_terminal_default_colors(fg: [u8; 3], bg: [u8; 3]);
```
```ts
// src/components/terminal/cssColor.ts
export function cssColorToRgb(value: string): [number, number, number] | null;
```

## Execution order

1. Red: Rust tests in `session.rs` (OSC 10/11 after `set_default_colors`, live change, mode tracking incl. split sequence), `emitter.rs` (split frame not torn, unclosed frame sent after the cap, plain output unchanged), and `src/components/terminal/cssColor.test.ts`.
2. Green: config cell → command → frontend call after each `refreshTermTheme()`; scanner → emitter wait.

## Builder coordination

One Builder. Rust tests build sessions with `NergalTerminalConfig::with_colors(private cell)` through `TerminalSession::with_config`, never the shared cell. Emitter tests sequence `advance_bytes` calls explicitly and use wide time bounds.
