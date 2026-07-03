## Why

Three contributor-facing docs are stale in ways with concrete cost:

- **(a) `docs/hooks.md:53`** tells contributors to reinstall via `cargo install --path src-tauri --force` — the exact command CLAUDE.md's "Binary install workflow" forbids (it shadows `/usr/bin/nergal` for the GNOME launcher and skips frontend bundling → the ghost-window bug class).
- **(b) IPC paths are documented as `/tmp/nergal*`** (`docs/hooks.md:9,11` — `/tmp/nergal*.sock`, `/tmp/nergal-plan-{pid}.fifo` — and CLAUDE.md's Naming section repeats them), but `src-tauri/src/platform/mod.rs:59-61` (`ipc_dir`) resolves `/run/user/<uid>/nergal/` on Linux with fallback `<home>/.local/share/nergal/ipc/` and — per its own doc comment — "NEVER falls back to a guessable directory under shared `/tmp`". The documented paths don't exist; anyone debugging IPC follows them into a wall (and the /tmp claim misdescribes a deliberate security property).
- **(c) `README.md` Quick start (~117-133)** gives build steps with none of the required system deps — `librsvg2-dev`, `patchelf`, gstreamer bits — that `pnpm tauri build` needs for a working AppImage (known-good list already exists in `release.yml:48-59` and project memory).

## What Changes

- **`docs/hooks.md`**: replace the `cargo install` instruction (53) with the sanctioned flow (`pnpm tauri build` + `sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb`, per CLAUDE.md); correct every IPC path (9, 11, 24 and any others in the file) to the real `ipc_dir` resolution (`/run/user/<uid>/nergal/` + fallback, socket/FIFO names within it).
- **`CLAUDE.md` Naming section**: same IPC-path correction (`/tmp/nergal*.sock` + `/tmp/nergal-plan-*.fifo` → the `ipc_dir`-based paths).
- **`README.md` build section**: add the Linux system-deps prerequisite block (apt list from `release.yml:48-59`, including the `GSTREAMER_*` env note from project memory) before the production-build step.

## Capabilities

### New Capabilities

- `contributor-docs-accuracy`: setup, IPC, and install documentation matches the shipped implementation (no existing docs-scoped spec capability exists — verified against `openspec/specs/`).

### Modified Capabilities

_None._

## Impact

- **Files**: `docs/hooks.md`, `CLAUDE.md` (Naming section lines only), `README.md` — documentation-only diff, no code.
- **Risk**: LOW; the only care point is transcribing the real `ipc_dir` semantics faithfully (verify against `platform/mod.rs` at edit time, including the macOS/Windows variants the doc may want to mention in one line).
- **Out of scope**: broader docs audit; restructuring docs/; the `ci-quality-gates` change's shared apt-list action (if it lands first, README can point at the composite action as the canonical list).
