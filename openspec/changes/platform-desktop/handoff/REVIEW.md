# Review — platform-desktop

_Reviewers write here during Mode B. Empty until execution._

## iprev RUN 2026-06-26 (platform-desktop)

- **Persona**: architect (senior-engineer independent plan review)
- **Evaluator**: `claude` (CLI `claude -p`)
- **Mode**: A (plan artifacts only — no source edits)
- **Rounds**: 6 (5-round loop + 1 final-confirmation pass after a single mechanical fix)
- **Final verdict**: **APPROVED** (round 6)
- **`openspec validate platform-desktop --strict`**: passes

### Verdicts per round
| Round | Verdict | New issues raised |
|---|---|---|
| 1 | REVISE | 13 (3 HIGH, 1 MED-HIGH, 5 MED, 4 LOW) |
| 2 | REVISE | round-1 all resolved; 5 NEW (1 HIGH, 2 MED, 2 LOW) — introduced by the round-1 fixes |
| 3 | REVISE | round-2 all resolved; 6 NEW (3 MED, 3 LOW) — consistency drift from the notification reframing |
| 4 | REVISE | round-3 all resolved; 3 MED — stale unconditional phrasing in secondary artifacts |
| 5 | REVISE | round-4 all resolved; 1 MED — one pre-existing stale Risks bullet |
| 6 | APPROVED | flagged item reconciled; 2 LOW non-blocking nits noted (addressed) |

### Substantive revisions applied
- **Reconciled the whole plan against the post-`platform-compile` source tree** (the artifacts were drafted pre-`platform-compile`): `zbus` is Linux-gated at `Cargo.toml:116` (NOT top-level), `show_items_via_dbus` is `#[cfg(target_os = "linux")]`, `reveal_in_downloads` now has two `#[cfg]` branches that collapse into one opener call. Fixed every stale line reference and the "top-level zbus" framing across proposal/design/spec/tasks.
- **Replaced the silent-failure notification fallacy with a build-time empirical gate (Decision 3/3a, task 6.0)**: the historical WebKitGTK regression was a *silent* `.show()` (`Ok` with no display), which is undetectable at runtime — so a runtime `Err`-fallback (round-1/2 proposal) protects nothing. The Linux notification mechanism (plugin vs gated `notify-send`) is now chosen at build time by human observation; macOS is always the plugin. Rollback, Risks, spec scenarios, and task 8.2 grep guard all realigned to this; removed every "warn-log monitoring catches silent failure" claim.
- **Corrected the opener API model**: all six sites call the plugin **from Rust** via `OpenerExt` (`app.opener().*`), which bypasses the capability/scope ACL — so the `opener:*` capability entries are defensive, not load-bearing (the false "missing capability = runtime error" claim was removed). This makes the in-code `obsidian://`/`nergal://` scheme allowlist the *sole* security boundary (Decision 6/7), now mandated-preserved + unit-tested. Fixed the file-vs-directory mapping (scratchpad reveal uses `open_path` to preserve "open the folder", not `reveal_item_in_dir`, which was a latent behavior change).
- **Hardened completeness**: kept the `reveal_in_downloads` existence guard the spec requires; added trait-import task; grep-before-delete for `default_app_for_mime`; macOS notification permission sequenced in `setup()` before pollers spawn (with the startup-delay tradeoff documented); added hermetic unit tests for the allowlist + downloads fallback; added the helper's non-Linux `#[cfg]` stub per the CLAUDE.md cross-platform invariant.

### Non-blocking items at approval (LOW, noted in round 6)
- task 2.1 inline expression vs 8.0(b) extracted `downloads_from` helper — same code, two phrasings; clarified they are the extract-not-reimplement of one another.
- `downloads_from(None, None)` → `/tmp/Downloads` extra path — added the assertion.
- task 6.4 macOS permission ordering is an acknowledged Open Question (gate-startup vs auto-prompt), correctly scoped, not a defect.
