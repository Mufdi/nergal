# Design — platform-bundle-ci

Design settled 2026-06-26. All decisions are final; this records the *why* behind them.

## Context

Nergal's release pipeline today is a single `build-linux` CI job on `ubuntu-22.04`. The multiplatform port scoping (vault: `Projects/nergal/Multiplatform port scoping.md`) identified macOS (Apple Silicon) as the first non-Linux target because the IPC seam is already Unix sockets (compatible with macOS with zero changes) and the GH Actions macOS runner is available without additional infrastructure.

Two constraints shape everything:
1. **No Apple Developer account yet**: bundles will be unsigned at the OS level. Gatekeeper will warn on first launch; users bypass it once via right-click → Open. This is documented and acceptable for an early adopter audience familiar with CLI tools.
2. **Tauri signer (minisign) is still active**: the `.app.tar.gz.sig` file provides artifact integrity for the `tauri-plugin-updater` flow. Apple OS-level signing and minisign signing are orthogonal.

## Goals / Non-Goals

**Goals:**
- Ship a working `.dmg` + `.app` on every `v*` tag push from CI, without human intervention beyond pushing the tag.
- `latest.json` covers both `linux-x86_64` and `darwin-aarch64` so in-app update checks work cross-platform.
- Linux release path unchanged end-to-end; existing Linux users unaffected.
- Deferred signing (Apple notarization + Windows cert) documented as a human-gated follow-up with the exact steps needed.

**Non-Goals:**
- Apple code signing or notarization (deferred).
- Windows bundling or code signing (later iteration).
- Universal binary (x86_64 + arm64 fat binary); targeting Apple Silicon only via `macos-latest`.
- Self-hosted runners, custom signing infrastructure, or Homebrew Cask publishing.
- Any UI changes beyond adding `MacApp` to the `InstallSource` enum.

## Decisions

### D1: Three-job CI structure (build-linux | build-macos | publish)

**Alternatives considered:**
- *Single job with OS matrix + condition branches*: `strategy.matrix.os: [ubuntu-22.04, macos-latest]` and skip steps by OS. Rejected — the Linux and macOS dep installation steps are fundamentally different (apt-get vs nothing); matrix branches would produce one release-creation attempt per runner, requiring an extra condition to gate it. Logic becomes brittle.
- *Two jobs, linux creates the release with macOS artifacts uploaded later*: race condition on asset upload; GH API does not guarantee asset order in release creation.
- *Three jobs* (chosen): `build-linux` and `build-macos` are independent and parallel. `publish` has `needs: [build-linux, build-macos]` and fires exactly once after both succeed. Clean dependency graph; failure in either build job blocks the publish. Mirrors the pattern used by many Tauri apps in the wild (e.g., the official `create-tauri-app` CI template).

### D2: Platform JSON fragments assembled by publish job

Each build job emits a per-platform JSON fragment (`linux-platform.json`, `macos-platform.json`) via `actions/upload-artifact`. The `publish` job downloads them and calls `scripts/merge-latest-json.mjs` to produce the final `latest.json`.

**Alternatives considered:**
- *Inline the merge into the `generate-latest-json.mjs` script and run on the publish runner*: the script currently reads artifact paths from the local filesystem (`src-tauri/target/release/bundle/appimage/`). On the publish runner those paths don't exist (artifacts were built on the other runners). Would need to pass all artifact paths as CLI arguments — messy and fragile.
- *Each runner generates a complete `latest.json` and they're merged*: merge logic must reconcile two full manifests; `version`, `notes`, and `pub_date` duplication creates a conflict vector.
- *Fragment approach* (chosen): each runner emits only its platform entry (`{platform: "linux-x86_64", signature: "...", url: "..."}`) plus the shared fields (`version`, `notes`, `pub_date`). `merge-latest-json.mjs` is a simple merge with a shared-fields check.

### D3: Target darwin-aarch64 only (no Intel, no universal binary)

`macos-latest` on GitHub Actions is currently `macos-15` (Apple Silicon, arm64). Intel (`macos-13`) is still available but being deprecated. The user base for a developer tool like Nergal on Intel Mac is small and shrinking. Targeting `darwin-aarch64` via `macos-latest` is the highest-value, lowest-complexity starting point.

**Intel follow-up**: adding a `macos-13` runner job and a `darwin-x86_64` platform entry to `latest.json` is additive — it slots in as a fourth job with no structural changes to the CI design.

**Universal binary**: building a fat binary via `cargo lipo` adds CI complexity and doubles compile time; not warranted until we know Intel demand.

### D4: macOS update flow mirrors .deb (manual download), not AppImage (auto-install via plugin)

