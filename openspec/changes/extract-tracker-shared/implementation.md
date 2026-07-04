# Implementation Plan: extract-tracker-shared

> Grounded in current codebase, symbols verified 2026-07-04 against disk (direct reads of `clickup/{writeback,auth,mirror,integration}.rs` + `linear/{writeback,auth,mirror,integration}.rs`). Behaviour (not just symbol existence) verified for the load-bearing claims. Source of rationale: the archived spike `openspec/changes/archive/2026-07-04-issue-tracker-adapter/design.md` Revision 1 (its line refs re-checked, none stale as of this pass).

## Verified codebase facts (do not re-assume)

### Slice 1 — WritebackRegistry
- `WritebackRegistry { entries: Mutex<HashMap<Key, WriteEntry>> }`: `clickup/writeback.rs:90-92` vs `linear/writeback.rs:71-73`, identical shape.
- Methods identical modulo id-naming: `record` (`clickup:105-125`/`linear:90-110`), `entries_for_task`/`entries_for_issue` (`clickup:131-141`/`linear:113-123`), `clear_entry` (`clickup:144-148`/`linear:142-146`), `purge_expired` (`clickup:152-157`/`linear:148-155`), `check_echo` (`clickup:192-215`/`linear:187-204`).
- **Linear-only** `tracked_issue_ids()` (`linear:126-138`) — no ClickUp equivalent (parametrize or add as a no-op-safe method).
- **`WriteConflict` stays per-tracker (iprev MINOR — serde wire)**: it is `Serialize`, emitted as `clickup:write-conflict`/`linear:write-conflict` with divergent id keys (`task_id` `clickup:165` vs `issue_id` `linear:164`). Frontend reads only `.field`/`.remote_value` (`src/stores/clickup.ts:709`, `linear.ts:639`) so a rename is not a runtime break, but it IS a silent wire change → shared `check_echo` returns neutral `(id, field, your, remote)` data, mapped to each tracker's `WriteConflict` at the emit site.
- **`EchoCheckResult` 4th variant (iprev MINOR)**: ClickUp has `AdditiveDivergence` (`clickup:182`) that Linear's 3-variant enum (`linear:174-181`) lacks. Shared enum keeps all 4; Linear's consumer match adds an unreachable arm (its `WriteFieldClass` is all-Scalar).
- **`WRITE_TTL` construction param (iprev MINOR — import cycle)**: derives from each tracker's own `DEFAULT_POLL_INTERVAL_SECS` (`clickup/poller.rs:26` vs `linear/mod.rs:25`). `tracker_shared` must NOT `use super::poller` → `WritebackRegistry<F>` takes the TTL `Duration` as a construction arg.
- **CRITICAL divergence to preserve**: `FieldClass::{Scalar,Additive}` + `WriteField::field_class()` exist ONLY on ClickUp (`clickup/writeback.rs:56-73`). Linear's `WriteField` (`linear:49-54`) has no `field_class()` and `check_echo` is the unconditional Scalar branch (`linear:187-204`); Linear's doc (`linear:44-48`) states the Additive path is "intentionally absent... a scoping decision". → the generic parametrizes over a `WriteFieldClass` trait; Linear impls it returning `Scalar` always, ClickUp returns its real split.
- TTL identical: `WRITE_TTL = Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS * 2)` (`clickup:39`/`linear:41`), each importing its own tracker's constant. `purge_expired` expiry rule identical (`clickup:155`/`linear:153`).
- **OUT OF SCOPE (v1)**: the comment post-once model (`CommentOutcome`/`post_comment`/`verify_comment_landed`/`classify_comment_error`, `clickup:217-358`/`linear:206-336`) is structurally identical BUT diverges in timestamp-tolerance UNIT (`COMMENT_TIMESTAMP_TOLERANCE_MS=5_000` clickup:222 vs `_SECS=5` linear:211) and author-id type (`Option<i64>`+`model::User` vs `Option<&str>`). The spike scoped the registry, not the comment model. Deferred — candidate 5th slice, not extracted here.

