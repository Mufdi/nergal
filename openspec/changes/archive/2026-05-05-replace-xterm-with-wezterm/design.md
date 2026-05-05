## Context

El terminal pane es el corazón de cluihud: es donde corre `claude` y donde el usuario vive el 80% del tiempo. Hoy usa xterm.js, lo que funciona pero nos clava el parser VT en JavaScript y nos impide construir sobre el terminal (annotations, hyperlinks clickeables, búsqueda semántica en scrollback). Movemos la emulación VT al backend Rust con `wezterm-term` — un parser VT maduro, cross-platform, zero-deps fuera de ecosistema wezterm, con soporte para OSC 8 hyperlinks, sixel, Kitty keyboard protocol, scrollback, e input encoding.

Esto es **Variante A** del análisis previo: seguimos en webview (Tauri + React) y el canvas es un componente React que dibuja celdas. No movemos a GTK overlay (Variante C) ni a ventana separada (Variante B). El goal es mantener el stack actual y solo cambiar el dueño del parser y el renderer.

## Goals / Non-Goals

**Goals:**
- Parser VT vive en Rust backend, uno por sesión
- Frontend renderiza un grid de celdas en `<canvas>`, no parsea bytes
- Input del usuario pasa por wezterm's encoder (single source of truth)
- Kitty keyboard protocol habilitado por default
- Ctrl+Backspace, Shift+Enter, Alt+letter funcionan con encoding distintivo
- OSC 8 hyperlinks renderizados como clickeables
- Paridad visual y funcional con la experiencia actual: theme, colores, cursor, selection, copy/paste, scrollback
- Hooks y flujo PTY sin cambios (contrato externo preservado)

**Non-Goals:**
- Ligaduras de fuente (xterm.js tampoco las renderiza bien; post-MVP)
- Sixel / iTerm2 image protocol (wezterm-term los parsea pero renderizar imágenes queda fuera del MVP)
- Búsqueda Ctrl+F incremental (post-MVP, fácil de agregar cuando el scrollback vive en Rust)
- Accesibilidad screen-reader completa (xterm.js tiene helper-textarea; en canvas necesita approach distinto, post-MVP)
- Variantes B/C (GTK window separada o overlay nativo): explicitamente descartadas
- Performance extremo (GPU-accelerated rendering via WebGPU): Canvas2D es suficiente para MVP

## Decisions

### Decision 1: Parser en backend, renderer en frontend

**Decisión**: `wezterm-term::Terminal` vive en `src-tauri/src/terminal/` — una instancia por sesión. El reader thread del PTY (ya existente en `pty.rs`) alimenta `Terminal::advance_bytes()` en vez de emitir bytes raw al frontend.

**Por qué**:
- Parser VT único para todo el stack (terminal, hooks, diff panel, transcript watcher pueden compartir modelo)
- Menos bytes cruzando el puente Tauri (un grid diff es más compacto que el raw stream de escape sequences)
- Permite features futuros (búsqueda en scrollback, export, etc.) sin duplicar parsing

**Alternatives considered**:
- *Parser en frontend* (ejecutar wezterm-term como WASM): descartado — añade toolchain, perdemos integración con hooks y transcript
- *Parser en Rust pero enviar bytes raw al frontend y a otros consumers*: descartado — perdemos la ventaja de modelo único

### Decision 2: IPC de grid-updates, no raw bytes

**Decisión**: nuevo evento Tauri `terminal:grid-update` con payload:
```rust
struct GridUpdate {
    session_id: String,
    /// Changed rows since last update: (row_index, row_cells)
    rows: Vec<(usize, Vec<Cell>)>,
    cursor: CursorPos,
    title: Option<String>,
    scroll_offset: usize,
}

struct Cell {
    ch: char,
    fg: Color,
    bg: Color,
    attrs: u8, // bold, italic, underline, etc. as bitflags
    hyperlink: Option<String>,
}
```

El backend mantiene la última versión enviada por sesión y emite solo **rows modificadas** (hash de cada row), no el grid completo.

**Por qué**:
- Volumen IPC acotado: un `cat archivo_grande` produce decenas de miles de escape sequences pero típicamente 24 rows de diff por frame
- Frontend no necesita parsear: recibe celdas ya resueltas

