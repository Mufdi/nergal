# Architect Brief — platform-desktop

**Project mission**: Nergal — Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. **Now porting to macOS + Windows, macOS first.**

## Control metadata
- **Tier** M · **ceremony** standard · **risk** medium · **files** ~6 · **visibility** public
- **Capabilities**: `platform-desktop-integration` (new) + `scratchpad` (modified delta)
- **Port sequence**: 3 of 5.

## What this change owns
Swap Linux-only desktop integration to cross-platform Tauri plugins: `tauri-plugin-opener` (open file/url + reveal-in-file-manager, replacing `xdg-open`/`xdg-user-dir`/`xdg-mime`/`gtk-launch` + D-Bus FileManager1 `ShowItems`) and `tauri-plugin-notification` (replacing the 3 `notify-send` sites: general, Linear, ClickUp). Downloads dir via `dirs::download_dir()`. **Lowest-risk cluster — mechanical.**

## Dependencies & sequencing
- **Depends on**: `platform-compile` (compile baseline). Independent of platform-proc/ipc.
- **Blocks**: nothing.

## Gating decision (Mode B)
- iprev **triaged** (files ≥ 5) but mechanical → may waive to a single code-quality review.
- Reviewer: single-sequential. Gates: compile + test + lint.

## Reference skills (A6)
- `tauri-app-dev` — Tauri plugin wiring (capabilities/permissions for opener + notification).

## Constraint
- The deep-link URI open path (`obsidian://`, `nergal://`) must keep working through the opener plugin.
- Do **not** claim full `zbus` removal: keyring's `async-secret-service` feature still pulls zbus transitively on Linux. Only the reveal path drops its direct D-Bus use.
