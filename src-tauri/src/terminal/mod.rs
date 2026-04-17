//! VT emulation backend built on `wezterm-term`.
//!
//! Phase 1 of `replace-xterm-with-wezterm`: owns the core terminal state in
//! Rust, independent of the PTY and IPC wiring (those land in Phase 2+).
//!
//! The entry points are [`TerminalSession`] (one instance per pty session)
//! and [`CluihudTerminalConfig`] (the [`TerminalConfiguration`] impl that
//! enables Kitty keyboard protocol and sets cluihud-specific defaults).

// Phase 1: nothing wires these into Tauri commands yet; Phase 2 will.
#![allow(dead_code, unused_imports)]

mod config;
mod session;
mod types;

pub use config::CluihudTerminalConfig;
pub use session::TerminalSession;
pub use types::{CellSnapshot, CursorSnapshot, GridSnapshot};