### Slice 2 — CredentialStore
- `StoredToken`/`StoredKey` + manual `Debug` redaction (`field(..,&"[redacted]")`): `clickup/auth.rs:25-39` vs `linear/auth.rs:46-60`.
- `fallback_dir`/`fallback_path` identical (`clickup:59-74`/`linear:80-95`).
- `write_fallback_file` (`clickup:190-233`/`linear:270-318`): **atomic temp+rename confirmed** — `create_new(true)` (`clickup:202`/`linear:286`), write+`sync_all()`, `std::fs::rename` (`clickup:228`/`linear:313`). **0600 confirmed** — `opts.mode(0o600)` inside `#[cfg(unix)]` block (`clickup:205-209`/`linear:290-294`), Windows relies on ACL/Credential Manager (documented). These security-sensitive lines must MOVE unchanged, not be rewritten.
- `read_fallback_file` identical, TOML parse errors redacted (`clickup:235-249`/`linear:320-334`).
- **CRITICAL — on-disk serde field name divergence (iprev MAJOR)**: `FallbackFile { token: String }` (`clickup/auth.rs:42-44`) vs `FallbackFile { key: String }` (`linear/auth.rs:63-65`) — these are the literal TOML keys in `~/.config/nergal/{clickup,linear}.toml`. A shared struct with one field name breaks the loser's existing files → forced re-auth on the file-fallback path (keyring unavailable). → shared field `secret` with `#[serde(alias = "token", alias = "key")]`; test BOTH legacy formats through the shared reader. (`StoredToken`/`StoredKey` are in-memory only — not disk serde — unaffected.)
- Keyring core: ClickUp flat `store_token`/`load_token`/`clear_token` (`clickup:92-169`) on one `KEYRING_ACCOUNT`; Linear generic `store_to`/`load_from`/`remove_from` (`linear:141-210`) parametrized by `(account, fallback_path)` with 6 wrappers (per-org `*_key_for` `linear:213-228` + legacy `*_key` `linear:233-249`). Linear's parametrized shape IS the template `CredentialStore` generalizes to (spike D3).
- **CRITICAL — stays OUTSIDE CredentialStore**: `AuthMode` + `authorization_header_value` (`linear/auth.rs:27-43`, OAuth-reserved) and `validate_org_id` (`linear:110-120`, path-traversal guard, tested `linear:428-437`) + `account_for`/`fallback_path_for` (`linear:124-132`) are Linear-only, zero ClickUp equivalent. Wrap CredentialStore, not fields of it (spike `design.md:107-109`).

### Slice 3 — closed-out marker trio
- `mark_closed_out` (`clickup/mirror.rs:781`/`linear/mirror.rs:865`) + `unmark_closed_out` (`clickup:802`/`linear:884`): identical modulo table (`clickup_closed_out(task_id,closed_at)` vs `linear_closed_out(issue_id,closed_at)`).
- **CRITICAL divergence to preserve**: `read_closed_out` — Linear `SELECT issue_id FROM linear_closed_out ORDER BY closed_at` (`linear:876`) HAS `ORDER BY`; ClickUp `SELECT task_id FROM clickup_closed_out` (`clickup:792`) does NOT, and uses a manual for-loop vs Linear's `.collect::<Result<_,_>>()`. → the shared free fn parametrizes ordering (or adds `ORDER BY` to both — verify no caller depends on ClickUp's insertion order; adding a deterministic order is safe).
- Tests consolidate: `closed_out_marker_round_trips_and_is_idempotent`/`unmark_*` (`clickup/mirror.rs:1133,1148`), `closed_out_round_trip`/`unmark_*` (`linear/mirror.rs:1263,1282`).

### Slice 4 — ComposableItem / compose framework — DESCOPED (iprev round 1, MAJOR/ROI)
> Cut from this change to a follow-up. `fit_to_budget` is control-flow-divergent (ClickUp 4 stages incl. checklist-collapse `clickup/integration.rs:361-401`; Linear 3 stages, no checklist `linear:338-374`) → a `ComposableItem` trait supplying tracker-specific collapse stages is the leaky-trait anti-pattern the spike rejected. Facts retained below for the follow-up; `integration.rs` is NOT touched by this change.
- `compose_task_markdown` (`clickup/integration.rs:43`)/`compose_issue_markdown` (`linear:76`); `assemble_clickup_context` (`clickup:52`)/`assemble_linear_context` (`linear:85`).
- Constants EQUAL: `CONTEXT_BUDGET_BYTES = 32*1024` (`clickup:16`/`linear:18`), `MAX_COMMENTS = 20` (`clickup:19`/`linear:21`). Linear doc "mirrors clickup/integration.rs" (`linear:3-4`).
- **CRITICAL divergence**: `fit_to_budget` (`clickup:355`/`linear:332`) bodies diverge beyond field renaming — ClickUp has an extra attrition stage (`collapsed_checklists` loop `clickup:379-389`) with no Linear counterpart; final `tracing::warn!` field lists differ. This is a CONTROL-FLOW difference, not just data shape → `compose_markdown<T: ComposableItem>` needs `ComposedSection` variants that let a tracker skip the checklist-collapse stage.
- Field sets NOT 1:1: shared `heading/description/comments/comments_omitted`; ClickUp-only `subtasks(_collapsed)/checklists(_collapsed)/custom_fields/attachments`; Linear-only `metadata/labels` (`clickup:97-109`/`linear:130-140`). `RenderedComment.date` (ClickUp) vs `.created_at` (Linear).

