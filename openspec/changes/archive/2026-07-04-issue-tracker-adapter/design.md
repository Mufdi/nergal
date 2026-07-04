## Context

Two tracker stacks, mirror-image module lists, no shared code beyond incidental util reuse. The duplication is disciplined (Linear was consciously built "Linear-style" on ClickUp's patterns, then improved on them — e.g. provisional writeback recording that ClickUp still lacks), which means the shapes are *convergent by intent* — favorable conditions for extraction. The precedent is `AgentAdapter` (`agent-adapter` spec): the codebase already did this successfully for agent CLIs (4 adapters behind one trait).

## Goals / Non-Goals

**Goals:**
- A concrete `IssueTrackerAdapter` contract proposal grounded in the two real stacks (not speculative generality).
- A migration path for one tracker + go/no-go recommendation, cheap enough to decide without betting the codebase.

**Non-Goals:**
- Building the adapter now; unifying frontend panels now; committing to a third tracker.

## Decisions

### D1: contract shape — trait over the backend seam, not the UI

Extract where the duplication is mechanical: `poller` (cadence, tombstoning, echo-check loop), `writeback` (registry semantics — provisional record, clear-on-failure, TTL), `mirror` (reconcile lifecycle, stale marking), `closure` (token flow), `auth` (keyring storage shape). Leave per-tracker: `client` (REST vs GraphQL), `model` (field vocabularies), state semantics (cycles/estimates vs lists/priorities). Frontend stays out of scope v1 — component duplication is real but not mechanical (different fields, different pickers).

**Alternatives considered:**
- *Full-stack adapter including UI*: highest payoff, highest risk; UI genericity historically produces lowest-common-denominator panels. Deferred to its own decision after the backend proves out.
- *Do nothing until a third tracker is real*: cheapest now, but the third copy locks in triplication and the extraction cost grows superlinearly (three call-site sets). The spike prices this trade instead of assuming it.
- *Config-driven generic tracker (no per-tracker code)*: trackers differ in auth flows and pagination/rate-limit semantics enough that config-only is fantasy. Rejected.

### D2: spike protocol (time-boxed, ~1 day)

1. **Duplication inventory**: diff the nine module pairs; classify each fn as shared-shape / parametrizable / tracker-specific (table).
2. **Draft the trait** (Rust sketch, no compile requirement): associated types for Task/State/Field, methods for the poller/writeback/mirror hooks.
3. **Paper-migrate ClickUp**: map each ClickUp module onto the trait; list every place the abstraction leaks (e.g. ClickUp checklists have no Linear analog — extension-point or tracker-specific surface?).
4. **Go/no-go**: extraction cost estimate vs. measured duplication + expected third-tracker cost. Append findings + recommendation to this design.md (Revision section per project openspec rules).

### D3: Linear is the semantic canon

Where the two stacks disagree on a pattern (echo recording order, overlay reconcile), the adapter contract adopts Linear's version — it is the later, deliberately-improved implementation (project memory + the pending ClickUp echo-ordering fix confirm the direction of improvement).

## Risks / Trade-offs

- [Design concludes no-go and the work feels wasted] → the duplication inventory itself becomes the checklist for keeping the stacks manually in sync (directly useful — the echo-ordering bug would have been caught by it).
- [Trait designed against 2 samples over-fits] → require the paper-migration step to name, for each trait method, what GitHub Issues would plausibly do (a cheap third sample on paper, no code).
- [Spike scope creeps into refactor] → hard rule: no source edits under this change; anything compiling is out of scope.

## Open Questions

- Which tracker migrates first if "go" (D2 assumes ClickUp; the spike validates).
- Whether the four openspec capabilities per tracker collapse into shared + per-tracker delta specs post-extraction (spec architecture question for the follow-up).

## Revision 1: spike findings

Method: every classification below is grounded in a direct read of both trees — `src-tauri/src/clickup/{auth,client,closure,integration,mirror,mod,model,poller,writeback}.rs` (15,759 combined LOC across both trackers) against `src-tauri/src/linear/` counterparts. No source files were edited; this section is the only change to the repo.

### 1.1 Duplication inventory

Legend: **SHARED-SHAPE** = same signature + logic modulo tracker names, extract as-is. **PARAMETRIZABLE** = same shape, differs only in a value/endpoint/field-set, extract with a param or associated type. **TRACKER-SPECIFIC** = genuinely divergent, stays per-tracker.

| Module pair | LOC (CU/Lin) | Classification | Notes with citations |
|---|---|---|---|
| `auth.rs` | 331 / 438 | **SHARED-SHAPE** (~250 LOC core): `StoredToken`/`StoredKey` (manual `Debug` redaction), `fallback_dir`/`fallback_path`, `write_fallback_file`/`read_fallback_file` (atomic 0600 temp+rename), keyring get/set/delete wrappers. Near-verbatim `s/token/key/` — compare `clickup/auth.rs:190-233` to `linear/auth.rs:270-318`. **PARAMETRIZABLE**: per-org account/file wrappers (`linear/auth.rs:212-227` `store_key_for`/`load_key_for`/`remove_key_for`) — same shape as the legacy single-account fns, just namespaced. **TRACKER-SPECIFIC**: `AuthMode` enum + `authorization_header_value` (`linear/auth.rs:27-43`, OAuth-extensible header seam) and `validate_org_id` (`linear/auth.rs:110-120`, path-traversal guard for multi-workspace) — both have **zero ClickUp equivalent**; ClickUp has no OAuth mode and no multi-workspace. |
| `writeback.rs` | 768 / 527 | **SHARED-SHAPE** (~500 LOC): `WriteEntry`, `WritebackRegistry` (`record`/`entries_for_*`/`clear_entry`/`purge_expired`), `WriteConflict`, `CommentOutcome`, `post_comment`, `verify_comment_landed`, `classify_comment_error` — compare `clickup/writeback.rs:90-158` to `linear/writeback.rs:71-156` (registry) and `clickup/writeback.rs:224-358` to `linear/writeback.rs:216-336` (comment model). **PARAMETRIZABLE**: `WriteField` enum + `check_echo`'s scalar branch (`clickup/writeback.rs:192-215` vs `linear/writeback.rs:187-204`). **TRACKER-SPECIFIC leak**: `FieldClass::{Scalar,Additive}` (`clickup/writeback.rs:56-73`) has no Linear equivalent — Linear's design doc says so explicitly: "the ClickUp additive path is intentionally absent here as a scoping decision" (`linear/writeback.rs:47-48`). Linear also adds `tracked_issue_ids()` (`linear/writeback.rs:126-138`), unused by ClickUp, because the two echo-check call sites are structurally inverted (see the poller row). |
| `closure.rs` | 619 / 668 | **SHARED-SHAPE** (~180 LOC): `TOKEN_TTL`, `{Closure,Gated}TokenStore` (`issue`/`take`/`insert_for_test`), the ZWS-mention-neutralizing sanitizer shape. Compare `clickup/closure.rs:56-142` to `linear/closure.rs:64-158`. **PARAMETRIZABLE**: `sanitize_comment_text` differs by one char class — ClickUp neutralizes `@word` AND `#word` (task-ref syntax, `clickup/closure.rs:161-183`); Linear neutralizes `@word` only (`linear/closure.rs:172-189`, no task-ref syntax in Linear). **TRACKER-SPECIFIC leak (real one)**: ClickUp issues ONE token command covering status+comment (`clickup_request_closure_token`, `clickup/closure.rs:219-255`) and the local "worked-closed" marker is a SEPARATE Tauri command called by the frontend after execution (`clickup_mark_closed_out`, `mod.rs:342-353`). Linear issues TWO token commands discriminated by a `close_out: bool` baked into the token (`linear_request_comment_token` at `linear/closure.rs:221-236` vs `linear_request_closure_token` at `linear/closure.rs:246-276`), and the local close-out (unbind + durable marker) is a THIRD action performed **inline inside** `linear_execute_gated_write` (`linear/closure.rs:409-439`), gated by `close_out`. This is a genuine shape mismatch, not a naming difference: ClickUp's closure flow is 2 request-shapes × 2 write-halves; Linear's is 1 request-shape (with a boolean discriminant) × 2 write-halves × 1 conditional local action. |
| `mirror.rs` | 1230 / 1369 | **SHARED-SHAPE** (~60 LOC, the cleanest hit in the whole inventory): `mark_closed_out`/`read_closed_out`/`unmark_closed_out` are essentially byte-identical function bodies modulo table/column names — `clickup/mirror.rs:781-802` vs `linear/mirror.rs:865-884`. **PARAMETRIZABLE**: the `SyncState` "get/set sync state" concept exists on both sides but with divergent fields. **TRACKER-SPECIFIC** (the bulk, ~1,100/1,300 LOC each): ClickUp's 4-level hierarchy (space→folder→list→task, `upsert_space`/`upsert_folder`/`upsert_list`/`upsert_status`/`upsert_task`) has no structural analog to Linear's flatter team/state/cycle/project model. Linear alone carries the account-swap **epoch/generation guard** (`current_key_generation`/`bump_generation_and_wipe`/`wipe_mirror`, `linear/mirror.rs:501-538`) — ClickUp's token-rotation equivalent is a single `clear_cached_user` field-clear (`clickup/mod.rs:32-38`), an order of magnitude simpler. Linear's tombstone/eviction model is a 4-way disposition (`Set3Action::{RetainMine,ClearFlag,ClearAndEvict,RetainUnchanged}`, `linear/poller.rs:63-107`) driven by its bounded-window+viewer-assigned+delta-reverify design; ClickUp's is a simpler "space-scoped complete fetch, absence = gone" rule (`clickup/poller.rs:557-576`). These are not the same shape at any level of abstraction above "a SQL upsert exists." |
| `poller.rs` | 2132 (incl. tests) / 727 (incl. tests) | **SHARED-SHAPE** (~150 LOC): the `restart`/`set_status`/`note_*`/`poll_interval`/sleep-loop wrapper (`clickup/poller.rs:1017-1092` vs the equivalent in `linear/mod.rs` — Linear inlines this into `mod.rs` rather than `poller.rs`, itself a naming/module-boundary divergence). **PARAMETRIZABLE at the signature level, TRACKER-SPECIFIC internally**: `fetch_cycle`/`reconcile` — ClickUp iterates **per-team** (`clickup/poller.rs:1172-1234` `poll_once` loops `for team_id in &selected`, calling `fetch_cycle`+`reconcile_team` once per team); Linear does **one fetch across all selected teams** merging three id-sets (window ∪ viewer-assigned ∪ delta-reverify, `linear/poller.rs:351-468` `run_cycle`). Different enough that an opaque `FetchedCycle` associated type is the right boundary, but nothing inside the fetch/reconcile pair is shareable. **TRACKER-SPECIFIC ordering leak** (the most consequential finding of this spike): ClickUp's echo/conflict check (`run_echo_check`) runs **inside** the reconcile transaction, before `tx.commit()` (`clickup/poller.rs:240-246`). Linear's echo/conflict hook runs **after** `run_cycle` returns, in `mod.rs`'s post-processing, explicitly reading "the post-reconcile mirror (server truth after blind upsert)" (`linear/mod.rs:1358-1417`, comment at 1358-1361). This is not a stylistic difference — it is literally the class of bug the proposal cites as motivation: the pending `clickup-writeback-echo-ordering` change exists because ClickUp lacked a pattern Linear already had (proposal.md:3). A trait method boundary drawn here today would either freeze ClickUp's not-yet-fixed ordering into the contract, or force Linear's ordering onto ClickUp before that fix lands independently — bad timing to lock in either. |
| `client.rs` | 759 / 1030 | **TRACKER-SPECIFIC** (nearly all of it): ClickUp is REST + API-key header + `last_page` pagination (`clickup/client.rs:157-216`); Linear is GraphQL + POST body + Relay-style cursor/`hasNextPage` pagination (`linear/client.rs:256-324`). The verb sets don't map 1:1 either (`get_user` vs `get_viewer`; ClickUp has 4 discrete setters `set_task_status`/`set_checklist_item`/`update_task`/`set_custom_field` vs Linear's single `issue_update` taking an `IssueUpdateInput` struct). **PARAMETRIZABLE in spirit only**: the "build a client with a timeout + a test-only base-url override" scaffolding (`clickup/client.rs:49-54` vs `linear/client.rs:96` `new`) is conceptually the same but implemented per-protocol; not worth a trait method on its own. |
| `model.rs` | 535 / 424 | **TRACKER-SPECIFIC**, 100% — expected, matches design D1. Zero shared types beyond incidental primitives (`String`, `i64`). |
| `integration.rs` | 771 / 712 | **SHARED-SHAPE** (~90 LOC, second-cleanest hit): `compose_task_markdown`/`compose_issue_markdown` and `assemble_clickup_context`/`assemble_linear_context` are near byte-identical control flow (collect active ∪ pinned ids, dedupe, degrade errors to `None` with a log) — `clickup/integration.rs:43-88` vs `linear/integration.rs:76-121`. Even the constants match exactly: `CONTEXT_BUDGET_BYTES = 32 * 1024` and `MAX_COMMENTS = 20` on both sides, and the fence-sentinel-neutralization trick is identical. Linear's docstring says outright: "mirrors `clickup/integration.rs`, which settled on the trusted-team stance" (`linear/integration.rs:3-4`) — this is **intentional convergence**, the strongest evidence in the whole codebase that extraction is safe here. **PARAMETRIZABLE**: the `ComposedTask`/`ComposedIssue` field sets and the fence header text differ per tracker but the composition/budget-fitting algorithm (`fit_to_budget`) is the same shape. |
| `mod.rs` | 1124 / 1595 | **SHARED-SHAPE**: the session-binding + issue-to-agent verb block — `bind`/`unbind`/`pin`/`unpin`/`compose_*_for_delivery`/`compose_*_prompt`/`send_*_as_prompt`/`reinject_*`/`spawn_worktree_with_*` — is a near-identical ~230-line block on each side (`clickup/mod.rs:727-962` vs `linear/mod.rs:353-580`); the closed-out marker trio duplicates the `mirror.rs` finding at the command layer too. **PARAMETRIZABLE**: token/key set-clear-validate-sync_status-sync_now (6 command pairs) share shape but diverge on the account-swap side-effect (`clear_cached_user` vs `bump_generation_and_wipe`, see the `mirror.rs` row) and on cardinality (`clickup_select_team(team_id: String)` vs `linear_select_teams(team_ids: Vec<String>)`). **TRACKER-SPECIFIC**: the entire multi-workspace block (`linear/mod.rs:193-326`, ~8 fns) has no ClickUp analog at all; `CustomFieldValue`/`is_writable_field_type`/`validate_status_for_task` (`clickup/mod.rs:424-496`) are ClickUp-only; Linear's activity-feed normalization (`normalize_activity`/`priority_word`/`estimate_str`/`cycle_label`, `linear/mod.rs:699-905`) and `linear_fetch_image` (CDN proxy, `linear/mod.rs:582-600`) are Linear-only. |

