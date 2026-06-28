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
- **Cross-platform invariant.** Nergal targets Linux + macOS + Windows. Every new OS-specific seam is born `#[cfg]`-gated (`cfg(unix)` for POSIX, `cfg(target_os = "linux")` for Linux-only like D-Bus/`/proc`/Secret-Service) **with a stub for the other targets** (prefer `cfg(not(unix))` for the non-POSIX branch so a hypothetical third unix target never falls through a gap), never ungated. Prefer cross-platform crates (`which`, `dirs`, `sysinfo`) over shelling out to Linux binaries (`which`, `xdg-open`, `notify-send`). Two **complementary** CI gates in `.github/workflows/ci.yml` enforce this on every `src-tauri/**` PR: `macos-cross-check` (`cargo check --target aarch64-apple-darwin` on `macos-latest`) catches macOS-breaking **Linux-only** constructs, and `windows-check` (`cargo check --target x86_64-pc-windows-msvc` on `windows-latest`) catches **ungated `std::os::unix`/`libc` seams** that compile on macOS (a unix target) but break Windows. Neither runs on the Linux dev host: the macOS objc build scripts need the macOS SDK, and `ring`'s build script needs MSVC. Both green = the cross-platform contract holds; a new ungated unix seam now fails `windows-check` at PR time rather than silently shipping.

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
3. **CI takes it from there**: `.github/workflows/release.yml` (triggered on `v*` tag push) runs **four parallel build jobs** — `build-linux` (ubuntu-22.04), `build-macos` (macos-latest, Apple Silicon), and `build-windows` (windows-latest, NSIS+MSI) build and sign their respective bundles independently, then `publish` downloads all three artifact sets, merges the per-platform JSON fragments into a single `latest.json` (via `scripts/merge-latest-json.mjs`), creates the GitHub release with all assets (`.deb`, `.rpm`, `.AppImage`, `.AppImage.sig`, `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`, `.msi`, `*-setup.exe`, `*-setup.nsis.zip`, `*-setup.nsis.zip.sig`, `latest.json`), and prepends a "Latest release →" banner to the previous release. Tags containing a `-` (e.g. `v0.0.0-test1`) publish as pre-releases and skip the banner, protecting `/releases/latest`. Monitor at https://github.com/Mufdi/nergal/actions.

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

### Deferred: Apple notarization (macOS OS-level signing)

macOS bundles ship unsigned today (Gatekeeper warning on first launch; bypass via right-click → Open). When ready, follow these six human-gated steps:

1. Enroll in Apple Developer Program ($99/yr) and generate a Developer ID Application certificate at developer.apple.com.
2. Export the cert as `.p12`, base64-encode it, and add two GH repo secrets: `APPLE_CERTIFICATE` (base64 content) and `APPLE_CERTIFICATE_PASSWORD`.
3. Add three more GH secrets: `APPLE_SIGNING_IDENTITY` (e.g. `Developer ID Application: …`), `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ID_PASSWORD`.
4. Add a `codesign` + `notarytool submit --staple` step to the `build-macos` CI job in `.github/workflows/release.yml`, after `pnpm tauri build`.
5. Cut a release; Gatekeeper warnings disappear for new `.dmg` downloads.
6. Upgrade the About UI: change the `mac_app` branch in `renderUpdateButton` to call the `tauri-plugin-updater` auto-install path instead of the download-and-reveal flow (one-line change in `SettingsPanel.tsx`).

### Deferred: Windows Authenticode (Windows OS-level signing)

Windows bundles ship unsigned today (SmartScreen "unknown publisher" prompt on first launch; bypass via More info → Run anyway). The About UI already surfaces this (`installSource === "windows"` copy block). When ready:

1. Acquire an Authenticode code-signing certificate (OV ~$200/yr, or an EV cert/token for instant SmartScreen reputation) from a CA (DigiCert, Sectigo, …).
2. Export the cert as `.pfx`/`.p12`, base64-encode it, and add two GH repo secrets: `WINDOWS_CERTIFICATE` (base64 content) and `WINDOWS_CERTIFICATE_PASSWORD`.
3. Wire Tauri's built-in signing: set `bundle.windows.certificateThumbprint` (or use `signtool` directly) and add a sign step to the `build-windows` CI job after `pnpm tauri build`, or configure `tauri.conf.json` `bundle.windows.signCommand`.
4. Cut a release; SmartScreen warnings fade as the cert builds reputation (immediate with an EV cert).
5. The `windows` About-UI branch already uses the auto-install path — no UI change needed (unlike the `mac_app` step 6 above).

## Documentation TOC

Read on demand when working in the relevant area:

- [`docs/architecture.md`](./docs/architecture.md) — stack, file tree, IPC patterns, event flow.
- [`docs/conventions.md`](./docs/conventions.md) — Rust + React/TS coding standards.
- [`docs/hooks.md`](./docs/hooks.md) — hook system, plan-review flow, ask-user interception, settings.json snippet.
- [`docs/design.md`](./docs/design.md) — design system (visual): R0162 YAML tokens + components + decision rules. Read before touching UI.
- [`docs/patterns.md`](./docs/patterns.md) — interaction patterns: keyboard nav tiers, chip-strips, file picker, focus zones. Read before adding shortcuts or panel navigation.
- [`openspec/specs/`](./openspec/specs/) — feature contracts. Read the relevant spec before implementing or proposing a change.
