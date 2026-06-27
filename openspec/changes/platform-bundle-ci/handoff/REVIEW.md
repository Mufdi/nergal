# Review — platform-bundle-ci

_Reviewers write here during Mode B. Empty until execution._

## iprev RUN 2026-06-26 (platform-bundle-ci)

- **Persona**: architect (senior-engineer independent plan review)
- **Evaluator**: claude (`claude -p`, headless)
- **Rounds**: 2 → final **VERDICT: APPROVED**
- **Issue counts**: R1 = 2 critical, 2 high, 3 medium, 2 low (REVISE). R2 = 0 critical/high; 1 medium + 2 low residual (APPROVED). Residuals fixed post-approval.
- **Mode**: Mode A — plan artifacts only; no `src-tauri/src` or other source touched. Grounded against the live repo (read `release.yml`, `generate-latest-json.mjs`, `updater.rs`, `SettingsPanel.tsx`, `tauri.conf.json`, and the Tauri 2 `MacConfig` schema).

### Round 1 findings (REVISE) and how each was addressed

1. **[CRITICAL] `category` under `bundle.macOS` is an invalid key.** Tauri 2 `MacConfig` is `additionalProperties:false` and has no `category` member → fails config-schema validation and breaks `pnpm tauri build` on EVERY platform (config parse is platform-agnostic), including the Linux release path. The category is already covered by top-level `bundle.category`. → Removed `category` from task 1.1, implementation Phase 1, proposal, and the `platform-distribution` spec; macOS block now holds only `minimumSystemVersion`. Rationale documented inline; task 1.2 notes the macOS `pnpm tauri build` is the authoritative schema gate (JSON `require()` is insufficient).
2. **[CRITICAL] "No frontend changes" was false.** `SettingsPanel.tsx` gates downloads on a hard `installSource === "deb"` reading `debAssetUrl`; `sourceLabel` is a fixed map with no `mac_app` key; the TS `UpdateCheckResult` has no dmg fields; the plugin auto-install path is gated on `appimage`. A `mac_app` source would get NO affordance and render `undefined`. The spec itself mandated deb-parity, but there was no frontend task. → Added Phase **2B** (tasks 2B.1–2B.5 + implementation Phase 2B): extend the `InstallSource` union, add `dmgAssetUrl`/`dmgAssetSize` to the interface, add the `sourceLabel` entry, broaden the gate to deb-style download-and-reveal reading `dmgAssetUrl`, and explicitly forbid routing `mac_app` through the `appimage` plugin auto-install path (Gatekeeper regression). proposal Impact, design D4, and the spec updated.
3. **[HIGH] macOS `latest.json` paired the `.app.tar.gz.sig` signature with a `.dmg` URL** → malformed updater manifest (verification + install both fail). → Pinned `darwin-aarch64.url` to the `.app.tar.gz` updater artifact across proposal, design D4 (mechanism A vs B split), tasks 3.1/4.1/4.2/5.1, implementation Phase 5, and the `latest.json` spec requirement + scenario; `.dmg` stays out of the manifest (surfaced only via `check_app_update().dmg_asset_url`).
4. **[HIGH] Inconsistent `v`-prefix handling** between `emitFragment` (`version="<tag>"`) and the v-stripped release-body. → design D2 + task 3.1 pin `version` = raw v-prefixed tag to match the shipped `generate-latest-json.mjs:36`; merge aborts on `version` divergence; bare-vs-v question scoped out as pre-existing.
5. **[MEDIUM] Smoke test would publish a real "latest" release** (no `--prerelease`) and clobber the live banner; the `pnpm release 0.0.0-test1` variant contradicted the raw-tag step and would abort on the CHANGELOG guard. → New design **D7**: `gh release create` passes `--prerelease` and the banner step is skipped when `$GITHUB_REF_NAME` contains a `-` (semver pre-release); tasks 5.3/5.4 + a dedicated `release-ci-signed` scenario; removed the contradictory `pnpm release` command.
6. **[MEDIUM] `/tmp` fragments don't survive `upload`/`download-artifact`.** → New design **D8** + corrected publish steps to resolve from the download dir.
7. **[MEDIUM] Test 2.5 could not exercise the real `detect_install_source()`** (it reads `current_exe()`). → task 2.2 extracts a pure `install_source_for_path(exe_str, appimage_env)`; 2.5 unit-tests it directly plus Deb/Appimage/Dev regression assertions.
8. **[LOW] Verification suite blind to the criticals** (`tsc`/`require()`). → task 1.2 and N.1 note the authoritative gates (macOS build for schema; code-review + N.4 for the runtime `mac_app` path).
9. **[LOW] `category` redundant** — moot after #1.

### Round 2 residual findings (APPROVED) — fixed post-approval

- **[MEDIUM] `upload-artifact@v4` least-common-ancestor path retention** would bury a `/tmp`-written fragment under a `tmp/` subdir, contradicting the hardcoded `artifacts/<set>/<fragment>.json` merge paths. → Fragments are now written into the workspace and uploaded as their OWN single-file artifacts (`linux-platform`, `macos-platform`), so each downloads deterministically to `artifacts/<name>-platform/<name>-platform.json`. Updated tasks 5.1 + implementation Phase 5 + merge invocation.
- **[LOW] macOS fragment URL hardcoded `Nergal.app.tar.gz`** while Linux derived its filename dynamically. → macOS now derives the tarball name via `ls *.app.tar.gz | xargs basename` (symmetric with Linux).
- **[LOW] MODIFIED requirement sentence said the banner updates unconditionally.** → Qualified with "(for non-prerelease tags only)".

`openspec validate platform-bundle-ci --strict` passes after every revision.
