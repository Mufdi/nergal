# Design — linear-mirror

This change mirrors the archived `clickup-sync` foundation for Linear. Where the
shape is identical (keyring auth, atomic reconcile, silent baseline, tombstoning,
floating detail, focus-zone nav), this doc references the ClickUp precedent and
documents only the **Linear-specific divergences**. The divergences are large
enough (GraphQL vs REST, fixed schema vs custom fields, bounded vs full poll,
`RATELIMITED` vs `429`) that this is an adaptation, not a copy.

## Verified API facts (anchored 2026-06-16, linear.app/developers)

- Endpoint: `https://api.linear.app/graphql` (single URL, POST).
- Personal API key header: `Authorization: <key>` — **no `Bearer` prefix** (OAuth uses `Bearer <token>`).
- Viewer query: `query { viewer { id name email } }`.
- Rate limiting: leaky bucket. On exceed → **HTTP 400 with GraphQL error code `RATELIMITED`** (no `429`, no `Retry-After`). Quota headers: `X-RateLimit-Requests-Remaining/Reset`, `X-RateLimit-Complexity-Remaining/Reset` (Reset = UTC epoch ms). Limits: 5000 req/hr, 3M complexity/hr, 10k complexity max per single query.
- Pagination: Relay cursor connections — `pageInfo { hasNextPage endCursor }`, args `first` / `after`. Issues orderable by `updatedAt`.

## D1 — GraphQL client instead of a REST client

