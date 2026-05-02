=== Round 1 (claude) ===
# Independent Review — Scratchpad Floating Panel

## Critical

1. **[critical] Ctrl+L collision with terminal `clear` is a real regression, not a free shortcut.** §"Shortcuts" claims `Ctrl+L registrado en stores/shortcuts.ts (verificado libre)`. In a Claude Code wrapper, Ctrl+L is the bash/zsh "clear screen" sequence (`\x0c`). cluihud's terminal uses `attachCustomKeyEventHandler` that returns `false` for any registered cluihud shortcut, so registering Ctrl+L will silently kill `clear-screen` for every PTY session. "Verificado libre" against `shortcuts.ts` is not enough — it must also be verified against terminal-canonical bindings. Either reassign (Ctrl+Alt+L, Ctrl+Shift+L, Ctrl+`) or carve a focus exception so Ctrl+L only triggers when focus is outside the terminal.

2. **[critical] No symlink / path-traversal validation on `scratchpadPath`.** §Implementation step 3 says "validar paths (no escape de scratchpadPath)" but doesn't specify how. Concrete attack surfaces: (a) symlink inside `scratchpadPath` pointing to `~/.ssh/`; the editor will read+overwrite arbitrary files since the path "looks inside". (b) `scratchpadPath` itself set to `/etc` or `~/`; soft-delete creates `.trash/` inside the user's home root. (c) Vault sync scenario: an Obsidian plugin places a symlink — cluihud silently follows it. Required: `canonicalize` resolved path, assert it is `starts_with(canonicalize(scratchpadPath))`, refuse symlinks for write ops. Plan does not commit to this.

## High

3. **[high] Epoch-in-filename vs mtime inconsistency in purge logic.** §"Soft-delete" specifies `scratch-{uuid}-trashed-{epoch}.md` *because* of mtime quirks/clock skew. §Implementation step 11 then says "tarea en startup que escanea `.trash/` y borra archivos con mtime > 30d". Verification step also says "tocar archivo en `.trash/` con `mtime` >30d". Either the design is right (parse epoch from filename) or the implementation is right (mtime). Pick one and make verification match. As written, the implementation undoes the design rationale.

4. **[high] Orphan DB rows on `scratchpadPath` change.** §"Storage abstraction" claims metadata is path-independent because keyed by UUID. But after a path change, every row in `scratchpad_meta` references a UUID for which no file exists in the new path. §"Cambiar el path" says "no migra files" — fine, but it doesn't specify DB behavior either. Are rows deleted? Marked dormant? Left dangling? On revert to the old path, do they auto-rehydrate? This is undefined and will produce zombie tabs or lost ordering. Spec the behavior explicitly: probably delete rows on path change, accept that revert loses position/order metadata.

5. **[high] Dirty-buffer flush on path change is racy.** §"Cambiar el path cierra todas las tabs abiertas, hace flush de buffers no guardados al path antiguo, y recarga desde el nuevo path." Autosave is debounced 300ms async — a flush triggered immediately may not have queued state from the last keystroke, and even if it does, write completion is not awaited before the close+reload. Required: synchronous flush (await the in-flight write + force any pending debounced writes) **before** closing tabs. Otherwise the last few characters typed before changing path are dropped silently.

6. **[high] `tmp + rename` atomicity is undefined re: tmp location.** §Implementation step 1 says "atomic via `tmp + rename`" but doesn't say where the tmp file lives. If tmp is on a different filesystem from `scratchpadPath` (likely if user points it into an Obsidian vault on a separate mount), `rename(2)` falls back to non-atomic copy — partial writes possible on crash. Specify: tmp file must live in the **same directory** as the target (`.scratch-{uuid}.md.tmp`), and cleanup of orphan tmp files on startup.

7. **[high] Watcher will index everything in `scratchpadPath` if pointed at a vault.** Plan does not specify whether `scratchpad_list_tabs` filters to `scratch-{uuid}.md` pattern or accepts any `.md`. With Obsidian sync explicitly listed as a goal, pointing at a vault means hundreds of unrelated notes show up as tabs. And `.trash/` (dot-prefix) needs to be excluded from the watcher and listing. Specify the filter precisely; wildcard `*.md` is not what users will want.

