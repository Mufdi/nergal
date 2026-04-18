//! VT emulation backend built on `wezterm-term`.
//!
//! Each [`TerminalSession`] owns a wezterm `Terminal` and a PTY writer;
//! the [`TerminalHandle`] wraps it with a [`GridDiffer`] and an async
//! emitter task that ships `terminal:grid-update` events to the frontend.
//! [`CluihudTerminalConfig`] is the [`TerminalConfiguration`] impl — it
//! defaults to Kitty keyboard protocol on and bumps scrollback to 10k
//! rows.

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
