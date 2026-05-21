# Tasks — Live Preview Browser (iframe)

## Phase 0 — Spike (DONE — pivot to iframe)

Phase 0 completed during /work execution. Findings documented in `handoff/spike-outcome.md`. Decision: pivot from secondary WebView to iframe-in-React-panel based on visual validation showing focus capture and z-order issues with secondary WebView in WebKitGTK.

Spike artifacts (spike_browser.rs, spike control panel in App.tsx, Cargo.toml `unstable` feature flag) reverted before Phase 1.

## Phase 1 — Backend foundations (~2h)

- [ ] **1.1** Crear `src-tauri/src/browser.rs`:
  - `validate_url(url: &str) -> Result<url::Url, String>`: parse + reject schemes != http/https/about:blank
  - Tests inline: accepts http/https/about:blank, rejects file/javascript/data/chrome/about:other
  - `PortScanner` struct con tokio task que loop cada 3s
  - `PortScanner::run(app_handle: AppHandle)`: parallel TCP probe a `PORTS` hardcoded; hysteresis con `BTreeMap<u16, u8>` (counter scans inactivos consecutivos); emit `localhost:ports-changed` cuando set cambia
- [ ] **1.2** Implementar comando Tauri en `browser.rs`:
  - `#[tauri::command] async fn browser_validate_url(url: String) -> Result<String, String>` — wrapper sobre `validate_url`, retorna URL canónica como string si válida.
- [ ] **1.3** Registrar comando en `src-tauri/src/commands.rs` (re-export) y wiring en `src-tauri/src/lib.rs`:
  - `mod browser;`
  - Añadir `browser::browser_validate_url` al invoke_handler
  - En `setup` block: `tokio::spawn(browser::PortScanner::run(app_handle.clone()))`
- [ ] **1.4** Tests:
  - `tests/browser_url_validation.rs`: `validate_url` accepts http/https/about:blank, rejects file/javascript/data/chrome.
  - `tests/port_scanner_hysteresis.rs`: port aparece tras 1 scan, requiere 2 scans inactivos para remover. Mock TCP probe via injected closure.
- [ ] **1.5** Verify: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`.

## Phase 2 — Frontend store + dock panel (~3h)

- [ ] **2.1** Crear `src/stores/browser.ts`:
  - `browserUrlAtom` — `Record<sessionId, string>` (default empty → "about:blank" cuando se monta)
  - `browserHistoryAtom` — `Record<sessionId, { back: string[], forward: string[] }>`
  - `localhostPortsAtom` — `number[]` (no per-session; ports son globales)
  - `browserModeAtom` — `Record<sessionId, "dock" | "floating">` (default "dock")
  - Helpers: `pushHistory`, `goBack`, `goForward` que manipulan los atoms.
- [ ] **2.2** Modificar `src/stores/rightPanel.ts`:
  - Añadir `"browser"` al `TabType` union
  - Push a `SINGLETON_TYPES` array
  - `PANEL_CATEGORY_MAP[browser] = "tool"`
- [ ] **2.3** Crear `src/components/browser/BrowserToolbar.tsx`:
  - URL bar (input controlled, Enter → invoke `browser_validate_url` → si OK, set `browserUrlAtom` + push history)
  - Back / Forward botones (disabled cuando history vacío)
  - Reload botón (forza re-render del iframe via `key` bump)
  - Mode-switch botón (dock ↔ floating, set `browserModeAtom`)
- [ ] **2.4** Crear `src/components/browser/BrowserPanel.tsx`:
  - Render BrowserToolbar
  - Render `<iframe key={reloadKey} src={url} sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals" className="w-full h-full border-0 bg-white" />`
  - URL state desde `browserUrlAtom[sessionId]`
  - Reload key (number) que se incrementa al click reload — fuerza React a re-render el iframe (refetch).
- [ ] **2.5** Modificar `src/components/layout/RightPanel.tsx` para renderizar BrowserPanel cuando active tab es browser AND `browserModeAtom[sessionId] === "dock"`. Cuando mode === "floating", renderizar placeholder card "Browser está en modo flotante" + botón "Volver al dock".
- [ ] **2.6** Wire `setupHookListeners` para `localhost:ports-changed` → actualiza `localhostPortsAtom`.
- [ ] **2.7** Verify: `npx tsc --noEmit`.

## Phase 3 — Floating mode (~1h)

