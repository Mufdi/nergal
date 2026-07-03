## 1. Shared util

- [ ] 1.1 Add `src/lib/escapeHtml.ts` exporting `escapeHtml(s: string): string` that escapes `&`, `<`, `>`, `"`, `'`. Doc comment: the required escaper for any value interpolated into a `ConfirmHost` body.

## 2. Dedupe existing copies

- [ ] 2.1 `src/stores/linear.ts` — delete the local `escapeHtml` (`:358`), import the shared one; call sites (`:484`) unchanged in behavior.
- [ ] 2.2 `src/stores/clickup.ts` — delete the local `escapeHtml` (`:557`), import the shared one; call site (`:544`) unchanged.

## 3. Fix all four unescaped sites

- [ ] 3.1 `src/components/layout/Sidebar.tsx:236` — wrap `session.name` in `escapeHtml(...)`, keep the `<strong>` and static text.
- [ ] 3.2 `src/components/layout/Sidebar.tsx:578` — wrap `ws.name` in `escapeHtml(...)`, keep the `<strong>`, static text, and `sessionsLine`.
- [ ] 3.3 `src/components/settings/SettingsPanel.tsx:1128` — wrap the Linear org `name` in `escapeHtml(...)`.
- [ ] 3.4 `src/components/settings/SettingsPanel.tsx:1903` — wrap `target?.label ?? "Untitled"` in `escapeHtml(...)`.
- [ ] 3.5 Update the contract comment `src/lib/confirm.ts:9-11` to point at `src/lib/escapeHtml.ts` (not the deleted store copies); add a one-line pointer at the `ConfirmHost` body prop.

## 4. Enforceable invariant

- [ ] 4.1 Add a lightweight scan/lint (test or CI grep) that flags a `confirm`/`swalConfirm` `body:` template literal interpolating a value without `escapeHtml`, so a fifth site cannot silently reappear.

## 5. Tests

- [ ] 5.1 Unit test `escapeHtml` (all metacharacters escaped; plain text unchanged).
- [ ] 5.2 Regression: a name/label containing markup renders as literal text in the confirm dialog (cover a Sidebar and a SettingsPanel site).

## 6. Verification

- [ ] 6.1 `npx tsc --noEmit`.
- [ ] 6.2 Manual: remove a workspace/session and delete a custom theme whose name contains `<` or `&` → the dialog shows the raw characters, no broken markup, no script.