8. **[high] Own-write tracking 500ms window is not robust enough to call "load-bearing for sync".** §"Watcher" hard-codes 500ms. With slow disk, FUSE mounts (Obsidian on iCloud Drive, Syncthing), or backpressure on the notify queue, events can arrive >500ms after the write. The autosave→clobber loop the plan claims to avoid is exactly what will happen on those filesystems. Either: track a content hash of the last self-write and compare on event (correct), or use a higher-level lockfile, or accept the limitation and mark Obsidian sync as not v1-supported instead of claiming the watcher "destraba" it.

## Medium — Extensibility claim audit (per requester focus)

9. **[medium, ornamental] "Storage abstraction" via `ScratchpadEntry { id, displayName, kind, getContent, setContent }` is not actually built.** §"Storage abstraction" describes this interface in prose, but the file list and Tauri command list (`scratchpad_read_tab`, `scratchpad_write_tab`, etc.) commits to filesystem-only behavior. Adding a Linear provider later requires either new commands or a refactor of these to dispatch on `kind`. To make this load-bearing in v1, either (a) actually implement a frontend-side `Provider` interface and have the local-md provider be one of N, or (b) drop the claim. As written it is an aspirational comment, not a hook.

10. **[medium, ornamental] "Custom naming sin migración" is misleading.** §"Naming layer" says adding `custom_name TEXT NULL` is "sin migración". An `ALTER TABLE ADD COLUMN` *is* a migration (it touches `db.rs`, schema_version, and any tooling around it). The claim should be "additive, low-risk migration" not "without migration". Minor but it's exactly the kind of ornamental phrasing the requester asked to flag.

11. **[medium, partially load-bearing] `<FloatingPanel panelId>` wrapper not actually scaffolded.** §"Floating panel chrome reutilizable (load-bearing)" hinges on the wrapper being a separate component. The §Impact file list shows `ScratchpadPanel.tsx` only — no `FloatingPanel.tsx`. Without the wrapper actually existing in v1, adding a "second floating tool" later means extracting drag/resize/persistence from `ScratchpadPanel`, which *is* a refactor. The schema being multi-row keyed on `panel_id` is genuinely load-bearing; the component reusability claim is not until the wrapper file is in the build. Add `src/components/floating/FloatingPanel.tsx` (or similar) to the file list and have `ScratchpadPanel` consume it from day one.

12. **[medium, partially load-bearing] Send-to-prompt adapter is plausible but the integration surface isn't specified.** §"Send-to-prompt deferred" hinges on the adapter being able to read CodeMirror selection from outside the editor. CodeMirror's `EditorView` is typically held in a ref local to the editor component. Unless the plan exposes either (a) a `currentSelectionAtom` updated on every selection change, or (b) a `viewRef` ref to the editor, the adapter cannot work and the day-of-implementation requires touching `ScratchpadEditor.tsx`. Spec the seam now.

## Medium — General

13. **[medium] Dynamic renumbering ("Scratch N" by position) is a UX bear-trap not acknowledged.** Close the middle tab → "Scratch 3" becomes "Scratch 2". A user with mental shortcut "my idea is in Scratch 3" loses it instantly. The plan calls this out as the design but does not weigh the cost. Two cheaper options: (a) "Scratch N" by `position` only assigned at creation, never recomputed (gaps allowed); (b) display only "Scratch" + content snippet. Reconsider before shipping.

14. **[medium] Empty-state and first-launch behavior unspecified.** What happens when the user first toggles Ctrl+L with no `scratchpadPath` directory? When all tabs are closed? When `scratchpadPath` is deleted externally while panel is open? Spec these edge cases.

15. **[medium] Panel position/visibility on viewport changes unspecified.** Multi-monitor disconnect, window resize smaller than panel coords, Wayland fractional scaling — any of these can leave the panel off-screen with no way to recover except clearing the DB row. Add a clamp on load: if persisted geometry is outside current viewport, reset to centered default.

16. **[medium] Esc routing collisions not addressed.** Plan says "Esc oculta cuando está enfocado [...] no roba el Esc global". But `AskUserModal`, command palette, plan modal, conflicts dialog all consume Esc. There is no documented order/precedence. Specify how: probably `event.target` containment check on the floating panel's root element, with `stopPropagation`.

17. **[medium] DB migration mechanism not described.** Plan adds two new tables but doesn't reference the existing migration pattern in `db.rs`. Provide the migration version bump explicitly (forward only, since SQLite). Note also: the `floating_panel_geometry` table name in §Impact differs from the `scratchpad_panel` name in §Implementation step 2 — pick one.

## Low

