# Nergal

Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Naming

**Nergal** end to end — brand AND internal. The legacy internal name `cluihud` was fully renamed to `nergal` (binary `nergal`, hook subcommands `nergal hook ...`, env vars `NERGAL_SESSION_ID`/`NERGAL_AGENT_ID`, IPC paths `/tmp/nergal*.sock` + `/tmp/nergal-plan-*.fifo` + `/tmp/nergal-ask-*.fifo`, sentinel `~/.nergal-active`, config dir `~/.config/nergal/`, deep-link scheme `nergal://`, MCP server key `nergal`). A one-time startup migration (`src-tauri/src/migrate_legacy.rs`) moves any pre-existing `cluihud` local state (config dir, `~/.claude/settings.json` hook commands, sentinel, `nergal-state.json`, the conditional-hook wrapper, codex/opencode MCP registrations) to the new names so upgrading users lose nothing. `cluihud` survives only as the source-side literals inside that migration module and in archived `openspec/changes/archive/` records (historical, intentionally untouched). The local repo dir on the dev machine is still named `cluihud/` — that is just a filesystem path, not in-repo content.

## Scope — qué es y qué no es Nergal

Nergal corre **alrededor** del agente CLI, no en su lugar. Esto define el filtro de inspiración, no una lista cerrada de features.

**Estable (no negociable):**
- Siempre corre `claude` (u otro agent CLI) underneath en un PTY real. No reemplaza bash/zsh/tmux.
- No reimplementa primitives nativos del agente (slash commands, skills, agents, hooks como motor). Los **observa y augmenta** vía hook pipeline + transcript watchers.

**Abierto (evolutivo):**
- **Multi-agent / agent-agnostic** ya es estable (4 OpenSpec changes archivados 2026-05-04). BYOA, coordinator patterns, parallel agent comparisons, switching entre CC / Codex / Gemini → bienvenidos.
- **Surfaces alrededor del ecosistema del agente**: skills marketplace para discovery/install, MCP server propio para que el agente consulte nergal, usage dashboards, deep-link protocol — todos válidos. La línea está en "no reescribir lo que el agente ya hace", no en "ignorar el ecosistema".
- **Workflow integrations** (issue trackers, design tools, browser preview, voice input, Docker isolation) son evoluciones naturales si reducen fricción en el loop agente↔humano.

**Filtro para evaluar inspiración:**
- ¿Replica primitives del agente (tool calling propio, skill emergence, training loops)? → redirigir.
- ¿Augmenta el loop o organiza el ecosistema alrededor? → evaluar, no descartar por scope-creep.
- Tooling completamente ajeno (ej. IDEs, design tools, terminal-only utilities) puede aportar **patrones de UX** aplicables aunque la herramienta no se parezca a Nergal. La fuente es señal débil; la idea es la señal fuerte.

## Critical conventions

- **Read before Write/Edit.** Always read files before modifying.
- **Comments: WHY only, never WHAT.** Document non-obvious constraints, workarounds, invariants. Restating the next line is not a comment.
- **Keyboard shortcuts use `event.code`** (not `event.key`) — WebKitGTK Linux bug. Verify `src/stores/shortcuts.ts` before adding a binding (collisions silently break flows).
- **No `unwrap()` / `expect()` in Rust outside tests.** Propagate with `anyhow` and `?`.
- **No TODO/FIXME** — track in issues or OpenSpec changes.
- **Absolute paths in tool calls.**

## Verification commands

| Action | Command |
|---|---|
| Dev | `pnpm tauri dev` |
| Build | `pnpm tauri build` |
| Rust check | `cd src-tauri && cargo check` |
| Rust test | `cd src-tauri && cargo test` |
| Rust lint | `cd src-tauri && cargo clippy -- -D warnings` |
| Rust format | `cd src-tauri && cargo fmt --check` |
| TS check | `npx tsc --noEmit` |
| Full check | `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit` |
| Reinstall installed app | `pnpm tauri build && sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb` — covers Rust + frontend + bundled hook CLI in one shot. Do NOT use `cargo install --path src-tauri --force`: it puts a binary in `~/.cargo/bin/` that shadows `/usr/bin/nergal` for the GNOME launcher (user PATH is inherited) and skips the Tauri frontend bundling step, producing ghost windows. |

Run the full check after significant changes.

## Release commands

Two-step ship flow (see OpenSpec changes `release-script` + `release-ci-signed`):

