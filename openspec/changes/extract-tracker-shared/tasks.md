## 1. Shared module scaffold

- [x] 1.1 Create `src-tauri/src/tracker_shared/mod.rs` + wire `mod tracker_shared;` into `lib.rs` (one-line, minimal diff). Empty submodule stubs added as each slice lands.

## 2. Slice 1 — WritebackRegistry (Linear first, then ClickUp)

- [x] 2.1 `tracker_shared/writeback_registry.rs`: `WritebackRegistry<F>` + `WriteFieldClass` trait (`field_class() -> FieldClass{Scalar,Additive}`) + `WriteEntry` + `EchoCheckResult` (4 variants incl `AdditiveDivergence`) + `check_echo` returning NEUTRAL `(id, field, your, remote)` data (NOT a shared `WriteConflict` — that stays per-tracker for its serde wire key) + TTL as a `Duration` construction param (no `use super::poller`). Built against Linear's shape. Consolidate the two registry test modules (assert what both asserted).
- [x] 2.2 Migrate `linear/writeback.rs` onto the shared type (Linear impls `WriteFieldClass` all-Scalar); delete the now-dead Linear copy; keep `tracked_issue_ids` reachable. Gates green.
- [x] 2.3 Migrate `clickup/writeback.rs` onto the shared type (ClickUp impls `WriteFieldClass` with its real Scalar/Additive split); delete the ClickUp copy. Gates green.

## 3. Slice 2 — closed-out marker trio

- [ ] 3.1 Grep callers of `read_closed_out` (both trackers) — confirm none depend on ClickUp's insertion order before adding a deterministic `ORDER BY`.
- [ ] 3.2 `tracker_shared/closed_out.rs`: `mark`/`read`/`unmark` free fns parametrized by table + id-column; add deterministic `ORDER BY closed_at` to both sides (consumer verified order-insensitive — behavior-preserving). Consolidate the trio's tests.
- [ ] 3.3 Rewire `clickup/mirror.rs` + `linear/mirror.rs` closed-out call sites to the shared fns; delete the duplicated bodies. Gates green.

## 4. Slice 3 — CredentialStore (security-reviewed)

- [ ] 4.1 `tracker_shared/credential_store.rs`: `CredentialStore` generic over service/account/filename — MOVE the `Debug` redaction, atomic temp+rename, and `#[cfg(unix)] opts.mode(0o600)` lines VERBATIM (a move, not a rewrite). Shared fallback field `secret` with `#[serde(alias = "token", alias = "key")]` so both trackers' existing on-disk TOML files still deserialize. Built against Linear's parametrized `store_to`/`load_from`/`remove_from` shape.
- [ ] 4.1b Test: deserialize a legacy ClickUp `token = "…"` file AND a legacy Linear `key = "…"` file through the shared `read_fallback_file` — both yield the secret (no forced re-auth on upgrade).
- [ ] 4.2 Migrate `linear/auth.rs` onto `CredentialStore`, keeping `AuthMode`/`authorization_header_value` + `validate_org_id`/multi-workspace namespacing as tracker-specific wrappers AROUND it (not inside). Gates green.
- [ ] 4.3 Migrate `clickup/auth.rs` onto `CredentialStore` (flat single-account wrapper). Delete both dead copies. Gates green.
- [ ] 4.3b Keyring/file constant pass-through (iprev round 2 MINOR — silent credential loss on the keyring path): `CredentialStore` is constructed with `service = "nergal"`, `account` ∈ {`clickup-token`, `linear-token`}, `filename` ∈ {`clickup.toml`, `linear.toml`}, passed through BYTE-IDENTICAL (no "tidying" the account name mid-move). The Linear per-org format `"linear-token::{org_id}"` stays produced by the wrapper (outside `CredentialStore`, per D3). Add a test asserting the constructed service/account strings equal the pre-refactor literals for both trackers.
- [ ] 4.4 Security review the CredentialStore diff line-by-line: 0600 perms, atomic rename, redaction, no secret in any error string, keyring account/service constants unchanged, `#[cfg(unix)]`/`#[cfg(not(unix))]` gating survives the file move — all preserved.

## 5. Slice 4 — ComposableItem / compose framework — DESCOPED (iprev round 1)

Cut from this change to a follow-up: `fit_to_budget` is control-flow-divergent (ClickUp's checklist-collapse stage has no Linear counterpart), so a shared `ComposableItem` trait would be leaky — the anti-pattern the spike rejected. `integration.rs` is NOT touched here. Grounded facts retained in `implementation.md` for the follow-up change.

## 6. Verification

- [ ] 6.1 Final gates: `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check` (real exit via `rtk proxy`) + `npx tsc --noEmit`.
- [ ] 6.2 `git diff --stat` confirms the net LOC reduction; `git grep` confirms no remaining duplicate copy of an extracted body; no DB migration added.
- [ ] 6.3 Manual (user walk): a ClickUp + Linear writeback/echo path, a keyring store/load, a closed-out round-trip, a compose-context delivery — no behavior change.