**Rollup**: of ~15,759 combined LOC, the SHARED-SHAPE + cleanly-PARAMETRIZABLE surface is roughly **2,600-2,900 LOC (~17%)** — concentrated in `writeback.rs` (registry + comment model, ~500 LOC), `auth.rs` (fallback-file + keyring core, ~250 LOC), `mod.rs` (session-verb block + token/closed-out commands, ~500 LOC combined), `closure.rs` (token store + sanitizer shape, ~180 LOC), `integration.rs` (compose/budget framework, ~90 LOC), and `mirror.rs`'s closed-out trio (~60 LOC). The remaining ~83% — all of `model.rs`, nearly all of `client.rs`, the bulk of `mirror.rs`/`poller.rs`, and the multi-workspace/activity-feed/custom-field surfaces in `mod.rs` — is irreducibly tracker-specific, matching D1's prediction but at a lower shared-fraction than D1's framing implied (D1 named poller/writeback/mirror/closure/auth as the extraction target wholesale; this inventory shows real leaks inside 3 of those 5).

### 1.2 Trait sketch

Two extraction shapes emerged, not one. Where the shared shape has **zero** tracker-specific escape hatches, a plain **generic struct** is the right tool (no trait needed). Where tracker-specific data threads through, a **trait with associated types** is appropriate. Forcing everything through one trait — which is what D1 proposed — is exactly the move this spike argues against (see 1.4).

