# REVIEW — fix-stale-docs

## Orchestrator vet (XS docs; done directly) · 2026-07-04

**Verdict: PASS.** Documentation-only, verified against source.

- IPC paths cross-checked against `platform/mod.rs`: Linux `/run/user/<uid>/nergal/` +
  `~/.local/share/nergal/ipc/` fallback, macOS `temp_dir()/nergal/`, Windows named pipes
  `\\.\pipe\nergal-<SID>-<endpoint>`, endpoint names `hook.sock`/`plan-{pid}.fifo` — all
  match the implementation.
- `cargo install --path` reinstall instruction → the sanctioned `pnpm tauri build` + `dpkg
  -i` flow (docs/hooks.md + a stray in README.md).
- **Fabricated path caught**: CLAUDE.md's `/tmp/nergal-ask-*.fifo` doesn't exist (ask-user
  is a fire-and-forget socket message, no FIFO) — correctly DROPPED, not "corrected" to a
  fake path.
- README Linux build-prereqs block (apt list byte-matches the linux-deps composite action).
- Strays found beyond the 3 enumerated files: README.md (own cargo-install + /tmp path),
  docs/architecture.md (/tmp path) — all fixed (task 4.1's grep spans all of docs/ + the 2).

## Task 4.1 greps

- `grep /tmp/nergal docs/ README.md CLAUDE.md` → empty. ✓
- `grep cargo install --path` → only the "do NOT run this" warning clauses remain (same
  precedent as CLAUDE.md's existing Reinstall row). ✓

## Gates

- Docs-only; no build/test needed. Both verification greps clean.
