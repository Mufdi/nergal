# contributor-docs-accuracy

Setup, IPC, and install docs match the shipped implementation.

## ADDED Requirements

### Requirement: Contributor docs describe the real system

Contributor documentation (README build steps, `docs/hooks.md`, CLAUDE.md operational sections) SHALL match the shipped implementation: the documented binary reinstall flow SHALL be the sanctioned one (`pnpm tauri build` + package install — never `cargo install --path`), documented IPC paths SHALL be the `ipc_dir()` resolution actually used (`/run/user/<uid>/nergal/` on Linux with the `~/.local/share/nergal/ipc/` fallback — never `/tmp`), and the README build section SHALL list the system dependencies `pnpm tauri build` requires for a working AppImage.

#### Scenario: contributor follows hooks.md reinstall

- **WHEN** a contributor follows `docs/hooks.md`'s reinstall instruction after editing `hooks/cli.rs`
- **THEN** they run the `pnpm tauri build` + `dpkg -i` flow and do not create a shadowing `~/.cargo/bin/nergal`

#### Scenario: IPC debugging finds the sockets

- **WHEN** a contributor looks for the hook socket or plan FIFO at the documented paths
- **THEN** the documented paths are where the running app actually binds them

#### Scenario: fresh clone builds a working AppImage

- **WHEN** a contributor on a fresh Ubuntu follows the README build section
- **THEN** the listed system deps are sufficient for `pnpm tauri build` to produce a working AppImage
