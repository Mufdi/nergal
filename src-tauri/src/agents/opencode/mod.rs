//! OpenCode adapter — wraps `opencode serve` (HTTP+SSE).
//!
//! Unlike CC's file-config hooks, OpenCode runs a long-lived HTTP server
//! per-session and emits SSE events on `/event`. cluihud's adapter:
//! 1. Spawns `opencode serve --port 0` per session via [`server_supervisor`].
//! 2. Parses the chosen port from stdout.
//! 3. Subscribes to `/event` via [`sse_client`] and translates events into
//!    the runtime's `EventSink`.
//! 4. Handles permission prompts by stashing pending requests in a map and
//!    POSTing the user's reply through [`permission_client`].
//!
//! Schema reference: `docs/agents/opencode-sse-schema.md`.

pub mod adapter;
pub mod permission_client;
pub mod server_supervisor;
pub mod sse_client;

pub use adapter::OpenCodeAdapter;