18. **[low] No bound on tab count.** 100 tabs in the bar overflows horizontally. Spec a max or scroll/wrap behavior.

19. **[low] Reveal button command injection surface.** Passing `scratchpadPath` to `xdg-open` is fine if escaped; `Command::new("xdg-open").arg(path)` is safe but `bash -c "xdg-open $path"` is not. State which approach.

20. **[low] Purge runs unconditionally on startup without notice.** A user away for a month loses all trash with no recovery. Acceptable but worth a one-line toast: "Cleared N old scratchpad notes".

21. **[low] Test coverage is backend-only.** No React component tests, no Tauri command-level integration tests, no automated assertion that Ctrl+L doesn't escape into the PTY. The "verificación manual mínima" list is helpful but not part of CI.

## Summary of must-fix vs nice-to-fix

Must-fix before code: 1, 2, 3, 4, 5, 6, 7, 8, 11, 17 (the table-name inconsistency).
Should-fix in scope: 9, 10, 12, 13, 14, 15, 16.
Defer: 18–21.

VERDICT: REVISE


=== Round 2 (claude) ===
# Independent Review — Round 2 (Scratchpad Floating Panel)

## What's resolved

R1 issues 1, 2 (mostly), 3, 4, 5, 6, 7, 8 (now hash-based), 10, 11, 12, 13, 14, 15, 16, 18, 19, 20 are addressed. The hash-based own-write tracking, atomic-rename-in-same-dir, content-hash collision check, ScratchpadPanel-on-FloatingPanel wrapper, `currentScratchpadSelectionAtom` seam, and Ctrl+Alt+L choice are all genuine improvements that match the spirit of the prior feedback.

## Still broken / new issues

1. **[critical] Table-name inconsistency NOT fixed — same defect as R1#17.** §Impact explicitly states "`floating_panel_geometry` … este es el único nombre canónico de la tabla en todo el proposal." But §Implementation step 2 still says: "DB schema: migration para `scratchpad_meta` + `scratchpad_panel`." The note contradicts the very next instance of the name. This was specifically flagged as must-fix in Round 1; carrying it forward unchanged is a process failure, not a typo. Pick `floating_panel_geometry` and replace step 2's `scratchpad_panel` literal.

2. **[high] Own-write hash tracking only stores the *last* hash — race under rapid autosaves.** §Watcher specifies "guarda el SHA-256 del contenido escrito per-file … si matchea el **último** propio → ignorar". Sequence: autosave writes A (rename → notify queued), user types again, autosave writes B (rename → notify queued, last_hash now = B). When notify event for the A rename arrives (debounced-up but possibly delayed), the file on disk now contains B; comparison `hash(B) == last_hash(B)` → ignored. So far OK. But on FUSE/Syncthing/iCloud where notify can be re-ordered or coalesced inconsistently, the file may briefly contain A while last_hash is B → treated as external edit → spurious "external change" toast or, worse, conflict-marked dirty buffers. Required: keep a small ring buffer (last N=8 self-hashes per file) and ignore if file hash is in the set. Cheap, eliminates the race.

3. **[high] Symlink refusal only applies to writes — read/list path still vulnerable.** §Impact path-validation says "refuse symlinks for write ops" and "verificar `metadata().is_symlink()` antes de cualquier write." Read/list/watcher emission paths are not constrained. An attacker (or an unwitting Obsidian plugin) can place `scratch-deadbeef…-….md` as a symlink to `~/.ssh/id_rsa`; cluihud's `scratchpad_list_tabs` enumerates it, the user sees "Scratch 5", clicks it, and the editor displays the SSH key. Even without writing, this is information disclosure. Require `is_symlink()` rejection on **every** FS op (read, list, watch event handling), not only writes.

4. **[high] First-launch logic only covers "no `scratchpadPath` setting" — not "setting present but dir missing".** §Empty-state covers "First-launch sin `scratchpadPath`: crear el directorio silenciosamente al primer toggle." But the common path-change failure ("user typed `~/Vault/scratch` but typo'd to `~/Vauls/scratch`") and "directory deleted while app closed" are not specified. Backend will get `ENOENT` on every list and the watcher attach itself fails. Spec: on `scratchpad_set_path` and on every toggle, ensure the directory exists (create if missing, surface error if creation fails — e.g., permission denied).

## Medium

