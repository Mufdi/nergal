# backend-module-structure Specification

## Purpose
TBD - created by archiving change split-god-modules. Update Purpose after archive.
## Requirements
### Requirement: Commands are domain-partitioned with an unchanged surface

Tauri command handlers SHALL live in domain files under `src-tauri/src/commands/`, re-exported from `commands/mod.rs` such that every `commands::<name>` path in `lib.rs`'s `generate_handler!` and every command's externally visible name and signature are unchanged.

#### Scenario: frontend contract is untouched

- **WHEN** the split lands
- **THEN** every `invoke("<command>")` call in `src/` resolves exactly as before, with no frontend change

#### Scenario: a domain is navigable

- **WHEN** a contributor looks for ship/PR logic
- **THEN** all such commands are in one domain file rather than interleaved in a 4k-line monolith

### Requirement: Database methods are domain-partitioned

`Database` methods SHALL be split across domain files under `src-tauri/src/db/` as multiple `impl Database` blocks, with `db/mod.rs` owning the struct, connection/open, and the migration runner. Method names and signatures SHALL be unchanged.

#### Scenario: behavior-preserving split

- **WHEN** the split lands
- **THEN** `cargo test` passes with no test modifications and `cargo check` shows no public API change

### Requirement: Split commits are pure moves

Each split commit SHALL contain only code moves (no logic edits), verifiable via `git diff --color-moved`, and the commit hashes SHALL be recorded in `.git-blame-ignore-revs`.

#### Scenario: blame survives the split

- **WHEN** a developer runs `git blame` (with the ignore-revs file configured) on a moved function
- **THEN** attribution points to the pre-split logic commits, not the move commit

