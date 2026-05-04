//! Pi adapter — wraps Pi (`@mariozechner/pi-coding-agent`) over a JSONL tail.
//!
//! Pi has neither file-config hooks (CC) nor an HTTP server (OpenCode).
//! Its model is "drop a JSONL transcript per session into
//! `~/.pi/agent/sessions/--<encoded-cwd>--/<uuid>.jsonl` while the agent
//! runs". cluihud observes those files via [`jsonl_tail`] and translates
//! entries to [`crate::agents::TranscriptEvent`] via [`transcript`].
//!
//! Pi is observation-only by design: there is no plan-mode equivalent,
//! no permission prompt, no task-list integration. Capabilities reflect
//! that — see [`PiAdapter::capabilities`].
//!
//! Schema reference: when a Pi binary is available, run a session and
//! capture a fixture under `src-tauri/tests/fixtures/pi/` to validate
//! the parser against. The current parser maps the documented entry
//! types (`session`, `agent`, `tool_call`, `tool_result`); unknown
//! types are silently ignored.

pub mod adapter;
pub mod jsonl_tail;
pub mod session_resolver;
pub mod transcript;

pub use adapter::PiAdapter;