- [ ] **3.1** Crear `src/components/browser/BrowserFloating.tsx`:
  - Wrapper sobre `FloatingPanel` con `panelId="browser"`
  - Render `<BrowserPanel>` adentro (mismo componente que dock; iframe state persiste porque BrowserPanel se monta una vez en Workspace.tsx con visibility toggling — ver 3.2)
  - Geometry leída/escrita vía el patrón existente del scratchpad (FloatingPanel maneja su SQLite row con panelId)
- [ ] **3.2** **Mount-once integration** en `src/components/layout/Workspace.tsx`:
  - Renderizar `<BrowserPanel>` siempre (una sola instancia) en un container hidden por default.
  - Cuando `browserModeAtom[currentSession] === "dock"` AND active tab === browser → mostrar via portal/teleport en el RightPanel slot.
  - Cuando mode === "floating" → mostrar dentro del FloatingPanel chrome.
  - Cuando ninguno → `display: none`.
  - Esto preserva el iframe state (no re-fetch) entre mode-switches y tab-switches.
- [ ] **3.3** Verify: manual test del mode-switch sin perder URL ni iframe state (scroll position, formulario half-filled, etc.).

## Phase 4 — Port detection UI (~1h)

- [ ] **4.1** Modificar `src/components/layout/StatusBar.tsx`:
  - Render chip por cada port en `localhostPortsAtom`
  - onClick: si tab "browser" no está abierto, abrirlo; set `browserUrlAtom[sessionId] = "http://localhost:" + port`; activar tab
  - Si chip click cuando ya está en localhost del mismo port → no-op (idempotente)
- [ ] **4.2** Estilo de chips alineado con `DESIGN.md` tokens (compact, monospaced, hover state).
- [ ] **4.3** Si lista vacía → no render chip placeholder (sin ruido visual).

## Phase 5 — Polish + docs (~1h)

- [ ] **5.0** Shortcut "Open Browser" usa `Ctrl+Alt+B` (NO `Ctrl+Shift+W` — la W puede colisionar con muscle-memory de close-tab `Ctrl+W` y otros bindings de Linux WM).
- [ ] **5.1** Añadir shortcut `Cmd+L` en `stores/shortcuts.ts`:
  - id: `browser-focus-url-bar`
  - matcher: `event.code === "KeyL" && (event.metaKey || event.ctrlKey)`
  - scope: **browser visible** (tab activo en dock OR floating mode abierto). El shortcut funciona cuando el user puede ver el browser, independientemente de qué tab esté activo en el dock.
  - Verificar contra shortcuts existentes — NO colisión.
- [ ] **5.2** Hookup del shortcut en `BrowserToolbar` (focus input ref + select-all del texto actual).
- [ ] **5.3** Verificar que `ctrl+shift+0` (expand panel existente) funciona transparentemente con BrowserPanel — debería funcionar sin cambios porque BrowserPanel se monta dentro del slot del right panel.
- [ ] **5.4** Update `DESIGN.md` con sección "Embedded browser pattern" (iframe + sandbox + mount-once con visibility toggling).
- [ ] **5.5** Spec `live-preview-browser` en `openspec/changes/live-preview-browser/specs/live-preview-browser/spec.md` actualizado a iframe-first (ya hecho post-pivot, verificar).

## Phase 6 — Verify + handoff (~30min)

- [ ] **6.1** Full check:
  ```bash
  cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check
  cd .. && npx tsc --noEmit
  ```
- [ ] **6.2** Manual functional pass (los 9 pasos del Build contract → "Cómo verifico").
- [ ] **6.3** Single-reviewer post-build: Claude evalúa diff y elige reviewer apropiado. Probable code-quality (no toca capabilities, sin security tag fuerte ya).
- [ ] **6.4** Update `handoff/REVIEW.md` con outcomes.
- [ ] **6.5** Suggest `/commit` al user (no auto-invoke).
- [ ] **6.6** Suggest `/openspec-sync` al user para archive + update main specs.

## Estimated total

- Phase 1: 2 h (backend simplificado)
- Phase 2: 3 h
- Phase 3: 1 h
- Phase 4: 1 h
- Phase 5: 1 h
- Phase 6: 30 min
- **Total: ~8.5 h** (tier L, reducido de ~10h gracias a la simplificación iframe).

## Critical path

Phase 1 (backend tests) y Phase 2 (frontend foundations) son secuenciales. Phase 3 y 4 son paralelizables post-Phase 2. Phase 5 y 6 son secuenciales al final.
