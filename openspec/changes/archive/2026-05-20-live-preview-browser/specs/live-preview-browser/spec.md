## ADDED Requirements

### Requirement: Browser tab as singleton tool panel
The right panel SHALL support a `"browser"` tab type categorized as a `tool` panel. Only one browser instance SHALL be active per session.

#### Scenario: Open browser via command palette
- **WHEN** user invokes Cmd+K and selects "Open browser"
- **THEN** a singleton `"browser"` tab SHALL open in the right panel for the active session
- **AND** invoking the same action again SHALL focus the existing tab, not create a duplicate

#### Scenario: Browser is session-scoped
- **WHEN** user has a browser tab open in session A and switches to session B
- **THEN** the browser URL and history of session A SHALL persist in atoms
- **AND** session B SHALL show its own browser state (or none if not opened)

#### Scenario: Browser closes when session ends
- **WHEN** user closes a session
- **THEN** the browser atoms (URL, history, mode) for that session SHALL be cleared

### Requirement: Dual-mode rendering (dock + floating)
The browser SHALL support two coexisting modes: persistent panel in the right dock, and floating popup. Mode-switch SHALL preserve URL, history, and iframe runtime state.

#### Scenario: Switch from dock to floating
- **WHEN** browser is rendered in dock mode and user clicks the mode-switch button
- **THEN** the chrome SHALL change to FloatingPanel with `panelId="browser"`
- **AND** the iframe runtime state (scroll position, form data, SPA in-memory state) SHALL persist via mount-once with visibility toggling
- **AND** the current URL and history SHALL persist

#### Scenario: Floating geometry persists
- **WHEN** user resizes or drags the floating browser
- **THEN** the geometry SHALL persist via SQLite keyed by `panelId="browser"` (no schema migration)
- **AND** reopening the floating browser in a new session SHALL restore the last geometry

#### Scenario: Dock placeholder when in floating mode
- **WHEN** browser mode is `"floating"` and user has the browser tab active in the dock
- **THEN** the dock slot SHALL render a placeholder card "Browser is in floating mode"
- **AND** SHALL show a "Return to dock" button that switches mode back

### Requirement: Iframe-based content rendering
The browser content SHALL be rendered using a `<iframe>` element with restrictive sandbox attributes. The browser SHALL NOT use Tauri secondary WebView APIs (rejected post-Phase 0 spike due to focus capture and z-order issues with WebKitGTK).

#### Scenario: Iframe sandbox attributes are restrictive but functional
- **WHEN** the BrowserPanel renders the iframe
- **THEN** the iframe SHALL have `sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals"`
- **AND** SHALL NOT have `allow-top-navigation` or `allow-pointer-lock`

#### Scenario: React modals render above iframe
- **WHEN** the browser tab is active and the iframe is visible
- **AND** user opens the command palette (Cmd+K) or any other React modal
- **THEN** the modal SHALL render above the iframe (DOM-natural z-order)

#### Scenario: cluihud shortcuts continue to function
- **WHEN** the browser tab is active and the iframe is visible
- **AND** user presses Cmd+K, alt+left/right, or ctrl+shift+0
- **THEN** the shortcuts SHALL behave as in any other tab (the iframe SHALL NOT capture global focus)

### Requirement: URL bar with navigation controls
The browser toolbar SHALL provide URL bar, back, forward, reload, and mode-switch controls. URL navigation SHALL be validated against an allowed-scheme list via the backend before updating the iframe src.

#### Scenario: Navigate via URL bar
- **WHEN** user types a URL with `http://` or `https://` scheme in the URL bar and presses Enter
- **THEN** the frontend SHALL invoke `browser_validate_url` to verify the scheme
- **AND** on success, SHALL update `browserUrlAtom[sessionId]` with the canonical URL
- **AND** SHALL push the previous URL to history.back

#### Scenario: Reject disallowed schemes
- **WHEN** user types a URL with scheme `file://`, `javascript:`, `data:`, `chrome://`, or `about:` (other than `about:blank`)
- **THEN** the backend `browser_validate_url` SHALL return an error
- **AND** the iframe src SHALL NOT change
- **AND** the URL bar SHALL show a non-blocking error indicator

