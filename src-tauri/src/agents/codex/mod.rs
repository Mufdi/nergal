//! Codex (OpenAI) adapter — file-config hooks like CC, plus a rollout JSONL
//! tail for cost extraction.
//!
//! Codex shares the [`Transport::FileHooks`] surface with CC: the same
//! Unix socket dispatcher receives hook events, the same hook event names
//! are emitted (PreToolUse, PostToolUse, Stop, …). The adapter mostly
//! supplies:
//! 1. [`setup`] — conservatively merges nergal's hook entries into
//!    `~/.codex/hooks.json` without trampling user-installed hooks.
//! 2. [`transcript`] — parses Codex's rollout JSONL with OpenAI naming
//!    (`prompt_tokens` / `completion_tokens`).
//! 3. [`rollout_resolver`] — finds the newest rollout file after spawn so
//!    the cost extractor knows where to read.
//!
//! Trust gate: Codex tracks per-project trust separately from nergal.
//! [`adapter::CodexAdapter::detect`] surfaces the bit; the UI may render a
//! "trust this project in Codex" banner, which is otherwise a no-op for
//! nergal.

pub mod adapter;
pub mod rollout_resolver;
pub mod rollout_tail;
pub mod setup;
pub mod transcript;

pub use adapter::CodexAdapter;