5. **[medium] "Storage abstraction" heading is still ornamental.** §Extensibility hooks now honestly admits in prose: "Drop-in de provider alternativo (future shape, NOT implemented in v1) … Esto **sí** sería un refactor, no un drop-in." Good. But the section heading remains "Storage abstraction" as if a hook is being installed. Either rename the subsection ("Stable identity — provider abstraction deferred") or move the honest disclaimer to the section title. The current shape will read to a future engineer as "we have a storage abstraction" when we explicitly do not. R1#9 was about this exact ornamental framing.

6. **[medium] Re-watch on path change can leak watcher attached to deleted dir.** §Implementation step 4 + path-change flow doesn't specify ordering: drop old watcher → create new dir if missing → attach new watcher. If new dir creation fails (permission denied on `/etc`), we end up with no watcher at all and a UI claiming the new path is active. Spec the failure path: revert to old path and toast error.

7. **[medium] Watcher event on rapid `tmp → final` may briefly observe `.scratch-{uuid}.md.tmp` and emit it.** Filter in §Watcher matches `scratch-{uuid}.md`. Tmp file is `.scratch-{uuid}.md.tmp` — leading dot + `.tmp` suffix. The dot-prefix should be filtered in addition to `.trash/`. Confirm the regex/glob explicitly rejects dotfiles, otherwise the debouncer might emit a transient event for the tmp during slow renames.