1. **In a Claude session**: say "cortemos v0.1.X" (or equivalent). Claude reads `git log <prev-tag>..HEAD` + relevant diffs + BUG-NN entries from the just-archived working file, writes a contextual user-facing CHANGELOG section, and prepends it to `CHANGELOG.md`. **The section must be COMPLETE, not a highlights reel** — but it is reader-facing release notes, not a commit log. Cross-check BOTH `git log <prev-tag>..HEAD` AND the BUG-NN registry so nothing user-perceivable is dropped (internal-only churn — refactors, openspec archival, the mechanical half of a rename — stays out). Grouping rule: **distinct capabilities each get their own line** (e.g. each Linear sub-feature, each MCP capability), but **facets of one feature/surface share a line** (e.g. the ports-popover fixes, two status-bar-layout fixes, two "modal didn't close" fixes) — completeness without a 50-bullet wall.
2. **Run the script**: `pnpm release <patch|minor|major>` (or explicit `pnpm release 0.1.10`). The script verifies the CHANGELOG section is present, bumps versions in `package.json` + `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json`, refreshes `Cargo.lock`, commits `chore(release): vX.Y.Z`, tags, and pushes `main` + tag.
3. **CI takes it from there**: `.github/workflows/release.yml` (triggered on `v*` tag push) builds, signs (via `TAURI_SIGNING_PRIVATE_KEY` repo secret), and publishes `.deb` + `.rpm` + `.AppImage` + `.AppImage.sig` + `latest.json`. Then prepends a "Latest release →" banner to the previous release. Monitor at https://github.com/Mufdi/nergal/actions.

Versioning policy (when to pick `patch` vs `minor`, what counts as "breaking" while still in `0.x`, expectations for the eventual `1.0` cut) lives in the project's private notes — ask the user if you need it.

| Command | Behavior |
|---|---|
| `pnpm release <bump>` | Full release (live). |
| `pnpm release:dry` (or `pnpm release <bump> --dry-run`) | All guards + computation, no mutations. Useful for previewing the changelog echo + the would-be operations. |
| `pnpm release <bump> --no-push` | Local commit + tag, skip push. Relaxes the "must be on main" guard for testing on a throwaway branch. |
| `pnpm release:body <version>` | Print the CHANGELOG section for that version as the GH release body. Used by CI. |
| `pnpm release:banner <new-tag>` | Update the previous release's body with a forward-pointer banner. Used by CI. |
| `pnpm release:test` | Runs the pure-helper test suite for all three release scripts. |

Pre-flight guards (script aborts before any mutation if any fails):
- Working tree clean (except `CHANGELOG.md`, which is expected dirty after step 1)
- On `main` (unless `--no-push`)
- Previous tag exists locally
- New tag doesn't exist locally or on origin
- `CHANGELOG.md` has a `## v<new>` section at top

### First-time signing key setup (one-off)

```bash
pnpm tauri signer generate -w ~/.tauri/cluihud-updater.key   # interactive password prompt
```

Then add two repo secrets at https://github.com/Mufdi/nergal/settings/secrets/actions:
- `TAURI_SIGNING_PRIVATE_KEY` — entire content of `~/.tauri/cluihud-updater.key`.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password from above.

The pubkey is already pinned in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`. **Never commit the `.key` file.**

### Key rotation

Regenerate the keypair, swap both repo secrets, replace `plugins.updater.pubkey` in `tauri.conf.json`, then cut a new release. AppImages signed with the old key cannot verify updates signed with the new key — this is expected behaviour and the only mitigation is for affected users to download the next release manually from GitHub.

## Documentation TOC

Read on demand when working in the relevant area:

- [`docs/architecture.md`](./docs/architecture.md) — stack, file tree, IPC patterns, event flow.
- [`docs/conventions.md`](./docs/conventions.md) — Rust + React/TS coding standards.
- [`docs/hooks.md`](./docs/hooks.md) — hook system, plan-review flow, ask-user interception, settings.json snippet.
- [`docs/design.md`](./docs/design.md) — design system (visual): R0162 YAML tokens + components + decision rules. Read before touching UI.
- [`docs/patterns.md`](./docs/patterns.md) — interaction patterns: keyboard nav tiers, chip-strips, file picker, focus zones. Read before adding shortcuts or panel navigation.
- [`openspec/specs/`](./openspec/specs/) — feature contracts. Read the relevant spec before implementing or proposing a change.
