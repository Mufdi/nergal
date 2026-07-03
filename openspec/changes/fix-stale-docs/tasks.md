## 1. docs/hooks.md

- [ ] 1.1 Replace the reinstall instruction (line 53) with the CLAUDE.md-sanctioned flow (`pnpm tauri build && sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb`), including the one-line WHY (cargo install shadows the launcher binary and skips frontend bundling).
- [ ] 1.2 Correct every IPC path in the file (lines 9, 11, 24 + a grep for `/tmp/nergal` catches strays) to the real resolution: `/run/user/<uid>/nergal/` (Linux; systemd-owned, un-squattable) with fallback `~/.local/share/nergal/ipc/`; verify names/semantics against `src-tauri/src/platform/mod.rs:59-71` at edit time; add one line for the macOS/Windows equivalents per the same module.

## 2. CLAUDE.md

- [ ] 2.1 Naming section: replace `/tmp/nergal*.sock` + `/tmp/nergal-plan-*.fifo` with the `ipc_dir()`-based paths (keep the sentence shape; this is a surgical path correction only).

## 3. README.md

- [ ] 3.1 Add a "Linux build prerequisites" block before the production-build step: the apt list from `release.yml:48-59` (`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libappindicator3-dev librsvg2-dev patchelf gstreamer1.0-plugins-base gstreamer1.0-plugins-good`) + the `GSTREAMER_PLUGINS_DIR` env note for AppImage.

## 4. Verification

- [ ] 4.1 `grep -rn "/tmp/nergal" docs/ README.md CLAUDE.md` returns nothing; `grep -rn "cargo install --path" docs/ README.md` returns nothing outside historical/archive content.
- [ ] 4.2 Read-through: each corrected claim spot-checked against the source it describes (`platform/mod.rs`, release workflow, CLAUDE.md install table).