8. **[medium] CI coverage still backend-only.** Step 1 added unit tests (good), but the previously-raised gaps remain unfilled in CI: no React component test for the floating panel, no Tauri integration test for `scratchpad_*` commands, no automated regression test that Ctrl+L in the terminal still emits `\x0c` (i.e., that we didn't accidentally swallow it). Step 10's "smoke test que `clear` sigue funcionando" is manual. Given Ctrl+L collision was the R1#1 critical, a regression test is worth its weight.

9. **[medium] DB migration version unspecified.** §Impact says "versión bump en `db.rs` siguiendo el pattern de migrations existente; forward-only" but doesn't pick a number. Acceptable to defer — but if `db.rs` already has version-N at HEAD, the proposal should commit to N+1 to avoid two parallel changes both bumping to N+1 (a real merge hazard given the active OpenSpec workflow).

## Low

10. **[low] Hash check on watcher event reads the file synchronously inside the notify handler.** Cheap for small notes, but if a user opens a 10MB file (or a vault sync drops one in matching the UUID pattern), the hash compare blocks the watcher loop. Cap file size on read or hash incrementally. Probably fine for v1; flag in code.

11. **[low] Off-screen rescue only runs on geometry load, not on viewport-change-while-open.** Display disconnect mid-session leaves the panel inaccessible until next launch. Add a `window.matchMedia` / `resize` listener that re-clamps. Nice-to-have.

12. **[low] `epoch` in filename is presumably seconds; spec it.** "Epoch embedded in filename" doesn't specify seconds vs millis. Pick one (ms is safer against future-purge collisions if many files trash within the same second), document the format.

## Summary

The Round 1 critical/high feedback is largely addressed and the extensibility-hook honesty is markedly improved. However, the **same table-name inconsistency** flagged as must-fix in R1#17 survived verbatim, the own-write hash strategy has a real race under rapid autosaves on slow filesystems, symlink refusal is incomplete on the read path, and the first-launch logic only handles one of two common cases.

These are mechanical fixes (1, 3, 4) and a small algorithmic adjustment (2). Worth one more revision pass.

VERDICT: REVISE


=== Round 3 (claude) ===
# Independent Review — Round 3 (Scratchpad Floating Panel)

## What's resolved (R2 → R3)

R2#1 (table name): fixed — `scratchpad_panel` literal gone, `floating_panel_geometry` is now used everywhere.
R2#2 (hash race): fixed — ring buffer of last 8 self-hashes per file.
R2#3 (symlink read path): fixed — "refuse symlinks en todas las ops" with `is_symlink()` on read/write/list/emit.
R2#4 (first-launch dir missing): fixed — three cases enumerated (no path, path-but-missing, deleted-while-open) with `mkdir -p` and revert.
R2#5 (ornamental "Storage abstraction"): fixed — renamed to "Stable identity (provider abstraction deferred)" with explicit "in v1 NO hay storage abstraction" disclaimer.
R2#6 (watcher leak on path change): fixed — step 4 spells out "ensure dir → drop old → attach new → emit listing", revert on any failure.
R2#7 (tmp dotfile in watcher): fixed — "cualquier dotfile (incluido `.scratch-{uuid}.md.tmp`)" in exclude list.
R2#9 (migration version): fixed — `migrations/006_scratchpad.sql` (current head: v5).
R2#10 (hash file size): fixed — 1 MB cap with rationale.
R2#12 (epoch unit): fixed — `epoch_ms`, milliseconds, anti-collision rationale documented.

## Remaining

1. **[medium] Step 1 test description contradicts the Watcher spec.** §Implementation step 1(d) says: "watcher no loopea sobre own-writes (validar `last_self_write_at` skip)." But the §Watcher section explicitly replaced timestamp-based tracking with the SHA-256 ring buffer (R2#2 fix). The test description still references the old `last_self_write_at` mechanism that no longer exists. This is exactly the kind of carry-forward inconsistency that survived two prior rounds (R1#17 / R2#1 table name); flagging it now to break the pattern. Replace with "validar que un notify event con hash en el ring buffer per-file no dispara reload."

2. **[low] Watcher filter pattern not committed as a literal regex.** §Watcher says "matchean `scratch-{uuid}.md` (UUID v4 hex)" but the actual regex (e.g. `^scratch-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\.md$`) is not pinned. Different implementers will interpret "UUID v4 hex" differently (with/without hyphens, version-nibble check, case). Pin the literal pattern (or "any UUID-shaped hex group" if version-nibble check is overkill) so the watcher filter can't drift from `scratchpad_create_tab`'s output.

3. **[low] Frontend test coverage still narrow.** R2#8 asked for React component + Tauri integration tests; R3 only adds the Ctrl+L PTY regression test (good, that was the most important piece). No automated test for: panel geometry persistence/restore, soft-delete → trash file presence, watcher → tab-changed event handling. Acceptable for a low-tier v1 if explicitly accepted as a deferred risk; the proposal does not currently call this out.

4. **[low] R2#11 (off-screen rescue on live viewport change) not addressed.** Still clamps only "al cargar la geometría persistida". Disconnect a monitor mid-session and the panel is unreachable until next launch. Defer is fine; nothing changed since R2.

5. **[low] §Stable identity rename-via-Obsidian behavior is hand-wavy.** "Si Obsidian renombra `scratch-{uuid}.md` a otro filename… el watcher detecta delete + create, vincula por UUID (lee del nuevo filename si matchea el patrón) o trata como nuevo file. Implementación específica en Mode B." If the new filename does **not** match the `scratch-{uuid}.md` pattern (Obsidian aliases, `01 - my scratch.md`, etc.), the watcher's filter rejects it and the file silently becomes invisible to scratchpad while the DB row dangles as a zombie. Acknowledge this edge or commit to "renames outside the pattern are treated as deletion + the DB row is purged on next watcher emit". As written, behavior on rename-to-non-matching is undefined.

## Extensibility hooks audit (load-bearing vs ornamental)

- **Stable identity (UUID)** — load-bearing (verifiable: filename + DB key are both UUID).
- **`floating_panel_geometry` multi-row schema** — load-bearing (`panel_id` PK from day 1).
- **`FloatingPanel.tsx` wrapper file** — load-bearing (now in §Impact file list, ScratchpadPanel composes it).
- **`currentScratchpadSelectionAtom`** — load-bearing (atom is created in v1 store; adapter is the only deferred piece).
- **Naming layer (display vs filename)** — load-bearing (display is `f(position)`, filename is UUID; honestly framed as needing a one-line ALTER for custom names).
- **Watcher as base of sync** — load-bearing for read-mostly, honestly disclaimed as not safe for concurrent writes.
- **Soft-delete as folder convention** — load-bearing (trivially extensible).
- **No coupling with sessions** — load-bearing (verifiable: `scratchpad.ts` does not import `activeSessionIdAtom`).
- **Provider abstraction** — explicitly NOT a hook in v1, honestly framed as future refactor.

No remaining ornamental claims.

## Summary

R2's four blockers (table name, hash race, symlink, first-launch) are all addressed cleanly. The extensibility-hook section is now honest: load-bearing items have verifiable artifacts (files, schemas, atoms), and the one item that isn't (provider abstraction) is explicitly disclaimed.

Issue 1 above (test description ↔ Watcher spec mismatch) is a documentation drift that will mislead the implementer to write a test for a mechanism that no longer exists, but it's fixable in seconds and not architecturally load-bearing. The remaining four are genuinely minor / acceptable defers.

VERDICT: APPROVED

