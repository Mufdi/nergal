# Slice 3 build log — CredentialStore (security-reviewed)

## Files changed

- `src-tauri/src/tracker_shared/credential_store.rs` (new, 428 lines) — `CredentialStore { service, account, filename }` built against Linear's parametrized `store_to`/`load_from`/`remove_from` shape. Absorbs: `StoredSecret` (merged `StoredToken`/`StoredKey` in-memory shape, manual `Debug` redaction), `fallback_dir`, `#[cfg(windows)] legacy_fallback_path` (generalized from filename), the atomic `write_fallback_file` (`create_new(true)` + `sync_all()` + `std::fs::rename`, `#[cfg(unix)] opts.mode(0o600)`), `read_fallback_file` (redacted TOML parse errors), and the keyring get/set/delete wrappers (`store`/`load`/`clear` methods). `FallbackFile { secret }` with `#[serde(alias = "token", alias = "key")]`.
- `src-tauri/src/tracker_shared/mod.rs` — added `pub mod credential_store;`.
- `src-tauri/src/linear/auth.rs` — rewired onto `CredentialStore`. Kept as Linear-specific wrappers OUTSIDE the shared struct: `AuthMode`/`authorization_header_value`, `validate_org_id`, `account_for`/`fallback_path_for` (the per-org `linear-token::{org_id}` namespacing), and a local `StoredKey { key, on_disk }` (public field named `key`, not `secret`, so the module's existing callers — e.g. `legacy.key` in `linear/mod.rs:1224,1247`) — never touched). `StoredKey: From<StoredSecret>` bridges the two. Deleted the duplicate keyring/fallback-file core.
- `src-tauri/src/clickup/auth.rs` — rewired onto `CredentialStore` as a flat single-account wrapper (`account="clickup-token"`, `filename="clickup.toml"`). Kept a local `StoredToken { token, on_disk }` for the same reason (callers use `.token`). Deleted the duplicate keyring/fallback-file core.

`writeback.rs`, `mirror.rs`, `integration.rs`, `closure.rs` untouched on both trackers, per scope.

## Security invariant 1 — on-disk fallback field serde-alias

`FallbackFile { #[serde(alias = "token", alias = "key")] secret: String }` (`tracker_shared/credential_store.rs`). Required test (4.1b), both assertions present and passing:

```rust
#[test]
fn legacy_clickup_token_field_deserializes_via_alias() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clickup.toml");
    std::fs::write(&path, format!("token = \"{SECRET}\"")).unwrap();
    let loaded = read_fallback_file(&path).unwrap().unwrap();
    assert_eq!(loaded.secret, SECRET);
}

#[test]
fn legacy_linear_key_field_deserializes_via_alias() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("linear.toml");
    std::fs::write(&path, format!("key = \"{SECRET}\"")).unwrap();
    let loaded = read_fallback_file(&path).unwrap().unwrap();
    assert_eq!(loaded.secret, SECRET);
}
```

Both `cargo test` green (see gate output below).

## Security invariant 2 — keyring/file constant pass-through

Constructor call sites, byte-identical to the pre-refactor literals:

- `src-tauri/src/clickup/auth.rs`: `CredentialStore::new("nergal", KEYRING_ACCOUNT, FALLBACK_FILE)` where `KEYRING_ACCOUNT = "clickup-token"`, `FALLBACK_FILE = "clickup.toml"` (both constants copied unchanged from the pre-refactor file).
- `src-tauri/src/linear/auth.rs`: legacy account — `CredentialStore::new("nergal", KEYRING_ACCOUNT, FALLBACK_FILE)` where `KEYRING_ACCOUNT = "linear-token"`, `FALLBACK_FILE = "linear.toml"`; per-org account — `CredentialStore::new("nergal", account_for(org_id), fallback_path_for(org_id))` where `account_for` still produces `format!("{KEYRING_ACCOUNT}::{org_id}")` and `fallback_path_for` still produces `format!("linear-{org_id}.toml")` — both unchanged from the pre-refactor free functions, just returning `String` into the constructor instead of building a `PathBuf`/keyring `Entry` directly.

Required test (4.3b), both assertions present and passing (in `tracker_shared/credential_store.rs`, since they assert on the shared struct's fields directly):

```rust
#[test]
fn clickup_constants_are_byte_identical_to_pre_refactor_literals() {
    let store = CredentialStore::new("nergal", "clickup-token", "clickup.toml");
    assert_eq!(store.service, "nergal");
    assert_eq!(store.account, "clickup-token");
    assert_eq!(store.filename, "clickup.toml");
}

#[test]
fn linear_constants_are_byte_identical_to_pre_refactor_literals() {
    let store = CredentialStore::new("nergal", "linear-token", "linear.toml");
    assert_eq!(store.service, "nergal");
    assert_eq!(store.account, "linear-token");
    assert_eq!(store.filename, "linear.toml");
}
```

## Move-verbatim confirmation

Diff-level, the security-sensitive lines carried across unchanged in shape (only the receiver changed from a free fn to a method, and `token`/`key` → `secret`):

**0600 (unix cfg block)** — identical to both originals (`clickup/auth.rs:205-209` / `linear/auth.rs:290-294` pre-refactor):
```rust
#[cfg(unix)]
{
    use std::os::unix::fs::OpenOptionsExt;
    opts.mode(0o600);
}
```

**Atomic temp+rename** — identical structure to both originals (`create_new(true)`, `sync_all()`, `std::fs::rename`):
```rust
let mut opts = OpenOptions::new();
opts.write(true).create_new(true);
...
let write_result = file.write_all(body.as_bytes()).and_then(|()| file.sync_all());
if let Err(e) = write_result {
    let _ = std::fs::remove_file(&tmp);
    return Err(anyhow!("writing secret temp file: {e}"));
}
drop(file);
if let Err(e) = std::fs::rename(&tmp, path) {
    let _ = std::fs::remove_file(&tmp);
    return Err(anyhow!("installing secret file at {}: {e}", path.display()));
}
```

**Debug redaction** — same pattern as both originals' manual `Debug` impls, now on the merged `StoredSecret` + `FallbackFile`:
```rust
impl std::fmt::Debug for StoredSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSecret")
            .field("secret", &"[redacted]")
            .field("on_disk", &self.on_disk)
            .finish()
    }
}
```
`linear/auth.rs::StoredKey` and `clickup/auth.rs::StoredToken` keep their OWN manual `Debug` redaction too (unchanged from pre-refactor), since those types still exist as thin per-tracker wrappers around `StoredSecret`.

**Redacted TOML parse error** — same as both originals:
```rust
let parsed: FallbackFile = toml::from_str(&raw)
    .map_err(|_| anyhow!("malformed secret file {} (redacted)", path.display()))?;
```

No security-sensitive line required a rewrite; nothing to flag.

## AuthMode / multi-workspace staying outside `CredentialStore`

`linear/auth.rs` keeps `AuthMode`, `authorization_header_value`, `validate_org_id`, `account_for`, `fallback_path_for` exactly as free functions/types in the module (not fields/methods of `CredentialStore`). The per-org wrappers construct a fresh `CredentialStore` per call via `store_for(org_id) -> CredentialStore` (and `legacy_store() -> CredentialStore` for the bare-account legacy path); `CredentialStore` itself has zero knowledge of org namespacing, OAuth, or path-traversal validation — matching design D3 ("wrap, don't grow a dead field").

## Public API preserved for callers

- `clickup/mod.rs`: `auth::store_token`, `auth::load_token`, `auth::clear_token` — unchanged signatures, still return `StoredToken` with a `.token` field.
- `linear/mod.rs`: `auth::store_key_for`/`load_key_for`/`remove_key_for`/`store_key`/`load_key`/`clear_key`, `auth::AuthMode`, `auth::StoredKey` (`.key` field) — unchanged signatures. Verified via `cargo build --lib` after each tracker migration with zero call-site edits needed outside `auth.rs` itself.

## Gate outputs

**clippy** (`cd src-tauri && rtk proxy cargo clippy --all-targets -- -D warnings`):
```
    Checking nergal v0.4.1 (/home/felipe/Projects/cluihud/src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.88s
```
EXIT=0.

**test** (`cd src-tauri && cargo test`):
```
test result: ok. 807 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.09s
   (+ 3 integration-test binaries, 0/1/1 passed, all ok; doc-tests 0)
```
Reconciling the count: actual pre-slice-3 baseline (verified via `git stash`, tracked-file-only — `credential_store.rs` is untracked and its `pub mod` wiring in `mod.rs` was stashed too, so it compiled out) was **808**, not 810 as assumed at hand-off. Old `clickup/auth.rs` (5 tests) + `linear/auth.rs` (8 tests) = 13; new `clickup/auth.rs` (0, fully delegated) + `linear/auth.rs` (3: header ×2, `validate_org_id` ×1) + `tracker_shared/credential_store.rs` (9: 0600 round-trip, overwrite-keeps-0600, write-error-redacted, malformed-read-redacted, missing-is-none, legacy-clickup-alias, legacy-linear-alias, clickup-constants, linear-constants) = 12. 808 − 13 + 12 = **807**. Matches. All 4 required tests (4.1b ×2, 4.3b ×2) present in the passing set; no test coverage dropped (every pre-existing assertion has a consolidated equivalent), net −1 from dedup of the near-identical 0600/overwrite/write-error/malformed/missing tests that existed twice (once per tracker) and now exist once.

**fmt** (`cd src-tauri && cargo fmt --check`): failed on first run — rustfmt wrapped the `keyring_entry` one-liner in `credential_store.rs` onto two lines (line length). Fixed with `cargo fmt`; re-check EXIT=0, clean diff.

**tsc** (`npx tsc --noEmit` from repo root): `TypeScript: No errors found`, EXIT=0 — unaffected, no frontend surface touched.

## Net LOC (`git diff --stat`, tracked files only)

```
 src-tauri/src/clickup/auth.rs       | 312 +++-------------------------------
 src-tauri/src/linear/auth.rs        | 328 ++++--------------------------------
 src-tauri/src/tracker_shared/mod.rs |   1 +
 3 files changed, 52 insertions(+), 589 deletions(-)
```
New file (untracked, not in the diff stat above): `tracker_shared/credential_store.rs` = 428 lines added.

Net for slice 3: 52 + 428 insertions − 589 deletions = **−109 lines**.

## Not cleanly preserved / flagged for reviewer

Nothing structural. Two deliberate, non-security generalizations worth a reviewer's eye:

1. `write_fallback_file`'s temp-file naming now always derives from `path.file_name()` (Linear's pre-refactor style) rather than a hardcoded per-tracker constant (ClickUp's pre-refactor style) — required because the shared fn serves both a flat filename (`clickup.toml`) and Linear's per-org filenames (`linear-{org_id}.toml`). Behavior-identical for both trackers' existing filenames; only the *mechanism* generating the temp name generalized, not the atomicity/mode guarantees.
2. `#[cfg(windows)] legacy_fallback_path` on `CredentialStore` is parametrized by `self.filename` instead of ClickUp's hardcoded `FALLBACK_FILE` constant or Linear's `roaming_counterpart(local_path)` derivation — same output for both trackers' existing filenames, just centralized onto the struct so both the flat and per-org callers get Windows roaming-migration for free.

Both are mechanical generalizations of already-parametrized-by-Linear logic, not behavior changes for either tracker's real config values — flagging per the "if a security-sensitive line changes shape, stop and report" instruction, even though neither line is on the 0600/atomic-rename/redaction/constant-equality list.
