//! VT emulation backend built on `wezterm-term`.
//!
//! Phase 1 of `replace-xterm-with-wezterm`: owns the core terminal state in
//! Rust, independent of the PTY and IPC wiring (those land in Phase 2+).
//!
//! The entry points are [`TerminalSession`] (one instance per pty session)
//! and [`CluihudTerminalConfig`] (the [`TerminalConfiguration`] impl that
//! enables Kitty keyboard protocol and sets cluihud-specific defaults).

// Phase 2: wired for dual-emission alongside the legacy `pty:output` path.
// Until the frontend renderer lands (Phase 3) some of these re-exports have
// no in-tree consumers; keep the warnings quiet while the module grows.
#![allow(dead_code, unused_imports)]

mod config;
mod differ;
mod emitter;
mod input;
mod session;
mod types;

pub use config::CluihudTerminalConfig;
pub use differ::GridDiffer;
pub use emitter::TerminalHandle;
pub use input::map_event;
pub use session::TerminalSession;
pub use types::{
    CellSnapshot, CursorSnapshot, GridRow, GridSnapshot, GridUpdate, TerminalKeyEvent,
};