**Decision.** `src-tauri/src/linear/client.rs` is a typed GraphQL client over the shared `reqwest::Client`: one POST to the single endpoint, body `{ query, variables }`, response `{ data, errors }`. Queries are authored as `const &str` GraphQL documents (no codegen crate). Nested relations are fetched in one query (an issues page brings each issue's `state`, `assignee`, `labels`, `project`, `cycle`, `parent` inline), so the poller needs far fewer round-trips than ClickUp's REST fan-out.

**Alternatives considered.**
- *`graphql_client` / codegen crate.* Adds a build-time schema dependency and proc-macro churn for a handful of hand-written queries. Rejected — the query surface is small and stable; hand-authored documents + serde structs are lighter and match the ClickUp `model.rs` precedent.
- *REST-style endpoint wrappers.* Linear has no REST API. N/A.

**Consequences.** Error handling has two layers: HTTP status (transport) and the GraphQL `errors` array (application). `RATELIMITED` arrives as `errors[].extensions.code` on an HTTP **400**, so the client must inspect the body even on a 4xx rather than bailing on status alone (D6).

## D2 — Auth layer extensible for OAuth (personal key now)

**Decision.** Auth persistence reuses the ClickUp pattern verbatim (keyring `cluihud`/`linear-token`, atomic-0600 `linear.toml` fallback, disclosure flag, transient-vs-noentry distinction). The one forward-looking addition: an `AuthMode` enum consumed by a single `authorization_header()` builder.

```rust
enum AuthMode { Personal, OAuthBearer }  // OAuthBearer reserved, not wired this change
fn authorization_header(mode: AuthMode, secret: &str) -> (HeaderName, HeaderValue) {
    match mode {
        AuthMode::Personal     => ("Authorization", secret.to_string()),       // no Bearer
        AuthMode::OAuthBearer  => ("Authorization", format!("Bearer {secret}")),
    }
}
```

This change only constructs `AuthMode::Personal`. The branch exists so a later `linear-oauth` change adds the token-acquisition flow and flips the mode without touching the client, the mirror, or the persistence layer — the same staging promise we made for ClickUp (OAuth deferred, not precluded).

**Alternatives considered.**
- *Hardcode `Authorization: <key>` now, refactor later.* Rejected — leaves a header string baked across call sites; the enum is ~6 lines and removes the future refactor.
- *Implement OAuth now.* Out of scope per the staging decision; OAuth needs an app registration + a local callback listener, a change of its own.

**Security.** The key is read only into the header builder, never logged, never in an error string (the GraphQL `errors` surfacing in D1 must redact — errors carry server messages, not the key, but the rule is enforced at the boundary), never returned to the frontend. Validation returns the resolved `viewer`, not the key.

## D3 — Data model mapping (schema, migration 023)

Linear's model is fixed-schema (no ClickUp custom fields, no checklists). The mirror drops those two table families and adds first-class labels + native state types.

| Concept | ClickUp (015–020) | Linear (023) |
|---|---|---|
| Top container | `clickup_spaces` → folders → lists | `linear_teams` (issues hang off the team) |
| Status | `clickup_statuses` per-list, `status_type` added in 020 | `linear_workflow_states` per-team, native `type` column from day one |
| Labels/tags | `tags_json` denormalized on task | `linear_labels` def table + `linear_issue_labels` join (first-class, colored, group-by axis) |
| Custom fields | `clickup_custom_field_defs` + values | **none** — Linear has no custom fields |
| Checklists | `clickup_checklists` + items | **none** — Linear uses sub-issues |
| Sub-tree | `parent_id` self-FK | `parent_id` self-FK (identical) |
| Priority | ClickUp 1–4 (1=urgent) | Linear 0–4 (`0`=none,`1`=urgent,`2`=high,`3`=medium,`4`=low) — store int, map at render |
| Grouping extras | — | `linear_projects`, `linear_cycles` (minimal metadata, nullable issue refs) |

**Tables (023):**
- `linear_teams(id PK, name, key, synced_at)`
- `linear_workflow_states(id PK, team_id FK→teams ON DELETE CASCADE, name, type, color, position, stale, stale_since)`
- `linear_labels(id PK, team_id FK NULLABLE, name, color)` — workspace labels have null team_id; **upsert-only, never tombstoned by absence** (D8/iprev #19); GC'd only when no live issue references them.
- `linear_projects(id PK, name, state, stale, stale_since)`
- `linear_cycles(id PK, team_id FK→teams ON DELETE CASCADE, number, name, starts_at, ends_at, stale, stale_since)`
- `linear_users(id PK, name, display_name, email, avatar_url)`
- `linear_issues(id PK, identifier, team_id FK, title, description, state_id FK, priority INT, estimate REAL, assignee_id FK→users NULLABLE, project_id FK NULLABLE, cycle_id FK NULLABLE, parent_id TEXT NULLABLE, was_viewer_assigned INT, due_date, created_at, updated_at, completed_at, url, synthetic INT DEFAULT 0, stale, stale_since)`
- `linear_issue_labels(issue_id FK→issues ON DELETE CASCADE, label_id FK→labels, PRIMARY KEY(issue_id,label_id))`
- `linear_comments(id PK, issue_id FK→issues ON DELETE CASCADE, user_json, body, created_at)`
- `linear_sync_state(id PK CHECK(id=1) single-row, baseline_done, last_full_sync, viewer_id, selected_team_ids_json)`

Indices: `idx_linear_issues_team(team_id)`, `idx_linear_issues_parent(parent_id)`, `idx_linear_issues_state(state_id)`, `idx_linear_issues_stale(stale)`, `idx_linear_issues_updated(updated_at)`, `idx_linear_issue_labels_label(label_id)`. `PRAGMA foreign_keys=ON` is already set (db.rs).

**Schema decisions forced by the bounded scope (iprev round 1):**
- **`parent_id` is a plain `TEXT`, not a self-FK** (iprev #2, #5). A sub-issue's parent can be out of the polled window → a hard self-FK (`REFERENCES linear_issues(id)`) would throw an immediate FK violation at insert and abort the transaction, and page order (`updatedAt` desc) is not topological so a child can precede its parent within one batch. The tree is built in app from `parent_id`, tolerant of a dangling parent (render as a root). Dropping the self-FK also removes the cascade that contradicted the childless-GC invariant (#5) — sub-issue retention is enforced in app GC, and the DB can no longer cascade-wipe a live subtree.
- **No `archived` column** (iprev #13). Linear's `issues` connection excludes archived issues unless `includeArchived: true`; we don't pass it, so an archived issue simply vanishes from the fetch and is evicted by absence — a populated `archived=1` state would never be observable. Modeling it would imply a state that never populates.
- **`synthetic` flag** (iprev #17): placeholder state/team rows synthesized for an unknown FK are marked `synthetic=1` and **excluded from absence-tombstoning** (a placeholder is by definition never in Linear's fetch); they are re-resolved (synthetic→real) when the genuine row arrives in a later fetch.
- **`was_viewer_assigned`** (iprev #3): the mirror records whether it currently believes an issue is the viewer's, so the poll can re-verify exactly those issues by id each cycle (robust un-assignment detection independent of `updatedAt` semantics — see D4).

**Implementation refinements (shipped — the migration `023_linear_mirror.sql` is authoritative over the table sketch above):**
- **`synthetic` lives on the FK-target tables (`linear_teams`, `linear_workflow_states`), not on `linear_issues`.** Only issues are tombstoned/evicted; teams/states/labels/projects/cycles are upsert-only and GC'd when unreferenced. A placeholder team/state row (`synthetic=1`) therefore can never oscillate (its table isn't absence-tombstoned), strengthening iprev #17. Issues carry no `synthetic` (they are never synthesized). `linear_projects`/`linear_cycles` dropped their `stale` columns for the same reason (upsert-only).
- **`linear_issues.description`** is selected into the panel `IssueView` so the floating detail renders the body from the same atom (Linear bodies are modest; no separate detail fetch needed for the description — only comments are lazy).

**Alternatives considered.**
- *Denormalize labels as `labels_json` (ClickUp tags parity).* Rejected — labels are a group-by axis with stable ids+colors; a join table makes group-by/filter a query, not a JSON parse, and matches how `linear_workflow_states` is first-class. The extra join table is cheap.
- *Keep the `parent_id` self-FK with deferred constraints (`DEFERRABLE INITIALLY DEFERRED`).* Solves intra-batch ordering but still fails at COMMIT for a genuinely out-of-scope parent. A plain column with an app-built tolerant tree is simpler and strictly more robust.
- *Model projects/cycles fully.* Deferred — this change needs only enough metadata to label and group an issue (name, dates). A future projects-panel change can deepen them.

## D4 — Bounded poll scope (the one material runtime decision)

ClickUp's invariant was "poll **all** tasks per Space, filter locally" — it worked because the validated workspace was small. Linear workspaces routinely hold **tens of thousands** of issues; an unscoped poll every 45s would burn the 3M-complexity/hr quota and stall. So the Linear poll is **bounded**, and that bound is the one decision worth flagging for the runtime walk.

**Decision.** One timestamp `cycle_now` is captured at cycle start and drives **both** the server filter and the tombstone-candidate predicate (iprev #10 — no independent forward-advancing clocks). `window_start = cycle_now - linear_active_window_days`. Per selected team, the poll fetches the union of three sets, all **scoped to the selected teams**:
1. **Active window** — issues with `updatedAt > window_start`, ordered `updatedAt` desc, cursor-paginated.
2. **Viewer-assigned (server, selected teams)** — issues where `assignee == viewer` **within the selected teams** (query carries a `team_ids` filter), regardless of age (small set; your backlog never disappears). The union is **constrained to selected teams** (iprev #9) — a workspace-wide assigned query would drag in issues whose team + workflow-states were never fetched, forcing perpetual placeholder synthesis.
3. **Re-verify the delta-mine (local→server by id)** — NOT a second full pass over your backlog (iprev r2 #2). Set 3 re-fetches only `mirror_mine − ids(set 1) − ids(set 2)`: the issues the mirror currently flags `was_viewer_assigned=1` that **neither** the window **nor** the viewer-assigned query surfaced this cycle. Those are exactly the ones that may have been un-assigned, moved away, or deleted — re-fetched by id (`issues(filter:{ id:{ in:[…] } })`) to learn their fate. This makes un-assignment detectable **regardless of whether an assignee change bumps `updatedAt`** (iprev #3) without doubling the per-cycle complexity (a still-assigned issue is already in set 2, so it's excluded from set 3).

   **Chunked + paginated (iprev r3 #3):** the delta is usually tiny but **unbounded right after a bulk un-assignment** — the moment correctness matters. So the by-id query chunks the id list (≤ 25 ids/batch, matching the `first ≤ 25` page cap) and cursor-paginates each batch (`hasNextPage`/`after`). Set 3 is one of the branches the completeness gate watches: its `complete` contribution is "every chunk of every batch reached `hasNextPage == false`". An un-paginated/truncated set-3 makes the cycle **incomplete**.

   The set-3 by-id query fetches the **full nested issue shape** (same fragment as `issues_page`: state/assignee/labels/project/cycle/parent), so its upsert is a complete reconcile — **not** a lean assignee-only update that would let §3.2's per-issue label reconcile strip the labels off the user's own backlog issue (iprev r4 #3).

   **Set-3 outcome handling — keyed on what a COMPLETE set-3 shows (iprev r2 #1/#7, r3 #2, r4 #3):** `issues(filter:{id:{in:[…]}})` does **not** 404 a missing id — a deleted, archived, or access-revoked issue simply does **not appear** in the returned `nodes`. So outcomes split by presence, and *absence is only actionable on a complete set-3*:
   - **Returned, `assignee == viewer`, team still selected** (e.g. set 2 was incomplete and missed it) → **retain flag, normal full upsert, no evict** (iprev r4 #3 — the fourth case). Still yours; nothing changed.
   - **Returned, `assignee != viewer`** → clear `was_viewer_assigned`; normal scope rules then apply (evict if aged-out).
   - **Returned, `team_id` not in selected teams** (moved to an unsynced team) → its workflow-states were never fetched, so it cannot render: **clear flag + evict** rather than synthesize an un-resolvable placeholder (the churn #9 warns of). By design: an issue you own that moves to a team you don't sync disappears until you add that team.
   - **Absent from a COMPLETE set-3** (the only way "deleted-while-mine" surfaces) → **clear flag + evict**. This is the sole path that cleans a deleted issue the mirror still thinks is yours.
   - **Absent from an INCOMPLETE set-3** (rate-limit / network give-up mid-batch) → **retain unchanged** (the absence is "we didn't get to it", not "it's gone"). Acting on absence here would hard-delete the user's real issues — the data-loss hazard r3 #2 calls out.

Assigned-to-me stays a **local** filter over the mirror (so the panel can also show team-wide active work).

**Completeness-gated tombstoning (iprev #1 — the data-destroying fix).** A fetch tombstones absent issues **only when every paginated branch for every selected team reached `pageInfo.hasNextPage == false`** — i.e. the fetch is provably complete. If pagination was interrupted (rate-limit give-up, network error, retry cap), the cycle commits **upserts only, zero tombstones**, and logs `incomplete fetch — tombstoning skipped`. Without this gate, a rate-limit on page 3 of 8 would see pages 4–8's live issues as "absent" and tombstone every active issue on them.

**Tombstone-candidate predicate (complete fetches only).** An issue is tombstoned (`stale=1`) iff: it belongs to a selected team **AND** `updated_at > window_start` (it was *in scope*, so the complete fetch was authoritative over it) **AND** it is absent from the cycle's global fetched-id set (across all teams + all three branches — so an issue **moved A→B** between two selected teams survives, iprev #15) **AND** it is not `synthetic` (iprev #17) **AND** not `was_viewer_assigned` (set 3 is authoritative for those). Reappearance resets `stale=0`.

**Age-out eviction (iprev #4 — distinct from tombstoning).** An issue with `updated_at <= window_start` that is **not** `was_viewer_assigned` is out of scope: the fetch is *not* authoritative over it (its absence means "aged out", not "deleted"). Leaving it forever would grow the mirror unbounded and show a 30-day-stale snapshot (and a possibly-reopened issue under show-completed). So a dedicated **eviction pass** hard-deletes such rows once they age past `window_start` (childless first — a parent with live in-scope sub-issues is retained until its subtree also ages out). This aged-out-non-mine eviction is **safe regardless of fetch completeness** (the row is out of scope by definition). But eviction reads the `was_viewer_assigned` flag, whose accuracy depends on set 3 — so the flag-clearing that *makes* an issue eligible for eviction is applied **only from a complete set-3** (iprev r3 #4): on an incomplete cycle the mine-flag is retained and the issue is not evicted. Eviction runs **inside the same reconcile transaction** (iprev r2 #6), ordered after upserts and tombstones, so a concurrent panel read never observes a half-evicted subtree. Eviction is independent of the tombstone-GC retention window.

**Alternatives considered.**
- *Assignee-only poll.* Loses team-wide visibility. Rejected.
- *Unscoped (ClickUp parity).* Infeasible at Linear scale. Rejected.
- *Rely on `updatedAt`-bumps-on-reassign alone* (drop set 3). Rejected — it's an unverified API assumption (iprev #3); set 3 makes correctness independent of it. The `updatedAt` window is still the primary scope; set 3 is the safety net for the viewer's own issues.

**Configurability.** `linear_active_window_days` (default 30) and `linear_poll_interval_secs` (default 45, floor 10) are config fields, both **backend-owned** (added to `BACKEND_OWNED_CONFIG_KEYS` — without this they hit the stale-config clobber the summarizer work fixed).

**Walk note.** This scope is the item to validate live: confirm a real workspace's issue volume is covered by window + assigned, and that the complexity headroom is comfortable at the default interval. Also verify empirically whether an assignee mutation bumps `updatedAt` (informational — set 3 makes us correct either way).

## D5 — Reconcile, baseline, lazy comments

Atomic single-transaction reconcile, FK-safe upsert order, placeholder synthesis on unknown FK (marked `synthetic`, D3), silent first-sync baseline, coalesced assignment notifications, lazy comment fetch — all as in `clickup-sync`. Linear-specific divergences:

- **Completeness-gated tombstoning + age-out eviction** live in D4, not here.
- **Account-swap safety via a key-generation epoch (iprev #8, #14, r2 #3, r3 #1/#6, r4 #1/#2/#4).** `linear_sync_state` is a single row storing `viewer_id` and a monotonic **`key_generation`** counter. The authoritative defense against an account swap is the epoch, not a viewer-id comparison (which r4 #2 showed becomes dead code once `set_key` is the sole setter):
  - **`linear_set_key`** is the only key-setter. It atomically: bumps `key_generation`, `wipe_mirror()`, clears `viewer_id`, clears `selected_team_ids_json` (r4 #4 — old-org team ids match nothing in the new org → otherwise a permanently-empty panel with no signal), and resets `baseline_done=0`. The wipe is immediate (r3 #6 — no prior-account rows linger).
  - **Each poll cycle captures `key_generation` at the start** and, **inside the reconcile transaction**, re-reads it and **aborts the commit if it changed** (r4 #1). This closes the swap race: a slow cycle that fetched old-account data while the user swapped keys cannot re-populate the just-wiped mirror — its commit is discarded.
  - The cycle resolves `viewer` to populate `viewer_id` (needed for the assigned-to-me filter) and to set `baseline_done`. **The wipe fires only inside `set_key`, never from the cycle's viewer resolve** — so a failed/empty viewer resolve (rate-limit, network, transient keyring) simply **skips the cycle and never wipes** (iprev r3 #1). A successful resolve seeds `viewer_id` into the null slot left by `set_key`; there is no "non-null differs → wipe" branch (the epoch already handled the swap), removing the dead/unreachable backstop r4 #2 flagged.
  - `baseline_done` is set to `1` **only after a complete sync over a non-empty team selection** (iprev #14).
- **Workspace-label reconcile (iprev #19).** Labels are **upsert-only, never tombstoned by absence** (a per-team fetch is not authoritative over workspace-scoped labels). A label definition is GC'd only when no live issue references it via `linear_issue_labels`; the join row is reconciled per issue (delete-absent + insert-present within the issue's own upsert), so a wrongly-dropped label can never silently strip a live association.
- **Known limitation — lazy comments (iprev #20, r2 #9).** Comment refresh keys on `updatedAt` advance + detail-open. A comment-only change that does not bump the issue's `updatedAt` is reflected only when the user opens the issue's detail (which always refetches). Acceptable for a read-only panel; recorded as a known limitation, not relied on as an invariant.

## D6 — Rate-limit handling diverges from ClickUp

ClickUp: `429` + `Retry-After` seconds. Linear: **HTTP 400 + GraphQL `errors[].extensions.code == "RATELIMITED"`**, no `Retry-After`. The client therefore:
1. On any response, parse the body before deciding success — a 400 may be a rate-limit, not a hard error (iprev: a naive `status.is_success()` gate would kill the poll on every rate-limit).
2. If `RATELIMITED`, back off until the reset of **the bucket that actually returned exhausted** (iprev #6) — the bucket whose `*-Remaining` header is `0` (or, if the error names a bucket, that one), **not** the nearer of the two. Waiting on the requests-reset while the *complexity* bucket is the exhausted one (likely, given the nested query, #11) would retry too early and burn the bounded retries. **Fallback when neither `*-Remaining` reads `0` and the error names no bucket** (iprev r2 #5 — possible under the leaky-bucket race): wait until `max(Requests-Reset, Complexity-Reset)` (the *later* reset, never the nearer), clamped as below.
3. Clamp the wait to `[1s, 60s]` (iprev #12): a forward-cap avoids an unbounded wait on a forward-skewed clock; a `1s` floor avoids `reset - now <= 0` (backward skew) collapsing to a busy-retry that burns the retry budget. After the bounded retries the cycle is treated as **incomplete** (D4 completeness gate → no tombstones).
4. **Hard complexity rejection is distinct from `RATELIMITED`** (iprev #11). A single query exceeding the 10k-complexity cap fails outright (not a rate-limit) — surfaced as a normal GraphQL error, logged, not retried as a rate-limit. To stay under the cap, `issues` pages use `first ≤ 25` and the inner `labels` connection `first ≤ 10`. Budget arithmetic (Linear bills ~1 point/node + connection multipliers): 25 issues × (~6 scalar+relation nodes + a 10-node labels sub-connection) ≈ 25 × ~16 ≈ **~400 points/page**, comfortably under the 10k per-query cap with >20× headroom. The cursor loop handles volume; this is the one-line computation, not just a deferred comment.
5. Proactively, if `X-RateLimit-Complexity-Remaining` is low, the poller MAY slow its cadence — optional, not required for done.

## D7 — Frontend reuse

`StatusIcon`/`PriorityIcon` are already "Linear-style" glyphs (they were built for ClickUp by analogy to Linear). They take a status-type/fraction and a priority level — both of which Linear provides natively, so they are reused as-is (the StatusIcon's open/in-progress/done class maps directly from the workflow-state `type`; the PriorityIcon takes Linear's 0–4 with the same urgent/high/normal/low buckets). The panel mirrors `ClickUpPanel` (chip-strip group-by, focus-zone, header-action nav) and the floating detail reuses the dual-shell controller pattern (`docs/patterns.md` §13). New components live under `src/components/linear/`; new atoms under `src/stores/linear.ts`; the `linear` + `linear-task` `TabType`s register in `src/stores/rightPanel.ts` exactly as `clickup`/`clickup-task` do.

## D8 — Untrusted-content rendering (iprev #7, #16, anchored)

Issue descriptions and comments are multi-writer untrusted input rendered in a WebKitGTK webview via `MarkdownView` (`react-markdown` + `remark-gfm`, **no `rehype-raw`** → raw HTML is already escaped, anchored at `MarkdownView.tsx`). Two gaps the attachment-only sanitization missed:

- **Inline markdown images auto-load (iprev #7).** `MarkdownView` does not override the `img` component, so `![](http://attacker/pixel.png)` in a description renders `<img src=…>` and auto-fetches on detail open — a tracking pixel / IP + presence leak / WebKitGTK SSRF surface. The Linear detail SHALL render markdown through a variant that **gates remote images**: either suppress auto-load (render a click-to-load placeholder) or allow-list image hosts. This is a `MarkdownView` `img`-component override scoped to the Linear surface (does not change ClickUp's existing usage).
- **Link + attachment URL schemes (iprev #16).** `react-markdown`'s `urlTransform` already strips dangerous link schemes (`javascript:`, `data:`) down to safe protocols; the Linear surface keeps that (verify the custom `obsidianUrlTransform` preserves the safe-protocol filter). For **attachment chips** and any "open in browser" action, the URL is untrusted issue data → it SHALL pass through the existing `validate_url` (`src-tauri/src/browser.rs`, allow-lists `http`/`https`/`about:blank`) **before** being opened, reusing the backend guard rather than trusting the link.

## Events

`linear:sync-status`, `linear:changed`, `linear:assigned` — the read-surface subset of the ClickUp event set (`linear:write-conflict` is reserved for `linear-writeback`).

## Out of scope (later changes)

- Writes (status move, comments, assignee, description) → `linear-writeback`.
- Agent integration (issue→prompt, bind/pin, spawn-worktree-with-issue) → `linear-agent-integration`.
- OAuth token acquisition → future `linear-oauth` (this change leaves the `AuthMode` seam).