The `tauri-plugin-updater` CAN do fully automated update installation on macOS (`.app.tar.gz` replace), but this requires the update to pass Gatekeeper, which requires Apple notarization. Without notarization, an auto-installed update would re-trigger the Gatekeeper warning on next launch — a confusing regression.

**Decision**: `MacApp` install source → About page offers "Download .dmg and open in Finder" (same as `.deb` flow), not auto-install. When notarization is added, the About page can be upgraded to invoke the plugin's install path with a one-line change.

The minisign signature is still verified by the updater at download time (artifact integrity), which is the more important security property.

### D5: Cargo.toml and lib.rs untouched by this change

The `tauri-plugin-updater` plugin was already added in `release-ci-signed`. No new Rust crates are needed. The macOS detection (`InstallSource::MacApp`) is a pure Rust enum variant + match arm addition in `updater.rs` — no new dependencies, no plugin initialization changes.

### D6: Deferred signing documented in CLAUDE.md and this design

The deferred gate covers:
- **Apple notarization**: Apple Developer Program ($99/yr) → Developer ID Application cert → `notarytool` submission. GH secrets needed: `APPLE_CERTIFICATE` (base64 p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ID_PASSWORD`.
- **Windows code signing**: EV cert (~$300-500/yr) or cloud signing service. Out of scope for this change entirely.

These are NOT tasks in this change. They are documented as follow-up human gates in `CLAUDE.md` and in tasks.md's "Deferred" section.

## Risks / Trade-offs

**[Risk] Gatekeeper blocks first run on macOS** → Mitigation: document the right-click → Open workaround in the GitHub release body. This is standard practice for open-source tools without Apple signing. After notarization is added in the follow-up, this risk disappears.

**[Risk] macOS CI cold build time (~15-20 min) exceeds GH Actions free-tier limits** → Mitigation: Cargo cache per OS reduces subsequent builds to ~7-10 min. At Nergal's release cadence (1-2/week) this is well within limits. `macos-latest` runners cost 10× more than Linux on GH Actions paid plans; acceptable for a release-only workflow.

**[Risk] `tauri.conf.json` `bundle.targets: "all"` may produce unexpected Windows artifacts on the macOS runner** → Mitigation: Tauri 2 only builds targets appropriate for the current host OS even with `targets: "all"`. Verified by Tauri docs; confirmed by inspecting the bundle output paths on macOS.

**[Risk] macOS app crashes on launch due to missing native runtime** → Mitigation: `minimumSystemVersion: "12.0"` ensures WebKit (built into macOS) and system frameworks are at known-good versions. Tauri 2 supports macOS 10.15+ but 12.0 is the practical floor for M1-era features. Set conservatively; can be lowered if needed.

**[Risk] `generate-latest-json.mjs` refactor breaks existing Linux path** → Mitigation: the Linux fragment emission is an additive mode; the script retains its original `main()` path for local use. The fragment-emission mode is a separate `emitFragment()` function invoked by CI. Regression tested: existing `pnpm release:test` must still pass.

## Migration Plan

1. macOS config and Rust changes land in the same commit as the CI refactor — no intermediate state where CI is broken.
2. The first CI run with the new workflow will be a cold cache build; expected ~20 min; monitor for failures.
3. If the macOS job fails after the Linux job has uploaded its artifacts, the `publish` job does not fire and no partial release is created. The tag can be deleted and re-pushed after fixing the issue.
4. Rollback: revert the CI YAML to the single-job structure. Linux release path is preserved identically; rollback removes macOS support but does not break Linux users.

## Open Questions

- None at design time. macOS bundle path patterns (`src-tauri/target/release/bundle/macos/`) should be confirmed on the first CI run; the implementation plan cites the Tauri 2 documented paths.

## Deferred: OS-level signing (human-gated follow-up)

**Apple notarization** — follow-up steps after obtaining Apple Developer Program membership:
1. Generate a Developer ID Application certificate via developer.apple.com.
2. Export as `.p12`, base64-encode, and store as GH secret `APPLE_CERTIFICATE` (with `APPLE_CERTIFICATE_PASSWORD`).
3. Add `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ID_PASSWORD` secrets.
4. Add a `codesign` + `notarytool submit --staple` step to the `build-macos` CI job after `pnpm tauri build`.
5. Cut a release. Gatekeeper warnings disappear for new downloads.
6. Upgrade the About UI to use `tauri-plugin-updater` auto-install flow for `MacApp` (one-line change in the install-source branch).

**Windows** — a later iteration: a fourth CI job on `windows-latest`, `bundle.windows` config, NSIS/MSI, `windows-x86_64` entry in `latest.json`, EV code-signing cert.
