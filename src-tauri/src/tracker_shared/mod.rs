//! Mechanics extracted from the parallel ClickUp + Linear tracker stacks.
//!
//! Two generic structs / free functions, not a shared trait over the
//! trackers — see `openspec/changes/extract-tracker-shared/design.md` (D1)
//! for why a full `IssueTrackerAdapter` trait was rejected.

pub mod closed_out;
pub mod credential_store;
pub mod writeback_registry;