```rust
// ── Generic struct extraction (no trait): the write-back registry ──
//
// Absorbs: WriteEntry, WritebackRegistry::{record, entries_for_*, clear_entry,
// purge_expired, tracked_issue_ids}, WriteConflict, EchoCheckResult, check_echo.
// Parametrized over the tracker's WriteField enum + its FieldClass mapping —
// ClickUp supplies FieldClass::Additive for some variants, Linear supplies
// FieldClass::Scalar for all (a real, not vestigial, use of the parameter).
trait WriteFieldClass: Clone + Eq + std::hash::Hash + Send {
    fn field_class(&self) -> FieldClass; // Scalar | Additive
}

struct WritebackRegistry<F: WriteFieldClass> { entries: Mutex<HashMap<(String, F), WriteEntry<F>>> }
impl<F: WriteFieldClass> WritebackRegistry<F> {
    fn record(&self, id: impl Into<String>, field: F, written: impl Into<String>, pre: Option<impl Into<String>>);
    fn entries_for(&self, id: &str) -> Vec<WriteEntry<F>>;
    fn tracked_ids(&self) -> Vec<String>;
    fn clear_entry(&self, id: &str, field: &F);
    fn purge_expired(&self);
}
fn check_echo<F: WriteFieldClass>(entry: &WriteEntry<F>, server_value: &str) -> EchoCheckResult; // absorbs clickup/writeback.rs:192-215 + linear/writeback.rs:187-204

// ── Generic struct extraction (no trait): the credential store ──
//
// Absorbs: StoredToken/StoredKey shape, fallback_dir/fallback_path,
// write_fallback_file/read_fallback_file, keyring get/set/delete wrappers.
// Parametrized over service/account/filename strings (already how Linear's
// per-org variant works internally — clickup/auth.rs:190-233, linear/auth.rs:270-334).
struct CredentialStore { keyring_account: &'static str, fallback_filename: &'static str }
impl CredentialStore {
    fn store(&self, secret: &str) -> anyhow::Result<bool>;   // -> on_disk
    fn load(&self) -> anyhow::Result<Option<StoredSecret>>;
    fn clear(&self) -> anyhow::Result<()>;
}
// AuthMode/authorization_header_value and validate_org_id/per-org namespacing
// stay OUTSIDE this struct — see leak #1/#2 in 1.3. They would be a
// tracker-specific wrapper AROUND CredentialStore, not a struct field.

// ── Trait extraction: only for the panel-data / compose surface ──
//
// Absorbs: compose_task_markdown/compose_issue_markdown, assemble_*_context,
// the fence-sentinel + budget-fitting algorithm. This is the ONE slice where
// a trait earns its keep, because the divergent part (field sets, fence text)
// is genuinely per-tracker DATA, not per-tracker CONTROL FLOW.
trait ComposableItem {
    fn fence_open(&self) -> &'static str;
    fn fence_close(&self) -> &'static str;
    fn sections(&self) -> Vec<ComposedSection>; // heading/description/comments/etc — tracker fills in what it has
}
fn compose_markdown<T: ComposableItem>(items: &[T], budget: usize) -> String; // absorbs fit_to_budget + fence logic, shared once

// ── Explicitly NOT sketched as one trait: poller / mirror / closure ──
//
// The spike's paper-migration (1.3) found these three have real per-tracker
// leaks (echo-check ordering, tombstone/completeness model, 2-vs-3-halves
// closure shape) that a single IssueTrackerAdapter trait would either paper
// over with tracker-specific overrides on nearly every method (dead trait
// surface) or freeze a not-yet-settled behavior (the echo-ordering fix still
// in flight for ClickUp) into a cross-tracker contract. See 1.4.
```

