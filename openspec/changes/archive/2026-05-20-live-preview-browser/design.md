# Design — Live Preview Browser (iframe)

## Architectural decisions

### D1. iframe in React panel — primary path

**Decision**: El browser se implementa con `<iframe>` dentro del componente React `BrowserPanel`. NO secondary WebView de Tauri (descartado post-Phase 0 spike).

**Razón del descarte de secondary WebView** (validación visual del spike):

| Issue | secondary WebView | iframe en panel React |
|-------|-------------------|---------------------|
| Z-order vs React modals/palette | 🔴 webview siempre encima | ✅ DOM normal, modals encima por z-index |
| Shortcuts globales (Cmd+K, alt+nav, ctrl+shift+0) | 🔴 robados por webview | ✅ React UI mantiene foco |
| Bbox sync con panel resize / mode-switch | 🔴 manual + buggy en WebKitGTK | ✅ flex/grid layout natural |
| Tauri `unstable` feature flag | 🔴 requiere | ✅ no requiere |
| ctrl+shift+0 expand panel | 🔴 webview no se entera | ✅ React layout adapta |
| X-Frame-Options en localhost dev servers (Vite, Next, Python, etc.) | ✅ ignora | ✅ rara vez bloqueado en dev mode |
| Cookies / session por target origin | 🟡 host-shared | ✅ per-origin natural |

Para el use case real (localhost dev preview integrado a panel del dock), iframe gana en todas las dimensiones excepto X-Frame-Options — y eso casi nunca se manifiesta con dev servers locales.

**Iframe sandbox attrs**: `sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals"`. Esto:
- Permite que el target ejecute JS (necesario para SPAs).
- Permite same-origin para el target (necesario para que cookies del target funcionen — sin esto el iframe siempre actúa como origin único).
- Permite forms y popups (típico de dev tools).
- Bloquea por default `allow-top-navigation` y `allow-pointer-lock` (defensa básica).

### D2. Iframe URL state — React-driven

**Decision**: La fuente de verdad para la URL actual del iframe es el atom React `browserUrlAtom`. El iframe `src` se ata directamente al atom value:

```tsx
<iframe
  src={url}
  sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals"
  className="w-full h-full border-0 bg-white"
  ref={iframeRef}
/>
```

Cuando el user escribe en URL bar y presiona Enter, se llama `browser_validate_url` (backend) primero para sanity check del scheme; si pasa, se actualiza `browserUrlAtom` y React re-renderiza el iframe con nueva src.

**Ventaja**: no hace falta sync bidireccional con un proceso externo. React es la única source of truth.

**Limitación**: navegaciones internas del iframe (link clicks dentro de la página) NO actualizan automáticamente nuestro `browserUrlAtom` por la same-origin policy. Mitigación:
- Para localhost (mismo origin que el iframe en práctica), podemos leer `iframe.contentWindow.location.href` periódicamente (rAF poll mientras visible) para detectar cambios y reflejarlos en URL bar. **Solo aplica si target es same-origin** — pero localhost dev servers casi siempre son same-origin entre páginas.
- Para cross-origin: URL bar muestra la última URL navegada explícitamente. Aceptable.

### D3. History — React in-memory

**Decision**: `browserHistoryAtom` mantiene `{ back: string[], forward: string[] }` por sesión. Toolbar back/forward botones manipulan el atom y disparan re-render del iframe con la URL elegida.

Nada de event sync con backend (el problema de R1 era con secondary WebView; iframe está auto-contenido).

### D4. Singleton + session lifecycle

**Decision**: Como `tasks` y `git`, `browser` se añade a `SINGLETON_TYPES`. Una instancia activa por sesión (per-session URL/history independiente).

