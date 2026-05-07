# Live Preview Browser — Dual-Mode Web Panel (iframe)

## Why

cluihud actualmente no permite verificar el avance visual de un dev server localhost sin context-switch a un browser externo. Cada vez que el agente edita frontend o el user quiere validar un cambio, hay que tab-switch fuera del workspace, perder foco, y volver. Para un wrapper que se posiciona como "completar el loop alrededor de Claude Code", este context-switch es el gap más visible en el flow frontend.

La feature consolida cinco herramientas analizadas en `Inspiración y Referencias - Claude Code GUIs (2026).md`:
- **cmux** + **Vibeyard** + **Limux** validan que browser embebido es viable en Linux desktop.
- **opensessions** introduce localhost port auto-detection como UX pattern accionable desde la sidebar.
- **Glass** + **Intent** sostienen la filosofía unified workspace (browser + editor + terminal en un binary).

> **Nota arquitectónica (post-Phase 0)**: la propuesta original consideró Tauri 2 secondary WebView. La validación visual del spike reveló dos issues bloqueantes para la integración panel-in-dock: (a) el secondary WebView captura el foco del teclado y rompe TODOS los shortcuts de cluihud (Cmd+K palette, alt+nav, ctrl+shift+0 expand, etc.); (b) z-order es webview-siempre-encima, lo que tapa modals/palette/dropdowns React. Pivot a **iframe en panel React** descartó ambos issues (z-order DOM normal, foco respeta React layout). Detalle en `handoff/spike-outcome.md`.

Foco cluihud: ambos modos — panel persistente del dock + floating popup — usando `<iframe>` dentro de `BrowserPanel`, reutilizando `FloatingPanel.tsx` (chrome ya battle-tested via scratchpad) para minimizar cost del modo flotante.

## What Changes

- Nuevo `TabType "browser"` en el right panel, categoría `tool`, singleton (una instancia activa por sesión).
- Componente `BrowserPanel` con toolbar (URL bar, back/forward, reload, mode-switch dock↔floating) y un `<iframe>` sandbox-attrs como área principal.
- Modo floating reutilizando `FloatingPanel` con `panelId="browser"` — geometría persistente vía SQLite sin migration.
- Backend Rust con comando `browser_validate_url` (URL scheme allowlist: `http`/`https`/`about:blank`) — el resto de la lifecycle del iframe vive en React.
- Port scanner backend (tokio TCP probe en rangos comunes) que emite `localhost:ports-changed` events.
- Status bar chip con ports localhost activos detectados; click → abre browser apuntando al port.
- Keyboard shortcut nuevo: `Cmd+L` (focus URL bar cuando browser está visible) — verificado contra `stores/shortcuts.ts` para evitar colisiones.
- Integración con shortcut existente `ctrl+shift+0` para expandir el panel (igual que diff/git/plan).

## Capabilities

### New Capabilities
- `live-preview-browser`: dual-mode embedded browser (dock panel + floating popup) implementado con `<iframe>` sandbox-attrs, port auto-detection vía backend tokio scanner, y mode-switch sin perder URL/history.

### Modified Capabilities
- `tab-system`: añade `"browser"` al `TabType` union, `SINGLETON_TYPES`, `PANEL_CATEGORY_MAP`.
- `panel-categories`: `browser` clasificado como `tool`.

## Impact

- **Frontend**: nuevo módulo `src/components/browser/` (BrowserPanel, BrowserToolbar, BrowserFloating). Modificación a `stores/rightPanel.ts`, `layout/RightPanel.tsx`, `layout/StatusBar.tsx`. Nuevo store `stores/browser.ts` con atoms keyed por sessionId (URL actual, history, ports detectados, mode).
- **Backend**: nuevo módulo `src-tauri/src/browser.rs` con `validate_url` helper + `PortScanner` tokio task. Registro en `commands.rs` y `lib.rs`. **Sin Tauri capabilities nuevas** — iframe vive en el webview React, no requiere permissions del runtime Tauri.
- **DESIGN.md**: documenta el pattern "browser embebido dual-mode con iframe + FloatingPanel reuse" como referencia para futuras features con web content hosting.
- **Persistence**: floating geometry vía SQLite reusing scratchpad pattern (`panelId="browser"` row). URL/history son in-memory por sesión, sobreviven mode-switch porque el iframe no se desmonta (mismo React component instance, keyed por sessionId).
- **No DB schema migration**: FloatingPanel ya está keyed por panelId; agregar uno más es no-op.
- **Tauri version**: NO requiere `unstable` feature flag (eso era para secondary WebView, ya descartado).

## Build contract

### Qué construyo

