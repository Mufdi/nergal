# REVIEW — clickup-sync

## Pre-build: iterative-plan-review (Claude evaluator, adversarial, codebase-verified)
2 rounds → **APPROVED**. R1: 18 findings [4 critical, 4 high] → R2: APPROVED with 2 medium + 2 low residuals, all fixed post-approval.

Round-1 criticals (all resolved): (C1) first-poll notification storm with no baseline → silent-first-sync gate (`clickup_sync_state.baseline_done`) + coalescing; (C2) absent-task semantics undefined → authoritative complete-Space-fetch tombstoning + un-tombstone-on-reappear; (C3) poll-scope ambiguity → resolved to all-tasks-per-Space, assigned-to-me as a local filter; (C4) FK insertion-ordering panic → ordered upsert in one transaction + placeholder-list synthesis. Highs: torn reads → fetch-all-then-commit atomic; pagination → `last_page` not row count; subtask tree → sole source `parent_id` from flat `parent`; rate-limit math corrected. Mediums/lows: sanitized markdown rendering (WebKitGTK XSS), lazy/gated thumbnails (SSRF), atomic `0600` token file (TOCTOU), custom-field defs derived from payloads, multi-team picker, token-leak guard, notification coalescing.

Round-2 residuals fixed post-approval: (1) un-assignment is NOT a tombstone case (task stays present with updated assignees, hidden by the local filter) — corrected in design/tasks/impl; (2) status caching vs freshness tension dissolved — statuses ride inline on the List objects in the hierarchy fetch, so no separate per-List call and no caching needed (rate-limit budget corrected down); (3) show-closed toggle does an on-demand `include_closed=true` fetch; (4) placeholder-list exempt from hierarchy-absent tombstoning.

## Post-build reviewers

### Mid-build (post groups 1-3: auth + client + mirror) — 2026-06-09

#### Reviewer: security (token handling)
**PASS.** 0 critical / 0 high / 2 medium / 2 low / 1 info. Applied immediately:
- [Medium ×2] `Debug` derive on `StoredToken`/`FallbackFile` would print the raw token → replaced with manual redacted `Debug` impls (`auth.rs`).
- [Low] `clickup_task_custom_values.field_id` lacked FK → added `REFERENCES clickup_custom_field_defs(id) ON DELETE CASCADE` in `015_clickup_mirror.sql` (pre-ship, no installed binary carries it; tasks.md DDL synced).

Deferred as optional hardening (not applied — edge-case/defense-in-depth):
- [Low] `config_dir()` `/tmp` fallback dir created with umask mode (file itself is 0600) — `DirBuilder::mode(0o700)` if desired.
- [Info] temp-file name uses predictable pid (DoS-only; parent dir is user-owned); `tempfile::NamedTempFile` would harden.
- [Info] client URL building interpolates API-returned ids into paths (ids are numeric strings; theoretical) — `path_segments_mut()` if hardening later.

Verified clean: parameterized SQL throughout, `HeaderValue::set_sensitive(true)`, TOML-error redaction, atomic 0600 fallback write, fixtures fictional, commands never return the token.

#### Reviewer: deps (keyring)
**PASS.** `keyring 3.6.3` (latest v3), OSV.dev clean for keyring/secret-service/zbus. Lockfile +27 entries, nothing unmaintained/yanked. Finding applied: Cargo.toml rationale comment was false ("rides the zbus already in the tree" — keyring 3 actually pulls a duplicate zbus 4.x stack next to the tree's zbus 5) → comment corrected, dedupe path documented (keyring v4 + zbus-secret-service-keyring-store). keyring v4 migration noted as future work; v3 fine short-term.

Re-verified after fixes: cargo clippy clean, 350 tests pass, fmt clean.

### Final review (post groups 4-5) — 2026-06-09

The planned 4-parallel reviewer pass (spec + code-quality + security + frontend) was **skipped by user decision** at spawn time ("continúa"). Replaced with an orchestrator inline spot-check of the load-bearing points:

- **Sanitized rendering**: no `dangerouslySetInnerHTML` / `rehype-raw` anywhere in the new frontend; descriptions + comments go through `MarkdownView` (react-markdown + remarkGfm, no raw-HTML pass-through) — the same path Obsidian untrusted note bodies use.
- **Thumbnail gating**: `isImageAttachment()` keys on mimetype/extension, never on `thumbnail_url` presence; detail-open only, `loading="lazy"`. Hardened during spot-check: `<img src>` additionally requires an `https?://` scheme.
- **External URL opening**: found `openShell()` called with multi-writer untrusted URLs (`task.url`, `attachment.url`) while `shell:allow-open` is unscoped → added `openExternalUrl()` guard (http/https only) in `ClickUpTaskDetail.tsx`. PrViewer precedent opens trusted gh-CLI URLs, so the gap was specific to ClickUp content.
- **Conventions**: zero `unwrap()`/`expect()` outside `#[cfg(test)]` modules (awk-verified across all 6 clickup .rs files); zero TODO/FIXME.
- **Gates**: clippy `-D warnings` clean · 371 Rust tests pass · `cargo fmt --check` clean · `tsc --noEmit` clean. Scope gate: ~28 files touched vs estimate 22 (ratio 1.27 < 1.5, no creep escalation).

Pending: manual walk (tasks.md 6.3) — requires a real token + live workspace.