Mapping back to the 1.1 classification: the `WritebackRegistry<F>` generic absorbs essentially all of the writeback.rs SHARED-SHAPE row (~500 LOC) plus the PARAMETRIZABLE `WriteField`/`FieldClass` row, with the Additive/Scalar split preserved as real behavior rather than dropped. `CredentialStore` absorbs the auth.rs SHARED-SHAPE row (~250 LOC); `AuthMode` and the per-org namespacing stay outside it as tracker-specific wrappers. `ComposableItem` absorbs the integration.rs SHARED-SHAPE + PARAMETRIZABLE rows (~90 LOC) plus the closed-out marker trio's shape (3 near-identical SQL-templated fns, extractable as free functions parametrized by table name rather than a trait). Nothing is sketched for poller.rs/mirror.rs/client.rs/model.rs/closure.rs's command-orchestration layer — those stay per-tracker per the 1.3 findings.

### 1.3 Paper-migrate ClickUp

Walking ClickUp onto the two extraction shapes above:

- **`WritebackRegistry<WriteField>`**: clean migration. ClickUp's `WriteField` already has `field_class()`; implementing `WriteFieldClass` for it is a rename. No leak.
- **`CredentialStore`**: clean migration for the base store/load/clear. ClickUp has no `AuthMode` and no per-org namespacing today, so it would construct `CredentialStore { keyring_account: "clickup-token", fallback_filename: "clickup.toml" }` and stop there — the struct's shape doesn't force it to grow the fields it doesn't need. No leak.
- **`ComposableItem`**: clean migration for the compose/budget framework. ClickUp's checklists/custom-fields become extra `ComposedSection` variants Linear simply never populates. No leak, and this is the one place genericity buys real future value (a GitHub Issues adapter's checklists-analog, if any, would slot in the same way).

