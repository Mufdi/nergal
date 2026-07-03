## Why

The app has one raw-HTML sink — `ConfirmHost.tsx:79` renders `opts.body` via `dangerouslySetInnerHTML` "by contract: callers escape user-controlled input." Two callers honor the contract with a **locally duplicated** `escapeHtml`:

- `src/stores/linear.ts:358` (definition) + `:484` (escapes issue titles into the body)
- `src/stores/clickup.ts:557` (definition) + `:544` (escapes task names)

But **four** call sites interpolate names **unescaped** into the same sink (via `confirm`/`swalConfirm` → `ConfirmHost`), across two files (iprev round 1 found the two `SettingsPanel` sites in addition to the two `Sidebar` ones):

- `Sidebar.tsx:236` — ``body: `<strong>${session.name}</strong> will be removed and its terminal closed.` ``
- `Sidebar.tsx:578` — ``body: `<strong>${ws.name}</strong> will be removed from the sidebar.${sessionsLine}` ``
- `SettingsPanel.tsx:1128` — ``body: `<strong>${name}</strong>'s API key will be deleted…` `` where `name` is the **Linear workspace/org name** — external-origin content, exactly the trust tier the proposal cites when escalating `ws.name`.
- `SettingsPanel.tsx:1903` — ``body: `<strong>${target?.label ?? "Untitled"}</strong> will be removed permanently…` `` (custom-theme label, user-named).

Today the Sidebar/theme cases are close to self-XSS (the value is something the user named), but the Linear-org name is external-origin, `ws.name` derives from a directory basename reachable via the deep-link workspace-creation path, and all four **break the escaping convention** the Linear/ClickUp paths already follow for the identical dialog. There are also two divergent copies of `escapeHtml` that can drift, and the `confirm.ts` contract comment points at those copies.

## What Changes

- **Extract one shared `escapeHtml` util** (e.g. `src/lib/escapeHtml.ts`) and re-point `linear.ts` and `clickup.ts` at it, deleting their local copies (dedupe).
- **Escape the interpolated names at all four sites** — wrap `session.name` (`Sidebar.tsx:236`), `ws.name` (`Sidebar.tsx:578`), the Linear org `name` (`SettingsPanel.tsx:1128`), and the theme `label` (`SettingsPanel.tsx:1903`) in the shared `escapeHtml`, keeping the surrounding `<strong>`/static markup intact (escape the value, not the whole body).
- **Fix the contract comment** — update `confirm.ts:9-11`, which currently points to the `escapeHtml` copies in `stores/clickup.ts`/`stores/linear.ts` being deleted, to point at the new shared util (otherwise it becomes a dead pointer). Add a short pointer at the `ConfirmHost` body prop too.
- **Make the invariant enforceable (cheap check)** — add a lightweight test/lint that scans for a `body:` template literal in a `confirm`/`swalConfirm` call interpolating a value without `escapeHtml`, so a fifth site cannot silently reappear. This is what makes the declared "capability" actually verifiable rather than aspirational.

This is the shared helper the sibling `confirm-deep-link-session-spawn` change depends on for escaping the attacker-controlled prompt/cwd text.

## Capabilities

### New Capabilities

- `html-escaping-contract` — the invariant that any caller-supplied value interpolated into the `ConfirmHost` raw-HTML body is HTML-escaped through the single shared `escapeHtml` util, with no per-module reimplementations.

## Impact

- **`src/lib/escapeHtml.ts`** (new): single `escapeHtml(s: string): string`.
- **`src/stores/linear.ts` / `src/stores/clickup.ts`**: delete local `escapeHtml`, import the shared one (no behavior change — same escaping).
- **`src/components/layout/Sidebar.tsx`**: escape `session.name` (`:236`) and `ws.name` (`:578`).
- **`src/components/settings/SettingsPanel.tsx`**: escape the Linear org `name` (`:1128`) and the theme `label` (`:1903`).
- **`src/lib/confirm.ts`**: update the contract comment (`:9-11`) to point at the shared util; add a pointer at the `ConfirmHost` body prop.
- **Tests/lint**: unit tests for `escapeHtml` (`<`, `>`, `&`, `"`, `'` escaped; plain text unchanged); a regression check that a name containing markup renders as literal text; a lightweight scan/lint asserting no `confirm` body interpolates a value without `escapeHtml`.
- **Out of scope**: the ConfirmHost raw-HTML architecture itself (kept — the fix is disciplined escaping at call sites, the same contract Linear/ClickUp already meet).