**React lifecycle**:
- `BrowserPanel` se monta cuando user activa el tab.
- Cambio de tab → React unmount del componente; al volver a montar, lee atoms (URL/history) y re-renderiza iframe con la URL guardada.
- **El iframe se re-monta** al cambiar de tab; eso significa que la página se re-fetcha (no preserva scroll/state interno del SPA cuando user oculta y muestra el browser).
- Mitigación si la perdida de iframe state molesta: usar `display: none` en el panel container en vez de unmount, así el iframe permanece vivo. Decisión MVP: dejar unmount; si feedback indica problema, switchear a hide-via-CSS en una iteración rápida.

### D5. Floating mode — reuso de FloatingPanel

**Decision**: `BrowserFloating.tsx` envuelve `<FloatingPanel panelId="browser" ...>` y renderiza `<BrowserPanel>` adentro. Geometry persiste en SQLite via panelId existente — sin migration.

**Mode-switch dock↔floating**:
- Cambia `browserModeAtom`. `RightPanel.tsx` y `Workspace.tsx` deciden qué chrome usar.
- Si el iframe se desmonta entre modes y se vuelve a montar, la URL persiste (atom intacto), pero el iframe re-fetcha.
- Para preservar el estado del iframe entre modes: en lugar de mount/unmount, usar **un solo `<BrowserPanel>` montado siempre, con `display: dock|floating|none` controlado por `browserModeAtom`**. El iframe vive una sola vez y solo cambia el chrome que lo envuelve.

Implementación elegida: variante portable. `BrowserPanel` se monta una vez en `Workspace.tsx` y se renderiza siempre; `display:none` cuando `mode='floating'` y el dock no está activo, etc. La complejidad mínima vale el state preservation.

**UX del dock cuando mode=floating**: el slot del browser tab en el dock muestra placeholder card "Browser está en modo flotante" + botón "Volver al dock" (clickea → set mode=dock).

### D6. Keyboard — Cmd+L para focus URL bar y ctrl+shift+0 para expand

**Decisiones**:

- **Cmd+L** (Ctrl+L en Linux): focus URL bar. Scope: browser visible (dock active OR floating open). Implementado en `stores/shortcuts.ts` con `event.code === "KeyL" && (event.metaKey || event.ctrlKey)`. Verificar contra shortcuts existentes.
- **ctrl+shift+0**: ya existe en cluihud para expandir el panel actual. El BrowserPanel se integra al patrón existente sin cambios — al estar activo en el dock, el shortcut funciona transparentemente.

### D7. Backend — minimum surface

**Decision**: Sin Tauri capabilities nuevas. El iframe es DOM en el main webview de Tauri; no requiere permissions adicionales.

**Único comando backend**: `browser_validate_url(url: String) -> Result<Url, String>`:
- Parse con `url::Url`
- Reject si scheme NO es `http` / `https` / `about:blank`
- Specifically blocks: `file://`, `javascript:`, `data:`, `chrome://`, `about:` (excepto `about:blank`)
- Log con `tracing::info!` para audit trail

**Por qué backend valida y no frontend**: defense-in-depth. Si en algún flujo un agente comprometido logra hacer prompt injection que llega al frontend e intenta navegar a `javascript:alert(...)`, el atom-set sí se ejecutaría sin validación frontend. Pasar la URL por backend antes de actualizar el atom cierra el surface.

**Tracking de navegación** (opcional Phase 2): backend escucha eventos del iframe via postMessage si target coopera. Para MVP no es necesario.

### D8. Port scanner — TCP connect probe con hysteresis

**Decision**: Backend tokio task que cada 3 segundos hace `TcpStream::connect(127.0.0.1:port)` a un set predefinido de ports comunes.

**Default port set**:
```rust
const PORTS: &[u16] = &[
    3000, 3001, 3030,        // Next.js / common dev
    4200, 4321,              // Angular / Astro
    5000, 5001, 5050,        // Flask / generic
    5173, 5174,              // Vite
    8000, 8080, 8888,        // Python http.server / generic
    9000,                    // Django
];
```

