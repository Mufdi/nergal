# Walk final — Windows (los 5 changes del port)

Validación runtime en la máquina Windows. Cubre lo que CI **no** puede: CI solo
compila (`cargo check`) + corre el pipeline de release; el comportamiento real
(named pipes, kill de procesos, spawn detached, instalador) se camina aquí.

Un solo build camina los 5 changes juntos (compile + ipc + proc + desktop +
bundle-ci).

## 0. Build / install

Dos formas de obtener el binario a caminar:

**(A) Build local (recomendado — espejo del walk Linux con `dpkg -i`):**
```powershell
pnpm install --frozen-lockfile
pnpm tauri build
```
Artefactos en `src-tauri\target\release\bundle\`:
- `nsis\*-setup.exe` (instalador NSIS)
- `msi\*.msi` (instalador WiX)
- `nsis\*-setup.exe` (+ `.sig`) — este Tauri firma el `-setup.exe` in-place; el
  artefacto del updater es el `.exe` + `.exe.sig`, NO un `.nsis.zip`

Instala con el `*-setup.exe` (o el `.msi`). **SmartScreen** mostrará "editor
desconocido" (sin firmar) → *Más información → Ejecutar de todas formas*.

**(B) `cargo test`** (cubre proc 5.2 + ipc 9.2 + bundle-ci 3.1):
```powershell
cd src-tauri
cargo test
```
Debe pasar el suite completo, incluyendo `install_source_installed_windows_is_windows`
(verdict installed→Windows) y `peer_identity_rejects_foreign_sid`.

> ⚠️ Requisitos del build en Windows: Rust stable (MSVC), Node 22, pnpm 10,
> Visual Studio Build Tools (MSVC) y WebView2 (preinstalado en Win11).

---

## Round 2 — re-verificar los fixes del walk-1

El primer walk (2026-06-28) cazó 4 bugs de runtime; todos corregidos en
`fix(platform): unblock Windows session creation…`. Re-caminar primero esto:

- [ ] **Sesión arranca y el panel renderiza** (era el bloqueante): crear una
      sesión muestra el prompt de PowerShell en el panel central (no negro). El
      PTY ahora lee `config.default_shell` (default `powershell.exe`), no `$SHELL`.
- [ ] **Sin flash de consola**: tras crear la sesión, **no** parpadea una ventana
      `cmd` cada pocos segundos (los pollers git/agent-version ahora usan
      `CREATE_NO_WINDOW`).
- [ ] **Log file existe**: Settings › diagnostics "copy logs" muestra el tail real
      (no "no log file"); el archivo está en
      `%LOCALAPPDATA%\nergal\nergal.log`. **Si algo falla, este log es lo que
      necesito** — pasámelo.
- [ ] **Shell configurable**: cambiar `default_shell` en Settings (ej. a `cmd.exe`)
      y crear una sesión nueva la usa **sin reiniciar** la app.
- [ ] **Ports chip sin ruido de sistema**: los ports detectados al arranque ya no
      incluyen los del kernel (pid ≤ 4); no debería aparecer el error
      "refusing to terminate a system pid" salvo que mates algo de sistema a mano.

## 1. Arranque + IPC (windows-compile + windows-ipc)

- [ ] **La app lanza** sin ghost-window ni crash (windows-compile 8.4).
- [ ] **Hook events fluyen**: abre una sesión `claude`, confirma que los paneles
      (Activities/Tasks) se pueblan. El named pipe `\\.\pipe\nergal-<SID>-hook`
      está vivo (revisar el log de arranque: "live hook endpoint found at startup"
      o el bind).
- [ ] **MCP sirve**: desde otra sesión, un `tools/call` al server nergal responde
      (named pipe `\\.\pipe\nergal-<SID>-mcp`).
- [ ] **Plan-review bloquea + resuelve**: dispara un plan (ExitPlanMode), el botón
      aprobar/revisar escribe la decisión por el sync-pipe y el CLI desbloquea.
- [ ] **Ask-user**: una `AskUserQuestion` tinta el tab y se limpia al responder.
- [ ] **Peer auth (security boundary)**: si tenés una 2ª cuenta de Windows, un
      connect desde otro usuario al pipe debe ser **rechazado + logueado**
      (foreign-SID). Confirmar que **no queda impersonation colgada**
      (`RevertToSelf` en todos los error paths — sin warning "thread may still be
      impersonating" en el log).

## 2. Procesos + puertos (windows-proc)

- [ ] **Ports chip** muestra los dev servers corriendo (levantá un `vite`/`next`
      en :3000/:5173 → aparece en el chip). Backend `GetExtendedTcpTable`.
- [ ] **Free-port** (kill desde el chip): mata el proceso dueño del puerto vía
      `OpenProcess`+`TerminateProcess` (sin flash de consola de `taskkill`).
- [ ] **Quake-shell teardown mata el árbol**: levantá un dev server en una quake
      shell, cerrá la sesión → el árbol `pnpm → node` muere (BFS de descendientes,
      sin process-group POSIX). Verificá que no quede el node huérfano.
- [ ] **Quake cwd**: la quake shell resuelve el cwd real o degrada a None
      gracefully (sysinfo PEB).

## 3. Desktop / spawn detached (windows-desktop)

- [ ] **Post-session runner sobrevive al exit de la GUI**: cerrá la app justo
      después de terminar una sesión → el runner `nergal post-session` drena los
      markers (revisar `…\nergal\logs\post-session.log` → "INFO drained N markers").
      **GOTCHA**: si el runner muere con la GUI (un job object de Tauri lo mata),
      avisame → agrego `CREATE_BREAKAWAY_FROM_JOB (0x0100_0000)` a los
      `creation_flags` en `post_session.rs`.
- [ ] **Docker-stop completa tras el exit**: con un compose levantado y owned,
      cerrá Nergal → el `docker compose stop` termina TODOS los services (no se
      corta a la mitad).
- [ ] **Opener/notification/dirs**: open-log abre el `.log`; reveal-in-folder abre
      el explorador; una notificación toast aparece (AUMID post-install); el dir de
      downloads resuelve.

## 4. Deep link + updater (windows-bundle-ci)

- [ ] **Deep link `nergal://`**: un link `nergal://...` abre/enfoca la app
      (registrado por el instalador NSIS).
- [ ] **In-app update → auto-install**: en Settings › About, la fuente dice
      "Windows (installer)"; con un release más nuevo publicado, "Install vX"
      corre el path auto-install de `tauri-plugin-updater` (downloadAndInstall),
      muestra el copy de SmartScreen, y "Restart to apply" relanza.
      *(Este check necesita un release real más nuevo que la versión instalada —
      se valida en el primer release de 3 plataformas, no ahora.)*

---

## Reportar

- ✅ Todo verde → avisá y archivo los 5 changes (`/openspec-sync`, reconciliando
  el delta spec de windows-ipc por la desviación accept-caller-gates).
- ❌ Algo falla → pasame el síntoma + el log relevante
  (`%LOCALAPPDATA%\nergal\logs\` y el log de arranque). Los GOTCHAs anticipados
  (BREAKAWAY_FROM_JOB, cwd→None) tienen fix conocido.
