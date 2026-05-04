//! Claude Code adapter scaffolding.
//!
//! Submodules carry the existing CC-specific logic (transcript watcher, plan
//! watcher / manager, cost extraction, transcript→tasks parser) moved from
//! the top-level `claude::` and `tasks::transcript_parser` modules. The
//! [`ClaudeCodeAdapter`] wrapper that implements [`super::AgentAdapter`]
//! lands in a follow-up commit; this module exists today only to host the
//! moved files at their new path.

pub mod adapter;
pub mod cost;
pub mod plan;
pub mod tasks_parser;
pub mod transcript;

pub use adapter::ClaudeCodeAdapter;
