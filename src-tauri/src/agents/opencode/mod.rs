//! OpenCode adapter — runs the `opencode` TUI inside the cluihud terminal.
//!
//! Earlier versions wrapped `opencode serve` (HTTP+SSE) and routed the UI
//! through a chat panel; that's been removed in favor of the terminal flow
//! shared with CC/Pi/Codex. See `adapter` for the current implementation.

pub mod adapter;
pub mod sse_client;

pub use adapter::OpenCodeAdapter;