#### Scenario: Back/forward navigate history
- **WHEN** user clicks back with `history.back` non-empty
- **THEN** the iframe SHALL navigate to the previous URL by updating `browserUrlAtom`
- **AND** the current URL SHALL be pushed to `history.forward`

#### Scenario: Reload current page
- **WHEN** user clicks the reload button
- **THEN** the iframe SHALL re-fetch the current URL by bumping a `reloadKey` prop on the iframe React element

### Requirement: Localhost port auto-detection
The backend SHALL run a port scanner that probes a predefined list of localhost ports every 3 seconds and emits state-change events to the frontend. Port presence SHALL apply hysteresis to avoid chip flicker.

#### Scenario: New port detected (single-scan add)
- **WHEN** a new localhost port becomes reachable via TCP connect on one scan
- **THEN** the backend SHALL emit `localhost:ports-changed` with the updated active list
- **AND** the StatusBar SHALL render a chip for that port

#### Scenario: Port flap absorbed (hysteresis)
- **WHEN** a previously active port stops responding for ONE scan only and resumes on the next
- **THEN** the port SHALL remain visible without flicker
- **AND** no `localhost:ports-changed` event SHALL be emitted for transient unavailability

#### Scenario: Port becomes truly unreachable (two-scan remove)
- **WHEN** a previously active port stops responding for TWO consecutive scans (~6 seconds total)
- **THEN** the backend SHALL emit `localhost:ports-changed` with the port removed
- **AND** the StatusBar chip SHALL disappear

#### Scenario: Click port chip opens browser
- **WHEN** user clicks a localhost port chip in the StatusBar
- **THEN** the browser tab SHALL open (or focus if already open) in dock mode
- **AND** SHALL navigate to `http://localhost:<port>`

### Requirement: Cmd+L focuses URL bar
The browser SHALL expose a keyboard shortcut to focus the URL bar when the browser is **visible** in either dock mode (active tab) OR floating mode (window open).

#### Scenario: Cmd+L focuses URL bar when browser tab is active in dock
- **WHEN** the browser tab is the active tab in the right panel and user presses Cmd+L (Ctrl+L on Linux)
- **THEN** the dock browser's URL bar input SHALL receive focus
- **AND** the current URL text SHALL be selected for easy replacement

#### Scenario: Cmd+L focuses URL bar when floating browser is open
- **WHEN** the floating browser window is open (regardless of which dock tab is active) and user presses Cmd+L
- **THEN** the floating browser's URL bar input SHALL receive focus
- **AND** the current URL text SHALL be selected

#### Scenario: Cmd+L is no-op when no browser is visible
- **WHEN** no browser is visible (no browser tab active in dock AND floating browser closed)
- **THEN** Cmd+L SHALL NOT trigger any browser-related action
- **AND** other panels' shortcuts SHALL behave as before

### Requirement: Panel expansion via existing ctrl+shift+0
The browser panel SHALL integrate transparently with cluihud's existing `ctrl+shift+0` shortcut for expanding right panels. No new shortcut wiring SHALL be required.

#### Scenario: ctrl+shift+0 expands the browser panel like other right panels
- **WHEN** the browser tab is the active tab in the right panel and user presses ctrl+shift+0
- **THEN** the right panel SHALL expand following the same behavior as for diff, git, or plan panels
- **AND** the iframe content SHALL adapt to the new panel size via natural CSS layout (no manual bbox sync needed)

### Requirement: No new Tauri capabilities
The change SHALL NOT add new permissions to `src-tauri/capabilities/default.json`. The iframe is rendered inside the main webview's React UI and does not require additional Tauri runtime permissions.

#### Scenario: capabilities file unchanged
- **WHEN** the change is implemented and reviewed
- **THEN** `src-tauri/capabilities/default.json` SHALL be byte-identical to its pre-change state
