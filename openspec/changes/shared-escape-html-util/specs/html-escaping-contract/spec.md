# html-escaping-contract

Ensures any caller-supplied value interpolated into the app's raw-HTML confirm-dialog sink is HTML-escaped through a single shared utility, with no divergent per-module copies.

## ADDED Requirements

### Requirement: Single shared HTML escaper for the confirm-dialog sink

Any caller-supplied value interpolated into the `ConfirmHost` body (rendered via `dangerouslySetInnerHTML`) SHALL be escaped through a single shared `escapeHtml` utility. Modules SHALL NOT define their own `escapeHtml`, and NO `confirm`/`swalConfirm` call site SHALL interpolate a caller-supplied value into the body unescaped — verified by a lightweight scan/lint so a new site cannot silently violate it. The known sites are `Sidebar.tsx:236,578` and `SettingsPanel.tsx:1128,1903`; the contract comment in `confirm.ts` points to the shared util.

#### Scenario: workspace/session names are escaped in the sidebar confirm

- **GIVEN** a workspace named `<img src=x onerror=alert(1)>`
- **WHEN** the sidebar shows the remove-workspace confirmation
- **THEN** the name renders as literal text (escaped) and no markup executes

#### Scenario: settings-panel confirms escape their interpolated names

- **GIVEN** a Linear workspace/org name (external origin) or a custom-theme label containing markup
- **WHEN** the "Remove workspace?" (`SettingsPanel.tsx:1128`) or "Delete custom theme?" (`:1903`) confirmation shows
- **THEN** the value renders as literal text, escaped through the shared util

#### Scenario: an unescaped confirm body is caught

- **WHEN** the scan/lint runs over the frontend
- **THEN** a `confirm`/`swalConfirm` `body:` template literal interpolating a value without `escapeHtml` is reported

#### Scenario: tracker titles use the shared escaper

- **WHEN** the Linear/ClickUp binding-replacement confirmations interpolate an issue/task title
- **THEN** they escape it through the shared utility (not a local copy), and the rendered result is identical to the prior local-escape behavior

#### Scenario: the escaper handles all HTML metacharacters

- **WHEN** `escapeHtml` receives a string containing `<`, `>`, `&`, `"`, and `'`
- **THEN** each is replaced with its entity, and a string with none is returned unchanged
