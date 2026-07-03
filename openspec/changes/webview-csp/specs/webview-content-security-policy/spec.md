# webview-content-security-policy

Provides a containment layer against script/HTML injection in the main webview by enforcing a Content-Security-Policy that defaults to `'self'` and allowlists only enumerated, justified origins.

## ADDED Requirements

### Requirement: Main webview enforces a Content-Security-Policy

The main webview SHALL enforce a Content-Security-Policy configured in `tauri.conf.json` (`app.security.csp`). The policy SHALL default to `default-src 'self'` (with `script-src`/`connect-src` also `'self'`) and SHALL allowlist only origins the application genuinely requires, each justified in the design/proposal (not as JSON comments). Because the in-app browser renders cross-origin `<iframe>`s inside the main webview, the policy SHALL include `frame-src http: https:` as a deliberate, documented allowance so browser tabs work; this SHALL NOT widen `script-src`/`connect-src`/`default-src`. The policy SHALL NOT regress the markdown renderers, the confirm dialog, the in-app browser tabs, external-link opening, or the updater, verified against a built bundle (or a mirrored `devCsp`), not plain dev.

#### Scenario: default-deny baseline

- **WHEN** the app loads the main webview
- **THEN** a CSP is present with `default-src 'self'`, and content from non-allowlisted origins is blocked

#### Scenario: legitimate surfaces still work

- **WHEN** the user views markdown-rendered tracker/PR content, opens the confirm dialog, opens an external link, and runs the updater
- **THEN** each functions normally under the policy (the required origins are allowlisted and justified)

#### Scenario: in-app browser iframe still loads

- **GIVEN** the in-app browser renders a cross-origin `<iframe>` per tab inside the main webview
- **WHEN** the user opens a browser tab to an arbitrary `https:` site under the policy
- **THEN** the iframe loads (because `frame-src http: https:` is allowed), while `script-src`/`connect-src` remain `'self'` so the framed origin cannot script the app

#### Scenario: policy is verified against the enforced build

- **WHEN** the policy is validated
- **THEN** it is checked against a built bundle (or a mirrored `devCsp`), not plain `pnpm tauri dev` (where `app.security.csp` is not applied)

#### Scenario: injected script is contained

- **GIVEN** a hypothetical HTML/script payload that reaches the webview
- **THEN** the CSP blocks its execution or external exfiltration path (no arbitrary `script-src`/`connect-src` origin is permitted)
