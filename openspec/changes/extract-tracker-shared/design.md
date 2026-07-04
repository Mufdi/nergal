## Context

ClickUp and Linear ship parallel backend stacks (9 modules each, 15,759 combined LOC) with no shared abstraction. The archived `issue-tracker-adapter` spike measured the duplication byte-for-byte and returned **GO-BUT-NARROWER**: ~2,600-2,900 LOC (~17%) is mechanically shared with zero paper-migration leaks, concentrated in 4 slices; the remaining ~83% (client/model/poller/mirror/closure orchestration) is irreducibly tracker-specific and carries 3 real leaks that a shared contract would either paper over or freeze the in-flight `clickup-writeback-echo-ordering` fix into. This change extracts only the profitable 4 slices. The repo already has a proven trait-based adapter precedent (`AgentAdapter`, `src-tauri/src/agents/`, spec `agent-adapter`) — but the spike's key finding is that most of this duplication wants a **generic struct**, not a trait.

## Goals / Non-Goals

**Goals:**
- Extract the 4 mechanically-shared slices into `src-tauri/src/tracker_shared/`, behavior-preserving, so a fix on genuinely shared mechanics lands once.
- Consolidate the near-parallel per-tracker test coverage for these paths onto the shared code.
- Net reduction ~600-800 LOC.

**Non-Goals:**
- The full 5-layer `IssueTrackerAdapter` trait (spike-rejected).
- Sharing poller/mirror/closure orchestration (3 real leaks — stays per-tracker).
- Frontend panel sharing (component duplication is non-mechanical — different fields/pickers).
- A third tracker.
- Any behavior, DB schema, or public command-surface change.

## Decisions

### D1: two extraction shapes, not one trait

Generic structs where the shared shape has zero tracker-specific escape hatches; a trait only where the divergent part is genuinely per-tracker DATA (not control flow).

- `WritebackRegistry<F: WriteFieldClass>` — generic struct. Absorbs `WriteEntry`, the registry (`record`/`entries_for_*`/`clear_entry`/`purge_expired`/`tracked_ids`), `check_echo`. `F` carries each tracker's write-field class; `WriteFieldClass::field_class() -> FieldClass{Scalar,Additive}` preserves ClickUp's real Additive path (Linear supplies all-Scalar).
- `CredentialStore` — generic struct. Absorbs `StoredToken/Key` shape, `fallback_dir`/`fallback_path`, atomic 0600 `write_fallback_file`/`read_fallback_file`, keyring get/set/delete. Parametrized over service/account/filename strings.
- closed-out marker trio — free functions parametrized by table name (`mark_closed_out`/`read_closed_out`/`unmark_closed_out`, byte-identical today modulo table/column).
- `ComposableItem` trait + `compose_markdown<T>` — the ONE trait, for the compose/budget framework (`fit_to_budget`, fence-sentinel). Per-tracker field sets become `ComposedSection`s.

