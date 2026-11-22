//! VT emulation backend built on `wezterm-term`.
//!
//! Each [`TerminalSession`] owns a wezterm `Terminal` and a PTY writer;
//! the [`TerminalHandle`] wraps it with a [`GridDiffer`] and an async
//! emitter task that ships grid updates to the frontend over a per-terminal channel.
//! [`NergalTerminalConfig`] is the [`TerminalConfiguration`] impl — it
//! leaves Kitty keyboard protocol unadvertised and bumps scrollback to 10k
//! rows.

#![allow(dead_code, unused_imports)]

pub(crate) mod config;
mod differ;
mod emitter;
mod input;
pub(crate) mod session;
mod types;

#[cfg(test)]
#[allow(clippy::type_complexity)]
mod pixel_geometry_tests;
#[cfg(test)]
mod sixel_zero_pixel_tests;

pub use config::NergalTerminalConfig;
pub use differ::GridDiffer;
pub use emitter::TerminalHandle;
pub use input::map_event;
pub use session::{MouseMods, PrimaryButton, PrimaryKind, TerminalSession};
pub use types::{
    CellSnapshot, CursorSnapshot, GridRow, GridSnapshot, GridUpdate, TerminalKeyEvent,
};