**Hysteresis** (chip flicker mitigation):
- Un port aparece en `localhostPortsAtom` tras **1 scan** (3s) detectándolo activo.
- Un port se remueve solo tras **2 scans consecutivos** (6s) detectándolo inactivo.

Implementación: `BTreeMap<u16, u8>` donde la key es el port y el value es el counter de scans inactivos consecutivos; remove cuando counter llega a 2.

**Emit event**: `localhost:ports-changed` con `{ active: u16[], at: timestamp }`. Frontend Jotai atom `localhostPortsAtom` se actualiza en `setupHookListeners()`.

### D9. CSP-aware proxy — deferred (not MVP)

**Decision**: NO implementar el proxy en este change. La gran mayoría de localhost dev servers (Vite, Next, Astro, Python http.server, Django dev, Flask debug) NO setean `X-Frame-Options` ni `frame-ancestors` restrictivos. El iframe carga directamente.

**Si en uso real un dev server bloquea**: se agrega como follow-up `browser-csp-proxy` (Rust axum/hyper proxy que strippea X-Frame-Options y rewrite content). Estimate documentado para ese caso: ~4-5h.

## Risks + mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| X-Frame-Options block en dev server raro | Low (most don't set it in dev) | Medium | Follow-up `browser-csp-proxy` si emerge |
| URL scheme attack (file://, javascript:) | Low | High | Backend `validate_url` defense-in-depth |
| Iframe re-fetch al mode-switch | Medium | Low | Mount-once con display:none toggling |
| Chip flicker por port flap | Medium | Low | Hysteresis 1-scan-add / 2-scans-remove (D8) |
| iframe en floating mode tapado por React modal del workspace | Low | Low | Mismo z-order DOM, predecible |
| Sandbox attrs muy restrictivos rompen SPA | Low | Medium | `allow-scripts allow-same-origin allow-forms allow-popups allow-modals` cubre 99% casos |
| Cookies de iframe persisten cross-session | Low | Low | Aceptable para personal desktop app |

## What changed vs the original (pre-spike) design

- ❌ Tauri secondary WebView — descartado por z-order + focus capture issues en WebKitGTK.
- ❌ `unstable` Tauri feature flag — no necesario.
- ❌ `core:webview:*` capabilities — no necesario.
- ❌ Bbox sync (ResizeObserver + IntersectionObserver + scroll listener + rAF poll) — no necesario, layout natural.
- ❌ Backend webview lifecycle commands (create/destroy/set_bounds/set_visible) — no necesario.
- ❌ Hide-vs-destroy strategy — reemplazado por mount-once con display toggling.
- ❌ Backend canónico de history — innecesario, React es source of truth.
- ❌ `did-navigate` callback wiring — innecesario.
- ❌ Phase 0 spike — completado, results documented in `handoff/spike-outcome.md`.
- ✅ Mantenido: URL scheme validation, port scanner con hysteresis, FloatingPanel reuse, singleton + session lifecycle (simplificado), Cmd+L scope (browser visible).
- ✅ Nuevo: integración con ctrl+shift+0 expand del panel (heredada del pattern existente).

## Files touched (Phase 1+)

- **NEW**: `src/stores/browser.ts`, `src/components/browser/BrowserPanel.tsx`, `src/components/browser/BrowserToolbar.tsx`, `src/components/browser/BrowserFloating.tsx`, `src-tauri/src/browser.rs`.
- **MODIFIED**: `src/stores/rightPanel.ts`, `src/components/layout/RightPanel.tsx`, `src/components/layout/StatusBar.tsx`, `src/components/layout/Workspace.tsx` (mount-once `BrowserPanel`), `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `DESIGN.md`.
- **NO TOUCH**: `src-tauri/Cargo.toml` (excepto el `url = "2"` que ya está + revertido el `unstable` feature), `src-tauri/capabilities/default.json` (no nuevas permissions necesarias).

Total: 9 files (5 new, 4 modified, 0 capability/cargo additions).