**Alternatives considered:** the full `IssueTrackerAdapter` trait spanning poller/writeback/mirror/closure/auth (spike D1's original sketch). Rejected: paper-migrating ClickUp onto it surfaced 3 leaks (closure 2-vs-3-halves, echo-check ordering, poller completeness disposition) that force per-tracker overrides on nearly every method (dead trait surface) or freeze an in-flight fix. A config-driven generic tracker was rejected earlier by the spike (auth/pagination semantics differ too much).

### D2: build against Linear's shape first, migrate ClickUp after echo-ordering settles

Linear is the semantic canon (spike D3 — later, deliberately-improved implementation, no known open bugs in its registry). Build the shared module against Linear's shapes, migrate Linear first, then ClickUp. ClickUp's writeback migration lands only after `clickup-writeback-echo-ordering` (archived Wave 2.10) is settled, so no in-flight ordering behavior is frozen mid-extraction.

**Alternatives considered:** ClickUp-first (the original proposal assumption). Rejected — ClickUp is the side with the recently-changed echo path; migrating it onto shared code concurrently with its own fix risks conflating two behavior changes.

### D3: tracker-specific wrappers stay OUTSIDE the shared structs

`AuthMode`/`authorization_header_value` (Linear's OAuth-extensible header seam) and Linear's multi-workspace `validate_org_id`/per-org namespacing have zero ClickUp equivalent — they wrap `CredentialStore`, they are not fields of it. Same for the account-swap side effects (`clear_cached_user` vs `bump_generation_and_wipe`): those stay in each tracker's `mod.rs`, not the shared store. This keeps the shared struct from growing a dead field for the tracker that doesn't need it (the GitHub-Issues paper check confirmed a 3rd tracker would push `AuthMode` wider if it were inside).

### D4: module location + shape

New `src-tauri/src/tracker_shared/` with a submodule per slice (`writeback_registry.rs`, `credential_store.rs`, `closed_out.rs`, `compose.rs`) + `mod.rs`. Consolidated tests live beside each slice, replacing the duplicated per-tracker test modules.

## Risks / Trade-offs

- [Behavior drift during extraction — a subtle semantic changes when two near-identical bodies merge into one] → migrate ONE slice at a time, full gate (clippy+test+fmt) green between each; the consolidated tests must assert the same properties both per-tracker suites asserted before. iprev on the plan before building.
- [Keyring/security regression — the `Debug` redaction, atomic temp+rename, 0600 perms silently weaken in the merge] → `CredentialStore` phase carries a dedicated security review; the extraction must be a move, not a rewrite, of the security-sensitive lines. Migrate it THIRD (after the pattern is proven on the two lowest-risk slices).
- [Freezing the in-flight ClickUp echo-ordering fix] → D2 sequencing: ClickUp writeback migrates only after that change settles.
- [Over-fitting the generic to 2 samples] → the spike's GitHub-Issues paper check already validated each shape against a 3rd sample; keep the Additive/Scalar split (the quieter sample's more-general shape).

## Migration Plan

Per-slice, each its own reviewable commit, gates green between:
1. `WritebackRegistry<F>` (Linear → ClickUp, ClickUp only after echo-ordering settles) — self-contained, best existing test coverage, forecloses echo-ordering-class drift.
2. closed-out marker trio — trivial, 3 fns, lowest risk.
3. `CredentialStore` (security-reviewed) — keyring/fallback core, `AuthMode`/multi-workspace stay as wrappers.
4. `ComposableItem` compose framework — lowest urgency, mostly cosmetic duplication.

Rollback: each slice is an independent commit; revert the offending slice without unwinding the others.

## Open Questions

- Do the four openspec capabilities per tracker (`clickup-{mirror,task-panel,writeback,agent-integration}` + Linear equivalents) collapse into shared + per-tracker delta specs post-extraction? Deferred — spec-architecture question for a follow-up, not blocking the code extraction.
- Whether the closure 2-vs-3-halves shape should converge (make ClickUp behave like Linear's `close_out`-discriminated flow) — the spike flagged this as a product parity decision, filed separately, NOT part of this mechanical extraction.

## Revision 1: iprev round 1

The plan-review gate (opus evaluator) spot-checked all four divergences against disk (all true) and returned REVISE with 2 MAJOR + 3 MINOR findings. Folded in:

- **Drop slice 4 (`ComposableItem`/compose) — descoped to a follow-up (MAJOR/ROI).** The `fit_to_budget` divergence is control-flow, not data: ClickUp runs comments → **collapse checklists** → collapse subtasks → truncate (`clickup/integration.rs:361-401`); Linear runs comments → collapse subissues → truncate (3 stages, no checklist, `linear/integration.rs:338-374`), plus `RenderedComment.date` vs `.created_at`. A `ComposableItem` trait supplying an ordered list of tracker-specific collapse stages IS the leaky-trait anti-pattern the spike rejected for poller/mirror. Cutting it now avoids a predictable build+revert cycle on the lowest-payoff slice (~90 LOC cosmetic). **Consequence: the extraction now has NO trait at all — two generic structs (`WritebackRegistry<F>`, `CredentialStore`) + free functions (closed-out).** `ComposableItem` in D1 is withdrawn.

- **CredentialStore fallback field serde-alias (MAJOR — silent credential loss).** The on-disk fallback structs use different TOML keys: `FallbackFile { token }` (`clickup/auth.rs:42-44`) vs `FallbackFile { key }` (`linear/auth.rs:63-65`) — the literal keys in `~/.config/nergal/{clickup,linear}.toml`. A single shared struct with one field name breaks the loser's existing files → `read_fallback_file` returns the redacted-malformed Err → credential treated as absent → forced re-auth (hits the file-fallback path: keyring/Secret-Service unavailable, headless/server Linux). Same class as the `cluihud.db`→`nergal.db` migration gotcha. **Decision:** name the shared field `secret` with `#[serde(alias = "token", alias = "key")]`; both legacy files parse, new writes converge. In-memory `StoredToken`/`StoredKey` are not serde-to-disk → unaffected. A slice-3 test deserializes BOTH a legacy `token = "…"` and a legacy `key = "…"` file through the shared reader; the security review checklist gains this line.

- **`WriteConflict` stays per-tracker (MINOR — serde wire).** It is `Serialize`, emitted as `clickup:write-conflict`/`linear:write-conflict` with divergent id keys (`task_id` vs `issue_id`). Verified the frontend handlers (`src/stores/clickup.ts:709`, `linear.ts:639`) read only `.field`/`.remote_value` — so a rename is not a runtime break, but it IS a silent wire change. **Decision:** keep `WriteConflict` per-tracker; shared `check_echo` returns neutral `(id, field, your, remote)` data mapped to each tracker's struct at the emit site — fully behavior-preserving, and keeps `EchoCheckResult` cleanly shareable.

- **`EchoCheckResult` unified enum (MINOR).** ClickUp carries a 4th variant `AdditiveDivergence` (`clickup/writeback.rs:182`) Linear lacks. The shared enum keeps all 4; Linear's consumer match adds an unreachable arm (Linear's `WriteFieldClass` returns `Scalar` always, so it never constructs the additive variant). Called out so it is not a build surprise.

- **`WRITE_TTL` as a construction param (MINOR — import cycle).** `WRITE_TTL` derives from each tracker's own `DEFAULT_POLL_INTERVAL_SECS` (`clickup/poller.rs:26` vs `linear/mod.rs:25`, both 45). `tracker_shared` must NOT `use super::poller::…` (cycle). **Decision:** `WritebackRegistry<F>` takes the TTL `Duration` as a construction argument; each tracker passes its own.

- **CLEARED:** `read_closed_out` ORDER BY is safe to unify — the consumers use the result as an order-insensitive membership set (`src/stores/clickup.ts:634-641`, `linear.ts:564-567`). Adding `ORDER BY closed_at` to ClickUp is behavior-preserving.

**Revised scope: slices 1 (WritebackRegistry) + 2 (closed-out) + 3 (CredentialStore). Slice 4 (compose) deferred.**

## Revision 2: iprev round 2

Round 2 returned CONFIRM (proceed to build) with one residual MINOR + one clarification, both folded as checklist/doc additions (no plan restructure):

- **Keyring `(service, account)` constant pass-through (MINOR — same class as the FallbackFile MAJOR, on the more common keyring path).** `KEYRING_ACCOUNT` (`clickup-token`/`linear-token`) and `FALLBACK_FILE` (`clickup.toml`/`linear.toml`) are per-tracker (`clickup/auth.rs:20-22`, `linear/auth.rs:19-21`); the keyring lookup key is `(service="nergal", account)`. If a migration "tidies" the account name, every existing user's stored token goes invisible on the keyring path (bigger blast radius than the file fallback), silent, forced re-auth — the `cluihud→nergal` keyring gotcha class. **Decision:** `CredentialStore` is constructed with these constants passed BYTE-IDENTICAL; Linear's `"linear-token::{org_id}"` per-org format stays produced by the wrapper (outside the struct, per D3); a slice-3 test asserts the constructed service/account strings equal the pre-refactor literals. (Task 4.3b.)

- **Downgrade is a non-goal (clarification).** After upgrade the fallback file is written with the canonical `secret` key; an OLD build's reader (no `serde(alias)`) would fail to parse it → forced re-auth on the file-fallback path only (keyring path unaffected, non-destructive re-prompt). For a forward-only auto-updater desktop app this is acceptable and explicitly out of scope — stated here so it is not a surprise.

**Plan CONFIRMED for build. Scope: slices 1 + 2 + 3.**