**Abstraction leaks found** (resolution stated for each):

1. **Closure 2-vs-3-halves shape** (`clickup/closure.rs:219-255` + `mod.rs:342-353` vs `linear/closure.rs:221-276` + `409-439`). ClickUp: 1 token-request command, 1 separate frontend-driven "mark closed" command. Linear: 2 token-request commands discriminated by `close_out: bool`, with the local close-out folded into the execute path. **Resolution: extension point, not a shared trait method.** Forcing ClickUp onto Linear's `close_out`-discriminated single-request shape is a legitimate improvement (it would fix a real UX inconsistency — a plain ClickUp comment currently has no "don't mark closed" path the way Linear's does), but that is a **product decision to make ClickUp behave like Linear**, not a mechanical extraction. Recommend filing it as a follow-up bug/parity item independent of this spike, not folding it into the trait design.
2. **Echo-check ordering** (`clickup/poller.rs:240-246` inside-transaction vs `linear/mod.rs:1358-1417` after-commit). **Resolution: tracker-specific, stays outside any shared surface.** This is the exact bug class the pending `clickup-writeback-echo-ordering` change is fixing right now for ClickUp independently. Committing to a trait method boundary here today would force a choice between freezing ClickUp's currently-wrong ordering into the contract, or blocking this spike on that change landing first. Correct call: let that change land and settle ClickUp's ordering on its own merits; revisit whether the two orderings converge only after both are independently correct.
3. **Multi-workspace + epoch/generation guard** (`linear/mod.rs:193-326`, `linear/mirror.rs:501-538`) is 100% Linear-only with no ClickUp counterpart to migrate — not a leak in the sense of "ClickUp needs it and lacks it," but a reminder that **any trait method touching account-swap semantics would need a default no-op arm for ClickUp forever**, which is exactly the "dead trait surface" failure mode. **Resolution: excluded from any shared surface entirely**; ClickUp's simpler `clear_cached_user` stays a local implementation detail.
4. **Additive vs scalar field semantics** (`clickup/writeback.rs:56-73` `FieldClass` vs Linear's all-scalar `WriteField`). **Resolution: kept in the generic (`WriteFieldClass` trait method above), not dropped.** This is the one leak that actually validates genericity — see the GitHub Issues check below.
5. **Poller fetch/reconcile completeness model** (per-team-complete-fetch vs window+viewer-assigned+delta-reverify-with-4-way-disposition). **Resolution: stays fully tracker-specific behind an opaque `FetchedCycle`/`ReconcileOutcome` boundary if a poller trait is ever drawn** — but per 1.4's recommendation, no poller trait is proposed by this spike at all; the shared restart/status/sleep wrapper is small enough (~150 LOC) that duplicating it is cheaper than the risk of coupling two different completeness models through one method signature.

**Sanity-check against a hypothetical GitHub Issues adapter** (paper only):

- `WritebackRegistry<F>` / `WriteFieldClass`: GitHub issue fields split naturally into scalar (`state`) and additive (`labels`, `assignees`) — this is a **better fit for ClickUp's Additive/Scalar model than for Linear's all-scalar one**, which is good evidence the generic should keep the distinction ClickUp introduced rather than the one Linear (the nominal "canon," per D3) happened to need less of. A trait/generic over 2 samples risks overfitting to whichever sample is louder; here the quieter sample (ClickUp) had the more general shape.
- `CredentialStore`: fits GitHub cleanly (PAT or OAuth installation token, same keyring/fallback shape). GitHub's need for an OAuth-style token is closer to Linear's `AuthMode` seam than to ClickUp's plain-token model — validates keeping `AuthMode` as a tracker-specific wrapper rather than baking it into the shared struct (a third data point would have forced the struct wider if `AuthMode` had been inside it).
- `ComposableItem`: fits well — issue body + comments + labels + assignees map onto `ComposedSection` variants with no strain.
- Poller/mirror (no trait proposed): GitHub's `since` param + ETag-based conditional requests is a **third, different completeness model** from both ClickUp's (space-scoped-complete) and Linear's (window+delta-reverify) — this is a third data point confirming D1/1.3's conclusion that this layer resists a shared contract at even 2 samples, let alone generalizing cleanly to 3.
- Closure 2-vs-3-halves: GitHub has no product concept resembling the confirmation-token gate at all (that is a Nergal-invented safety UX, not something GitHub's API shapes) — meaning this leak is not "a third tracker will clarify it," it is intrinsic to how much local product behavior (unbind, mark-worked) varies per integration, and would recur with a GitHub adapter regardless of what trait is drawn.

### 1.4 Go/no-go recommendation

**GO-BUT-NARROWER.** Extract only what 1.1-1.3 showed to be mechanically shared with zero or one clean parameter: the `WritebackRegistry<F>` generic (writeback.rs), the `CredentialStore` struct (auth.rs core, excluding `AuthMode`/multi-workspace), the closed-out marker trio (mirror.rs, as 3 small SQL-templated free functions or a tiny 3-method trait — either is fine, it's ~60 LOC either way), and the `ComposableItem`/compose-budget framework (integration.rs). **Do not** build the full `IssueTrackerAdapter` trait spanning poller+mirror+closure+auth that D1 originally proposed — the paper-migration in 1.3 found three leaks inside exactly those layers (closure 2-vs-3-halves, echo-check ordering, completeness-model divergence) that would force the trait's method boundaries to either leak per-tracker overrides everywhere or freeze in-flight bug fixes.

**How much duplication is really shared**: ~2,600-2,900 LOC of ~15,759 (~17%), concentrated in 4 modules. This is a real number, not a rounding error, but it is meaningfully less than D1's framing implied ("extract where duplication is mechanical: poller, writeback, mirror, closure, auth" — five layers; the spike found clean mechanical duplication in essentially 2 of those 5, `writeback` and `auth`, plus one layer D1 didn't name, `integration`).

**How many leaks force tracker-specific surface**: 3 significant ones (closure shape, echo ordering, completeness model), all inside the layers this recommendation excludes from extraction. Zero leaks were found in the 4 layers recommended for extraction — `writeback.rs`'s registry, `auth.rs`'s credential core, `mirror.rs`'s closed-out trio, and `integration.rs`'s compose framework all paper-migrated cleanly with no tracker-specific override needed beyond the parameters already designed in.

**Does the trait pay for itself at 2 trackers or only 3+?** For the full 5-layer trait D1 proposed: not at 2, and the GitHub Issues paper sanity-check in 1.3 suggests not cleanly at 3 either (GitHub's completeness model is a third, different shape — this problem doesn't converge with more samples, because pagination/completeness semantics are inherently tied to each API's specific guarantees). For the narrower 4-module extraction recommended here: it pays for itself **now, at 2**, because the payoff is bug-fix propagation on genuinely mechanical code (a `WritebackRegistry` fix lands once for both trackers), not "cheaper to onboard tracker 3" — a future GitHub adapter would still need to write its own poller/mirror/closure/client/model from scratch under this recommendation; only the registry/credential-store/closed-out/compose slice would be reused.

**Cost estimate for the recommended (narrower) scope**: ~600-700 LOC of new shared code (new module, suggest `src-tauri/src/tracker_shared/`) replacing ~1,300-1,500 LOC of near-duplicate code across 8 existing files (`clickup/auth.rs`, `linear/auth.rs`, `clickup/writeback.rs`, `linear/writeback.rs`, `clickup/mirror.rs`, `linear/mirror.rs`, `clickup/integration.rs`, `linear/integration.rs`) — a net reduction of roughly 600-800 LOC. Risk: **low-to-medium**. None of the four extracted pieces touches network-protocol logic or the completeness/tombstoning state machines (the riskiest code in either tree); both sides already have solid, nearly-parallel test coverage for these exact paths (compare the registry/closure test modules in `clickup/writeback.rs` and `linear/writeback.rs` — they assert the same properties today, and would consolidate rather than duplicate under the generic).

**Migration order if GO**: (1) `WritebackRegistry<F>` first — self-contained, no DB/network dependency, best existing test coverage, and directly forecloses future echo-ordering-class drift once both trackers share one implementation. (2) closed-out marker trio second — trivial, 3 functions, lowest risk. (3) `CredentialStore` third — slightly more sensitive (keyring/fallback-file security-reviewed code), do it with care once the pattern from (1)-(2) is proven. (4) `ComposableItem` compose framework last — lowest urgency, mostly cosmetic duplication today. Direction of migration: build the shared module against **Linear's shape first** (matches D3 — Linear is the semantic canon where the two disagree, and its `WriteField`/registry code has no known open bugs), then migrate ClickUp onto it, rather than the proposal's original ClickUp-first assumption — ClickUp is the side with the pending independent fix (`clickup-writeback-echo-ordering`), so migrating it onto a shared registry is safer done *after* that fix lands and its behavior is settled, not concurrently with it.
