Linear: FRI-597

## Why

Nergal se cierra entero cuando una app de terminal manda una imagen sixel o iTerm2: yazi lo hizo en la quake al previsualizar una imagen. Nergal crea el emulador con un tamaño de 0 × 0 píxeles. wezterm-term divide por el ancho en píxeles de una celda al colocar la imagen, y el perfil release (`panic = "abort"`) convierte ese panic en el cierre de la app. Además, el log de la corrida que se cayó se pierde, porque cada arranque lo trunca.

## What Changes

- El emulador y el PTY reciben la geometría real en píxeles del canvas: columnas × ancho de celda y filas × alto de celda, en píxeles del dispositivo. El frontend la manda al crear el terminal, al redimensionar y cuando cambia el tamaño de la celda.
- El emulador nunca recibe menos de un píxel por celda. Si el frontend no manda la geometría, o manda 0, el backend usa un tamaño de celda por defecto. Así una imagen no puede provocar una división por cero.
- Las apps que preguntan el tamaño en píxeles (`CSI 14 t`, `CSI 16 t`, `TIOCGWINSZ`) reciben valores reales en vez de 0.
- Al arrancar, Nergal conserva el log de la corrida anterior como `nergal.log.1` antes de abrir el nuevo `nergal.log`.
- Solo un proceso a la vez es dueño del log, mediante un lock. Hoy una segunda ejecución (el launcher o un link `nergal://`) trunca el log de la instancia que está corriendo, porque el log se abre antes del chequeo single-instance.
- Nergal sigue anunciando sixel. Dibujar las imágenes en el canvas queda fuera de este change: es FRI-600.

## Capabilities

### New Capabilities

- `app-diagnostics-log`: dónde escribe Nergal su log de proceso y cuánta historia conserva entre arranques.

### Modified Capabilities

- `terminal-wezterm`: nuevo requirement. El emulador y el PTY reciben la geometría en píxeles del canvas, y una imagen nunca tira la app.

## Impact

- Backend: `src-tauri/src/terminal/session.rs` (construcción y resize del emulador), `src-tauri/src/pty.rs` (`spawn_pty`, `resize_pty`, comandos `start_claude_session`, `spawn_aux_shell`, `resize_session_terminal`), `src-tauri/src/lib.rs` (apertura del log).
- Frontend: `src/components/terminal/terminalService.ts` (spawn, `fit`, cambio de fuente).
- IPC: los tres comandos ganan dos parámetros opcionales de píxeles. Un llamador que no los manda sigue funcionando con el tamaño de celda por defecto.
- Sin cambios de schema ni dependencias nuevas.