**Alternatives considered**:
- *Enviar grid completo cada tick*: descartado — demasiado tráfico, peor cuanto más grande el terminal
- *Enviar bytes raw y parsear en ambos lados*: descartado — duplica trabajo, pierde el punto del refactor
- *Shared memory / zero-copy*: descartado — complejidad no justificada para MVP

### Decision 3: Coalescing y throttle de updates

**Decisión**: los updates se acumulan en una ventana de `~8ms` (≈120fps) en el backend antes de emitir. Si el stream es rápido (ej. `yes`), se tira la ventana anterior y se envía solo el último estado.

**Por qué**: evita saturar el IPC bridge en outputs masivos. 120fps es overkill; 60fps también funcionaría bien.

**Implementación**: un `tokio::sync::Notify` por sesión. El reader thread llama `notify_one()` al terminar de procesar un chunk; un task dedicado espera la notificación, duerme 8ms acumulando más cambios, computa el diff contra el último snapshot emitido y emite.

### Decision 4: Input encoding en backend via wezterm

**Decisión**: frontend captura `keydown` en el canvas, construye un payload minimal:
```ts
type TerminalKeyEvent = {
  code: string;        // KeyboardEvent.code (ej. "KeyA", "Backspace", "Enter")
  key: string;         // KeyboardEvent.key (ej. "a", "A", "Backspace")
  text?: string;       // el text insertable, si es printable
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
};
```
Lo envía via nuevo command `terminal_input` al backend, que lo traduce a `wezterm_term::KeyCode` + `Modifiers` y llama `Terminal::key_down()` — que a su vez encodea bytes y los escribe al PTY via el writer ya existente.

**Por qué**:
- Single source of truth: wezterm-term tiene el encoder battle-tested
- Kitty keyboard protocol se habilita en un solo lugar
- Elimina la necesidad de mantener lógica de encoding en frontend (hoy xterm.js la hace por nosotros)

