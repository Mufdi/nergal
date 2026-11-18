# REVIEW — setup-wizard (FRI-498)

## Frontend reviewer (sonnet, code-level; app not rendered)
- 🟡 Enter chip shown while Configure disabled → **fixed** (chip only when configurable).
- 🟡 Inline error without `role="alert"` → **fixed**.
- 🟡 Capture-phase Enter fires Configure with focus on Skip → **fixed** (Revision 1, D4).
- 🟢 "will be added" copy when not configurable → **fixed** ("would").
- 🟢 Toast on probe failure → **declined**: the spec hides all UI on a broken settings.json on purpose.
- 🟢 `canConfigure` alias → **kept**: the locked tests use it.
- 🟢 ref forwarding → verified: React 19 passes `ref` as a prop through `DialogContent` to the Popup.
- Verdict: no 🔴.

## Adversarial reviewer (opus)
### Bugs
- Important: Enter on focused Skip writes settings.json → **fixed** (Revision 1, D4).
- Important: AppImage path had no way out (`APPIMAGE` guard) → **fixed** (`binary_usable` ignores only binaries under `$APPDIR`; Windows always true).
- Important: D7 inspected every hooks key, breaking `nergal hook setup` on unrelated malformed events → **fixed** (scoped to `HOOKS` + `OBSOLETE_HOOKS`).
- Nit: Windows always disabled → **fixed** by the cfg(windows) branch.
- Nit: `disableAllHooks: true` not detected → **declined**: a user who disabled hooks should not be nagged.
### Compliance
- Important: dialog scenarios without automated tests → **accepted as manual walk** (no DOM test env; tasks 6.2–6.6). Added the `run_at` bytes-unchanged test for malformed events.
- Important: docs drift (`nergal setup` does not exist; obsolete snippet; codex claim) → **fixed** in docs/hooks.md and README.md.
- Nit: launch dialog stacking → listed in design Risks; covered by the manual walk.
### Security
- No findings beyond the consent bug above (fixed).

## Gate verdict
No spec FAIL, no security FAIL, no frontend 🔴. All Important findings fixed or explicitly accepted.
