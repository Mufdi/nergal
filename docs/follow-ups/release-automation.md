# Release automation — follow-up

Goal: take the manual release flow we just ran for `v0.1.1` and turn it into a single command (`pnpm release patch` / `minor` / `major`) plus a CI workflow that uploads bundles to a GitHub release.

## What's manual today

For each release we did, by hand:

1. Bump `version` in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, refresh `Cargo.lock`.
2. Append a new section at the top of `CHANGELOG.md` (flat bullets, action-verb prefixes, code in backticks — match the format already in the file).
3. Commit `chore(release): vX.Y.Z`, push to `main`.
4. Tag `vX.Y.Z`, push tag.
5. Run `GSTREAMER_PLUGINS_DIR=/usr/lib/x86_64-linux-gnu/gstreamer-1.0 pnpm tauri build` (~3 min). Verify `target/release/bundle/{deb,rpm,appimage}/Nergal_X.Y.Z_*` artifacts exist.
6. `gh release create vX.Y.Z --title "Nergal vX.Y.Z" --notes-file <body.md> <three artifact paths>`.
7. `gh release edit v<previous> --notes-file <prev-with-banner.md>` to surface "Latest release →".

Friction: easy to forget a version bump (Cargo.lock especially), the release body / changelog drift in tone, and the previous release body has to be re-edited every time.

## Target shape

**Local-driven**:
- A `scripts/release.mjs` (Node) or `bin/release.sh` that takes a bump kind (`patch|minor|major`) or explicit version.
- Bumps the three version files, regenerates `Cargo.lock` via `cargo metadata`, opens `$EDITOR` on the new CHANGELOG section pre-filled with the commit subjects from `git log <prev-tag>..HEAD`.
- Commits, tags, pushes both — stops there. The GitHub Actions workflow takes it from the tag.

**CI build**:
- `.github/workflows/release.yml` triggers on `v*` tags.
- Runs on `ubuntu-22.04` (we need WebKitGTK 4.1).
- Installs `librsvg2-dev`, `patchelf`, `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`.
- Caches `~/.cargo/registry`, `~/.cargo/git`, `src-tauri/target` keyed on `Cargo.lock`.
- Builds with `pnpm tauri build`, uploads `.deb`/`.rpm`/`.AppImage` as release assets via `softprops/action-gh-release@v2` (or `gh release upload` directly).
- Auto-derives the release body from `CHANGELOG.md` (`awk` between version markers).

**Previous-release banner update**:
- Either a step in the same workflow (`gh release edit <prev>` with a banner pointing to the new tag), or a separate `bin/update-prev-release.sh` we run after.
- Keeps a small template at `docs/follow-ups/release-banner.tpl.md`.

## Open questions

- Do we want signed bundles? Tauri supports signature keys for the updater, which we don't ship yet but probably should.
- Should the CI also publish to a Flatpak / AUR / Snap channel later? Not for v0.x.
- Where does the release blurb live: in the commit messages (so the script grabs them), in `CHANGELOG.md` directly, or hand-written in a release-notes file per tag? The CC-style flat-bullet changelog is short enough that "extract from CHANGELOG" is probably enough.

## When

Not blocking — patch releases keep being manual until this lands. Trigger: as soon as we cut v0.1.2 and feel the manual steps again.