**Alternatives considered**:
- *Encoder en frontend* (portar wezterm-term's input logic a TS): descartado — duplicación innecesaria, difícil de mantener sincronizado
- *Usar `web-terminal-key-encoder` de terceros*: descartado — estado de mantenimiento incierto, queremos el match exacto con wezterm

**Latencia**: un roundtrip IPC local en Tauri es sub-ms. Para keystrokes humanos (max ~10/s) esto es imperceptible. El test crítico es autorepeat (holding backspace): el backend procesa los key events secuencialmente, el PTY eco-echo regresa igual que hoy.

### Decision 5: Frontend renderer fuera de React lifecycle (patrón actual)

**Decisión**: mantener el patrón de `terminalService.ts` — el renderer es una clase TS vivida fuera de React, con un host `<div ref>` que React expone. React solo controla qué sesión es visible.

**Por qué**:
- El patrón actual ya funciona, minimiza churn
- React re-renders no tocan el canvas
- Alineado con cómo xterm.js estaba integrado

**Implementación**:
- `TerminalRenderer` class maneja canvas instances, font atlas, y cell drawing
- Escucha `terminal:grid-update` para la sesión activa
- Maneja selection/copy-paste via mouse listeners propios
- `TerminalCanvas.tsx` es el wrapper React mínimo (host div + lifecycle)

### Decision 6: Font atlas pre-rendered en OffscreenCanvas

**Decisión**: al inicializar, el renderer pre-genera un atlas de glyphs (ASCII printable + Latin-1 + caracteres box-drawing comunes) en un OffscreenCanvas, indexado por `codepoint + attrs`. El render loop copia rectángulos del atlas al canvas visible (fast blit, no shape).

**Por qué**:
- Canvas2D `fillText` en cada celda cada frame es lento (~5-10ms para un terminal 120x40). Con atlas baja a sub-ms.
- Atlas se regenera solo si cambia fuente/tamaño/theme

**Limitaciones**:
- No ligaduras (cada codepoint es un glyph independiente)
- Para codepoints raros fuera del atlas: fallback a `fillText` (slow path)
- Emojis: TODO, probablemente `fillText` directo (no van al atlas)

### Decision 7: Kitty keyboard protocol on-by-default

**Decisión**: habilitamos el protocol en el wezterm-term setup. Esto hace que sequences como `CSI 27 ; 5 ; 8 u` se envíen para Ctrl+Backspace en vez de ambiguos `^H` o `^?`.

**Caveat**: el shell del usuario puede necesitar config para interpretar estos codes. Pero:
- zsh/bash con defaults modernos ya manejan muchos
- Claude CLI (lo que realmente corre en el terminal) no depende de ellos
- El usuario puede seguir bindeando manualmente (`bindkey` / `bind -x`)

**Por qué**: nos alinea con Ghostty (que usa el mismo protocol por default) y cierra el gap del "Ctrl+Backspace no borra palabra" que fue el detonante de la pregunta.

**Opt-out**: config setting en `~/.config/cluihud/config.toml` → `terminal.kitty_keyboard = false`.

## Risks / Trade-offs

### Riesgo: pérdida de features de xterm.js

xterm.js es un proyecto maduro con años de pulido en bordes raros: IME composition, screen reader support, accessibility, DOM selection. Al migrar perdemos acceso a ese pulido gratis.

**Mitigación**:
- IME composition: replicar el pattern `compositionstart/end` (hoy ya tenemos en `wireIMEFix`) sobre el canvas. El texto compuesto se envía via `terminal_input` con el text composed.
- Selection: implementar mouse-drag selection directamente en el canvas (coordenadas → row/col → cell range). Copy via `navigator.clipboard.writeText`.
- Accesibilidad: **deferred**. MVP acepta regresión. Añadir `aria-live` region con los últimos ~5 lines post-MVP.

### Riesgo: performance en outputs masivos

Un `cat` de un archivo de 10MB puede producir 100k+ escape sequences. El parser wezterm-term es rápido (es el que usa wezterm.app) pero el IPC bridge puede saturarse si no se coalescen updates.

**Mitigación**:
- Coalescing window de 8ms (ver Decision 3)
- Backpressure: si el frontend no puede seguir, backend drop old diffs (solo importa el último estado)
- Tests de stress con outputs de 50k+ líneas

### Riesgo: font rendering cross-platform

Canvas2D `fillText` usa la font stack del OS. Linux/WebKitGTK tiende a tener hinting distinto que Firefox/Chrome. xterm.js DOM delega al browser y termina más consistente.

**Mitigación**:
- MVP es Linux-first (igual que el resto de cluihud)
- Bundle una fuente específica (JetBrains Mono via `@fontsource`) para control total
- Tests visuales manuales en Linux; post-MVP para macOS/Windows si aplica

### Trade-off: reescribir lo que xterm.js da gratis vs. ganar control

Accept el trade-off. La ganancia (parser único, OSC 8, Kitty kb, extensibilidad) justifica el costo. El MVP acepta regresiones menores en bordes (ligaduras, accessibility, raw performance marginal) a cambio de control total del modelo.

## Migration Plan

Feature branch (`feat/libghostty-terminal` → renombrable) con flag interno `experimental.wezterm_terminal = false` por default. Ambos paths coexisten hasta completar el MVP:

1. **Fase 1 — Backend VT core** (sin tocar frontend):
   - Añadir `wezterm-term` a Cargo.toml
   - Implementar `TerminalSession` con `advance_bytes`, `key_down`, `resize`, `grid_snapshot`
   - Tests de unidad con sequences conocidas
2. **Fase 2 — IPC contract**:
   - Nuevos events `terminal:grid-update`
   - Nuevos commands `terminal_input`, `terminal_resize`, `terminal_get_full_grid`
   - Backend emite en paralelo a `pty:output` (ambos flows vivos)
3. **Fase 3 — Frontend renderer**:
   - `TerminalRenderer` + `TerminalCanvas` detrás del flag
   - Toggle en settings para switchear xterm.js ↔ wezterm
4. **Fase 4 — Paridad funcional**:
   - Selection, copy/paste, scrollback, cursor, theme, resize
   - Kitty keyboard protocol + input encoding
   - Tests E2E manuales con Claude CLI real
5. **Fase 5 — Cut-over**:
   - Flag on por default
   - Soak period: 1 semana de uso personal
6. **Fase 6 — Cleanup**:
   - Remover `@xterm/xterm` y `@xterm/addon-fit` de `package.json`
   - Remover `pty:output` event (solo queda `terminal:grid-update`)
   - Remover el flag y el código antiguo

El cambio es reversible vía flag hasta Fase 5. Post-Fase 6 la reversión requiere revertir commits.
