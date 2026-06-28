# Design — windows-bundle-ci

## Context

The release CI is a 3-job structure (`build-linux` + `build-macos` + `publish`) producing a merged `latest.json`. The macOS iteration established the pattern: a per-OS build job emits a `<platform>-platform.json` fragment + bundles, and `publish` merges the fragments and creates the release. This change adds the symmetric `build-windows` job + the `windows-x86_64` updater entry + the install-source/UI plumbing, completing the 3-platform release.

## Decision 1 — NSIS is the Windows updater target; `.msi` is manual-download

Tauri produces both a WiX `.msi` and an NSIS `*-setup.exe` from `targets: "all"`, plus updater artifacts. **Chosen**: the **NSIS** `*-setup.nsis.zip` (+ `.sig`) is the `windows-x86_64` updater artifact in `latest.json`; the `.msi` is a manual-download release asset only (mirroring the `.dmg`). NSIS is Tauri's first-class Windows updater path — `tauri-plugin-updater` downloads the `.nsis.zip`, extracts the installer, and runs it silently to update in place. The `.msi` cannot be an updater artifact (no in-place silent-update story), so pairing it in `latest.json` would break verification — exactly the `.dmg` lesson.

- *Alternative: WiX `.msi` as the updater artifact.* **Rejected** — not Tauri's updater path; the `.msi` is for admins/GPO deployment, shipped as a manual asset.

## Decision 2 — Windows routes through auto-install, NOT manual download (unlike macOS)

macOS deferred auto-install to notarization because Gatekeeper blocks an un-notarized `.app` from auto-launching. **Windows is different**: `tauri-plugin-updater` runs the minisign-verified NSIS installer, which works even unsigned — SmartScreen shows a one-time "Windows protected your PC" prompt (bypassable via "More info → Run anyway"), but the update proceeds. **Chosen**: the About UI routes `InstallSource::Windows` through the **`appimage` auto-install branch** (`tauri-plugin-updater`), giving Windows real in-app auto-update now, with a UI note about the SmartScreen prompt until Authenticode signing lands.

- *Alternative: mirror `mac_app` (manual `.msi`/`.exe` download + reveal).* **Rejected** — needlessly forfeits working auto-update; the SmartScreen prompt is a one-time bypass, not a hard block like Gatekeeper-on-unnotarized.

## Decision 3 — `build-windows` mirrors `build-macos`; no system-dep install

**Chosen**: `build-windows` on `windows-latest` mirrors `build-macos` — Rust stable, pnpm + Node, `pnpm install --frozen-lockfile`, `pnpm tauri build` with the minisign signing env, cargo cache keyed by `runner.os` (so Windows gets its own cache, no collision). **No `apt-get`/GStreamer** (Linux-only) and **no extra WebView2 install** — Tauri's default `webviewInstallMode: downloadBootstrapper` ships a small installer that fetches WebView2 at install time if absent (the runner already has it for the build). The job emits `windows-platform.json` via the existing `generate-latest-json.mjs --fragment windows-x86_64` (the script is platform-name-agnostic).

## Decision 4 — Windows Authenticode signing is a deferred human-gated follow-up

**Chosen**: ship unsigned (Authenticode) now, like the unsigned-macOS decision. The minisign signer still protects updater integrity. The follow-up gate (cert acquisition → GH secrets → a `signtool`/Tauri `bundle.windows` sign step in `build-windows`) is documented in the spec + CLAUDE.md. EV certs clear SmartScreen immediately; OV certs build reputation over downloads. Until then the one-time SmartScreen prompt is the only friction, and auto-update still works through it.

- *Note:* a pre-release tag (contains `-`) already publishes as a pre-release + skips the banner; the Windows job inherits that behaviour with no extra logic (it only adds assets to the same release).

## Decision 5 — `bundle.windows` config kept minimal

**Chosen**: rely on Tauri defaults (WiX + NSIS, `downloadBootstrapper`) — `targets: "all"` + `createUpdaterArtifacts: true` already produce everything. Add a `bundle.windows` section ONLY if the build surfaces a need (e.g. an explicit `nsis.installMode: "perMachine"` or a WebView2 mode change). The deep-link scheme (`plugins.deep-link.desktop.schemes: ["nergal"]`) and the AUMID (`identifier: com.nergal.app`) are already declared — the installer applies them, satisfying `windows-desktop`'s notification + deep-link prerequisites.

## Risks / Trade-offs

- **[Medium] SmartScreen friction** on the unsigned installer — one-time bypass; documented; resolved by the deferred Authenticode gate.
- **[Low] WebView2 absent on a target machine** — `downloadBootstrapper` fetches it at install; offline machines need `embedBootstrapper`/`offlineInstaller` (a follow-up if users report it).
- **[Low] No local validation** — `windows-latest` CI builds the bundle; the user's Windows machine walks install + auto-update. The first real Windows release is the smoke-test (as the macOS 3-job CI was — it caught real bugs).
- **[Low] `generate-latest-json.mjs` / `merge-latest-json.mjs` assumptions** — confirm both are platform-name-agnostic (they were written generic in the macOS iteration); a 3rd fragment must merge without a hardcoded 2-platform assumption.

## Migration / rollback

Additive: one CI job, `publish` wiring, a `latest.json` entry, an `InstallSource` variant + UI branch, CLAUDE.md docs. Git-revertible; a revert drops the Windows job + entry, leaving the 2-platform release intact.
