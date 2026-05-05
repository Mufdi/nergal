## Why

El terminal pane de cluihud usa hoy `@xterm/xterm` + `@xterm/addon-fit`: renderer DOM en webview y parser VT en JavaScript. Esto nos deja tres limitaciones estructurales:

1. **Parser VT en JS**, desalineado del resto del stack (Rust). Los mismos bytes que Claude emite por stdout también llegan al transcript watcher, hooks y eventualmente al diff panel — hoy cada consumidor parsea por separado. Un parser único en Rust centraliza el modelo de escape sequences.
2. **Features ricas faltantes**: sin OSC 8 hyperlinks (rutas clickeables en outputs de Claude), sin encoding correcto de Ctrl+Backspace o Shift+Enter por default, sin Kitty keyboard protocol.
3. **Extensibilidad limitada**: anotaciones sobre output del terminal, búsqueda incremental, scrollback programático — todo bloqueado por vivir la verdad del terminal dentro de xterm.js.

La decisión es mover la emulación VT al backend Rust usando `wezterm-term` (Variante A: webview + parser Rust + renderer `<canvas>` en frontend). El flujo de input del usuario hacia el PTY y el sistema de hooks permanecen idénticos — solo cambia dónde vive el parser y cómo se renderiza el grid.

## What Changes

- **Backend**: nuevo módulo `src-tauri/src/terminal/` con `wezterm-term::Terminal` por sesión. El reader thread del PTY alimenta `advance_bytes()` en vez de emitir bytes raw al frontend.
- **Backend → Frontend IPC**: se reemplaza el evento `pty:output` (bytes) por `terminal:grid-update` (snapshot/delta del grid de celdas). Los bytes raw dejan de cruzar el puente Tauri.
- **Frontend**: se elimina la dependencia de `@xterm/xterm` y `@xterm/addon-fit`. Se introduce `TerminalCanvas` (React component) que dibuja el grid en `<canvas>` usando un font atlas.
- **Input**: `keydown` del canvas → envía `{key_code, modifiers, text}` vía nuevo comando `terminal_input` → backend encodea con `wezterm-term::input` y escribe al PTY. El contrato externo (usuario teclea → bytes al PTY) se preserva byte-a-byte.
- **Kitty keyboard protocol**: habilitado por default, habilita Ctrl+Backspace, Shift+Enter, Alt+letter, etc. con encoding no ambiguo. El shell del usuario sigue necesitando los bindings correspondientes (ej. `bindkey '^H' backward-kill-word`) — no es algo que la app fuerce.
- **Hooks**: sin cambios. El sistema de hooks vive en Unix socket + file watchers, completamente ortogonal al terminal rendering.
- **BREAKING (interno)**: contrato IPC `pty:output` → `terminal:grid-update`. No afecta hooks ni API pública, pero cualquier consumidor directo de `pty:output` necesita migrar.

## Capabilities

### New Capabilities
- `terminal-wezterm`: emulación VT en backend Rust vía wezterm-term, con grid renderizado en canvas frontend, encoding de input vía wezterm (incluyendo Kitty keyboard protocol), y preservación del flujo PTY existente.

### Modified Capabilities
<!-- Ninguna capability existente cambia públicamente. El cambio es internal: mover el owner del parser VT. -->

## Impact

- **Dependencies**:
  - Añadir `wezterm-term` y `termwiz` vía **git dependency** en `src-tauri/Cargo.toml` — ninguna está publicada en crates.io, solo viven dentro del workspace de wezterm. Pinneamos al último tag estable (`20240203-110809-5046fc22`, feb 2024). El core VT logic no ha tenido cambios materiales desde entonces (wezterm está en "spare time" mode pero el core es estable).
  - Remover `@xterm/xterm` y `@xterm/addon-fit` de `package.json` (al final del rollout)
- **Backend**:
  - Nuevo módulo `src-tauri/src/terminal/` con `TerminalManager`, `TerminalSession`, `GridDiffer`, encoder de input
  - `pty.rs` delega bytes raw a `TerminalSession::advance_bytes()` en vez de emitir directo
  - Nuevos commands Tauri: `terminal_input`, `terminal_resize`, `terminal_get_full_grid`
- **Frontend**:
  - Reemplazo de `src/components/terminal/terminalService.ts` por `TerminalRenderer` (class no-React, igual al patrón actual de vivir fuera de React)
  - Nuevo `TerminalCanvas.tsx` que monta el renderer
  - Font atlas: pre-render de glyphs en OffscreenCanvas para performance
- **Tests**:
  - Backend: tests de snapshot del grid para sequences comunes (Claude output, progress bars, syntax highlighting)
  - Frontend: smoke test de render + input echo
- **UX**:
  - Paridad visual con el theme actual (colores, cursor, selection)
  - Paridad funcional mínima: copy/paste, selection, scrollback
  - **Deferred (post-MVP)**: ligaduras de fuente, sixel, búsqueda Ctrl+F, accesibilidad screen-reader
- **Riesgos**:
  - Canvas rendering sin ligaduras de monospace (xterm.js tampoco las soporta bien, pero con DOM al menos delega al browser)
  - Volumen de mensajes IPC en outputs grandes (ej. `cat` de archivo) — mitigación: coalescing de deltas y throttle
  - Font rendering cross-platform (inicialmente Linux-first, igual que la app)