### Sequencing fact
- ClickUp's writeback `record`-before-`await` fix (`clickup-writeback-echo-ordering`) is ARCHIVED + PASSED (`archive/2026-07-04-clickup-writeback-echo-ordering/handoff/REVIEW.md`); confirmed in code `clickup/mod.rs:537 record` before `:543 .await`, `clear_entry` on Err (`:546`), same at `:563,616,639,648,712`. → migrating ClickUp writeback does NOT fight an in-flight fix; the spike's "after echo-ordering settles" precondition is already met.

## Execution order

Three slices, each its own commit, full gates (`clippy --all-targets -- -D warnings` + `cargo test` + `cargo fmt --check`) green between each. Build the shared type against Linear's shape first, then migrate ClickUp onto it. NO trait results — two generic structs + free functions.

1. **`tracker_shared/writeback_registry.rs`** — `WritebackRegistry<F>` + `WriteFieldClass` trait + `WriteEntry`/`check_echo` (returns neutral data; `WriteConflict` stays per-tracker) + `EchoCheckResult` (4 variants) + TTL as construction param. Migrate Linear, then ClickUp (ClickUp supplies real `FieldClass`). Consolidate both registry test modules.
2. **`tracker_shared/closed_out.rs`** — the trio as free fns parametrized by table+id-column; add deterministic `ORDER BY closed_at` to both (consumer verified order-insensitive). Migrate both mirror.rs call sites.
3. **`tracker_shared/credential_store.rs`** — `CredentialStore` generic over service/account/filename; MOVE the 0600/atomic-rename/redaction lines verbatim; shared fallback field `secret` with `#[serde(alias = "token", alias = "key")]` + a legacy-format test per tracker. `AuthMode`/multi-workspace stay as Linear wrappers. **security review here.**

(Slice 4 compose — descoped, see above.)

## Plan (per slice)

- Each slice: create `tracker_shared/<slice>.rs` with the generic/trait built against Linear's shape + unit tests (consolidated from the two existing test modules); add `pub mod <slice>;` to a new `tracker_shared/mod.rs` wired into `lib.rs`; rewire Linear's call sites to the shared type and delete Linear's now-dead copy; run gates; rewire ClickUp's call sites and delete ClickUp's copy; run gates.
- `lib.rs` wiring: add `mod tracker_shared;` (verify against the existing module list; keep the diff minimal — one line).

## Per-phase risk

- [Behavior drift when two near-identical bodies merge] → migrate ONE slice, ONE tracker at a time; gates green between; the consolidated test must assert what BOTH per-tracker suites asserted. → Mitigation is the per-slice commit cadence + iprev on this plan.
- [Slice 3 `read_closed_out` ordering change alters a caller's assumption] → grep callers of `read_closed_out` on both sides; confirm none depend on ClickUp's unordered/insertion order before adding `ORDER BY`. → concrete check before slice 2.
- [CredentialStore fallback serde field merge drops a stored credential (iprev MAJOR)] → shared field `secret` with `#[serde(alias = "token", alias = "key")]`; a test per tracker deserializing its legacy on-disk format through the shared reader; this line is on the slice-3 security-review checklist.
- [CredentialStore security regression — 0600/atomic-rename/redaction weakened] → MOVE verbatim, security review the diff line-by-line, migrate it third (after the pattern is proven on slices 1-2). Watch the `#[cfg(unix)]`/`#[cfg(not(unix))]` gating survives the file move (Windows path).
- [Keyring `(service, account)` constant drift → silent credential loss on the keyring path (iprev round 2, bigger blast radius than the file fallback)] → pass `service="nergal"`, `account`∈{`clickup-token`,`linear-token`}, `filename`∈{`clickup.toml`,`linear.toml`} BYTE-IDENTICAL through the constructor; Linear's `"linear-token::{org_id}"` per-org format stays in the wrapper (D3). Test asserts the constructed strings equal the pre-refactor literals.
- [Toolchain skew: local clippy 1.93 misses lints CI 1.96 flags] → expect a possible CI clippy round-trip per slice; fix with clippy's own suggestion + re-push.

## Verification

- Per slice + final: `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check` (via `rtk proxy` for a true exit code) + `npx tsc --noEmit` (unaffected — pure backend).
- Net LOC: confirm ~600-800 line reduction across the 8 files (spike estimate) via `git diff --stat`.
- Behavior parity: the consolidated tests pass with the same assertions the per-tracker suites made; no DB migration added; `git grep` confirms no remaining duplicate copy of an extracted body.
- Manual (user walk): exercise a ClickUp + a Linear writeback/echo path, a keyring store/load, a closed-out round-trip, and a compose-context delivery to confirm no behavior change.