1. `src/stores/browser.ts` — atoms: `browserUrlAtom` (per session), `browserHistoryAtom`, `localhostPortsAtom`, `browserModeAtom` (dock|floating).
2. `src/components/browser/BrowserPanel.tsx` — host del iframe + toolbar + área del iframe sandbox-attrs.
3. `src/components/browser/BrowserToolbar.tsx` — URL bar, back/forward, reload, mode-switch.
4. `src/components/browser/BrowserFloating.tsx` — wrapper sobre `FloatingPanel` con `panelId="browser"`.
5. Modificación `src/stores/rightPanel.ts` — añadir `"browser"` al `TabType`, a `SINGLETON_TYPES`, y `PANEL_CATEGORY_MAP[browser] = "tool"`.
6. Modificación `src/components/layout/RightPanel.tsx` — render `BrowserPanel` cuando active tab es browser.
7. Modificación `src/components/layout/StatusBar.tsx` — chip con localhost ports activos.
8. `src-tauri/src/browser.rs` — `validate_url(url) -> Result<Url, String>` helper + `PortScanner` tokio task.
9. Registro de comandos en `src-tauri/src/commands.rs` + init en `src-tauri/src/lib.rs`.
10. Update `DESIGN.md` con el pattern dual-mode iframe.

### Cómo verifico

```bash
cd cluihud
# Backend gates
cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check
# Frontend gates
cd .. && npx tsc --noEmit
# Functional (manual, post-build, golden path)
pnpm tauri dev
# 1. Open browser tab via Cmd+K → "Open browser"
# 2. Type localhost:5173 in URL bar → page renders en el iframe
# 3. Cmd+K palette se abre por encima del iframe (z-order DOM normal)
# 4. ctrl+shift+0 expande el panel browser igual que diff/git/plan
# 5. Toggle floating mode → iframe sigue mostrando misma URL en FloatingPanel
# 6. Start `python3 -m http.server 8080` → status bar muestra :8080 chip
# 7. Click :8080 chip → browser navega a esa URL
# 8. Cmd+L → focus URL bar
# 9. Reload, back, forward funcionan
```

### Criterio de done

- [ ] Tab "Browser" abre via Cmd+K command palette (singleton: solo una activa por sesión).
- [ ] Toolbar funcional: URL bar (Enter para navegar), back, forward, reload, mode-switch.
- [ ] Iframe carga `http://localhost:5173` (Vite dev server) sin X-Frame-Options issues.
- [ ] Cmd+K palette se abre POR ENCIMA del iframe (validado visualmente).
- [ ] `ctrl+shift+0` expande el panel browser igual que otros paneles del dock.
- [ ] Shortcuts de cluihud (alt+nav, Cmd+K, etc.) siguen funcionando con browser visible.
- [ ] Floating mode reutiliza `FloatingPanel` con `panelId="browser"`, geometría persistente vía SQLite sin migration.
- [ ] Mode-switch dock↔floating preserva URL y history (mismo React component instance).
- [ ] Port scanner detecta dev servers comunes (Vite 5173, Next 3000, Python http.server 8080) y los muestra en StatusBar como chips.
- [ ] Click en port chip abre browser con esa URL.
- [ ] Cmd+L focus URL bar (sin colisiones — verificado contra `stores/shortcuts.ts`).
- [ ] `cargo clippy -- -D warnings`, `cargo test`, `cargo fmt --check`, `tsc --noEmit` todos pasan.
- [ ] DESIGN.md actualizado con el pattern.
- [ ] Spec `live-preview-browser` finalizado.

### Estimated scope

- files_estimate: 9
- risk_tier: low
- tags: [feature]
- visibility: private
- spec_target: live-preview-browser

## Out of scope (this iteration)

- **Reload-on-save sync**: file watcher dispara reload del iframe automáticamente al guardar archivos. → Follow-up `browser-reload-on-save`.
- **Multi-tab interno** dentro del browser: una sola URL activa por sesión. Para múltiples páginas usar floating + dock simultáneos.
- **Bookmarks / history persistente**: solo back/forward in-memory por sesión.
- **Cookies / sessions / login UI**: storage default del iframe (per-origin del target). Sin UI propia para gestionarlo.
- **CSP-aware proxy** para sitios con X-Frame-Options restrictivo: se evaluará si el use case real (localhost dev servers) lo necesita. La gran mayoría de dev servers (Vite, Next, Astro, Python http.server, Django, Flask) no setean headers restrictivos en dev mode. Si en práctica encontramos un dev server que bloquea, se agrega como Phase opcional o follow-up `browser-csp-proxy`.

## Out of scope — descartado completamente

- **Design mode** (Orca-style: click element → DOM snippet al chat). Cross-origin same-origin policy de iframe lo hace técnicamente imposible sin reescribir el target page o instalar un dev-tool. Decisión: NO se implementará. Si el caso aparece a futuro, requiere arquitectura distinta (ej. agente instala devtools-extension en el browser embebido, fuera de scope cluihud).
