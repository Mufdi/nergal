---
type: review
feature: launch-prompt-via-file
reviewer: adversarial
reviewed_at: 2026-11-23T00:00:00Z
verdict: FAIL
---

# Review — launch-prompt-via-file

> Rules: review only the diff and the artifacts named below. Skip anything a gate or CI already enforces (types, lint, formatting, test pass/fail). Tag every finding with its pass and its severity. **Important** = would ship a defect, a vulnerability, or a divergence from spec/plan. **Nit** = everything else; **max 5 nits**, drop the rest.

## Adversarial

### Diff scope

- Files changed: 8 (7 modificados + `src-tauri/src/pty_launch_prompt_tests.rs` nuevo) · LOC: +509 / -41, más 663 líneas de tests.
- `cargo test --lib launch_prompt`: 23 passed, con bash y zsh reales.

### Pass 1 — Bugs

No hay findings Important. Revisé estos puntos y se cumplen:

- Todo error después de `reserve_session_pty` sale por `?` o `return Err` con el guard armado: `spawn_pty` (`src-tauri/src/pty.rs:1046-1058`), `Gone` (`:1062-1070`), adapter faltante (`:1111`), `adapter.spawn` (`:1183`), `prepare_launch` (`:1212-1221`), locks y `send_launch` (`:1230-1242`).
- El guard se declara en `:1042`, antes de los lock guards de `:1230-1231`. En un `?` se liberan los locks primero y luego corre el rollback.
- El orden de locks es `session_ptys` → `instances` → `launched_ptys`. Ningún sitio toma `session_ptys` con `instances` tomado.
- `disarm()` corre dentro del bloque de locks (`:1243`). `Gone` deja el guard armado.
- `Drop` (`:626-649`) usa `lock().ok()` e ignora errores de I/O.
- `--continue`, `--resume <id>` y `resume_pick` quedan antes de `--`. Codex y OpenCode mantienen la regla de no enviar prompt en resume.

### Pass 2 — Security

- [Nit] `src-tauri/src/pty.rs:488-500` — En un host unix, `shell_kind` clasifica `pwsh` como PowerShell (`src-tauri/src/config.rs:56`). Entonces `resolve_launch_prompt` devuelve `Inline` y teclea el prompt no confiable en el shell. Esto ocurre también en Linux, la plataforma del focus freeze. Nadie verificó en `pwsh` que `--` llega al binario. La spec lo permite (MAY), pero la proposal lo vende como security fix. → Usar el archivo también para PowerShell en unix, o rechazar un prompt con saltos de línea en shells no POSIX. Como mínimo, abrir un issue en Linear.
- [Nit] `src-tauri/src/agents/mod.rs:387-401` — `launch_prompt_file_path` usa una denylist. Acepta `!` y `"`. La ruta va dentro de `"$(…)"`, y ahí las comillas simples no frenan la history expansion de bash. Los pty ids salen de UUIDs, así que hoy no es explotable. → Cambiar a una allowlist `[A-Za-z0-9._-]`.

### Pass 3 — Compliance

- [Important] `openspec/changes/launch-prompt-via-file/tasks.md:6,14`, `design.md:84`, `implementation.md:62-63,74` — `tasks.md` marca `[x]` la tarea 1.5, que pide `deliver_launch`. Esa función no existe. La tarea 1.1 pide tests de `deliver_launch` con callback `on_file`, que tampoco existen. `design.md` D3 dice que `deliver_launch` borra el archivo si falla la escritura. El código usa `prepare_launch` + `send_launch`, y el archivo lo borra el guard. Hay dos diseños para la misma entrega, y un `[x]` que el código no respalda. → Pasar `design.md` D3, `tasks.md` 1.1/1.5 e `implementation.md:62-63,74` a `prepare_launch`/`send_launch` con `openspec instructions`. Luego correr `openspec validate`.
- [Important] `openspec/specs/agent-adapter/spec.md:17,44` y `openspec/specs/post-session-runner/spec.md:33` — La delta solo agrega requisitos (ADDED). La spec principal sigue diciendo `SpawnSpec { binary, args, env }`. También dice que binary, args y env "are sufficient for the runtime to launch the agent", y con prompt eso ya no es cierto. `post-session-runner` sigue diciendo que el EOF del PTY dispara el marker. Ahora solo lo dispara un PTY que escribió su comando (`src-tauri/src/pty.rs:814-820`). Al archivar, las specs principales quedan en contradicción con el código. → Agregar deltas MODIFIED para "AgentAdapter trait declares identity…" (con `prompt`) y para "Marker file lifecycle" (EOF solo de un launch entregado). `obsidian-context-injection/spec.md:10,20` también habla de "positional" y `--prompt`: conviene actualizarla en el mismo paso.
- [Important] `src-tauri/src/pty.rs:814-820` — El escenario "A launch that never wrote its command writes nothing to the vault" tiene una segunda cláusula. Una launch posterior que sí escribe su comando debe finalizar al EOF. Esa cláusula protege a todas las sesiones normales contra una regresión silenciosa del footer y del marker. Ningún test cubre ese wiring, y ningún walk lo cubre. `evidence.md:45` afirma que los walks 2.3–2.7 cubren el wiring. Ninguno de esos walks revisa el vault ni cierra una sesión durante el arranque. → Agregar un walk 2.8: abrir una sesión con log de Obsidian, salir del agente y del shell, y confirmar el footer y `pending-mocs/<sid>.json`. Agregar un walk 2.9: cerrar la pestaña en el primer segundo y confirmar que no hay footer ni archivo en `launch-prompts/`. Corregir `evidence.md:45`.
- [Nit] `src-tauri/src/pty.rs:443-444` — El doc comment dice que un `File` en un shell no POSIX "renders nothing". El código de `push_prompt` (`:447-460`) renderiza la sustitución POSIX para cualquier shell. El WHY afirma algo falso. → Borrar la frase o hacer que ese brazo no renderice nada.
- [Nit] `src-tauri/src/agents/claude_code/adapter.rs:178,208`, `src-tauri/src/pty.rs:1151-1153` — Los comentarios siguen diciendo "must precede the positional prompt below" y "the adapter folds it into the launch command". El adapter ya no agrega el prompt. Además, `docs/hooks.md:5` y `CLAUDE.md:11` listan el contenido de `ipc_dir()` sin `launch-prompts/`. → Actualizar esos comentarios y agregar `launch-prompts/` a ambas listas.
- [Nit] `src-tauri/src/agents/codex/adapter.rs:479-504`, `src-tauri/src/agents/opencode/adapter.rs:599-629`, `src-tauri/src/agents/claude_code/adapter.rs:507-540` — Los tests nuevos usan `if let Ok(spec) = a.spawn(..)`. `spawn` llama a `which` y falla si falta el binario. En los runners de CI no hay `claude`, `codex` ni `opencode`. Entonces el mapeo a `AfterDoubleDash` o `Flag("--prompt")` solo se prueba en la máquina de desarrollo, y en CI pasa sin assert. El patrón ya existía. → Como mínimo, imprimir el skip. Mejor: separar el armado de `prompt` en una función pura y testearla.

#### Spec conformance (delta `specs/agent-adapter/spec.md`)

- Acceptance criteria: [x] Prompt with history-expansion triggers — `pty_launch_prompt_tests.rs:231,236,241`.
- Acceptance criteria: [x] Prompt text never appears in the typed command — `pty_launch_prompt_tests.rs:301`.
- Acceptance criteria: [x] Prompt file is private and deleted by the launch — `pty_launch_prompt_tests.rs:275,221`; `agents/mod.rs:403`.
- Acceptance criteria: [x] Missing prompt file skips the agent — `pty_launch_prompt_tests.rs:253`; `pty.rs:376`.
- Acceptance criteria: [x] Prompt is normalized before delivery — `pty_launch_prompt_tests.rs:394,489`; `pty.rs:481`.
- Acceptance criteria: [x] No prompt adds nothing — `pty_launch_prompt_tests.rs:415`.
- Acceptance criteria: [x] Leftover prompt files are swept at startup — `pty_launch_prompt_tests.rs:653`; `lib.rs:1150`.
- Acceptance criteria: [x] Option-like prompt for Claude Code — `pty_launch_prompt_tests.rs:325`.
- Acceptance criteria: [x] Option-like prompt for OpenCode — `pty_launch_prompt_tests.rs:347`.
- Acceptance criteria: [x] Prompt file cannot be written — `pty_launch_prompt_tests.rs:449,556`; `pty.rs:1212-1221`.
- Acceptance criteria: [x] Writing the command to the PTY fails — `pty_launch_prompt_tests.rs:515,556`.
- Acceptance criteria: [.] The session was killed during startup — piezas en `pty_launch_prompt_tests.rs:515,556,612`; wiring en `pty.rs:1062-1070,1232-1242`. Lo acepto: la lógica está en funciones testeadas y el wiring es corto.
- Acceptance criteria: [x] A newer queued prompt wins — `pty_launch_prompt_tests.rs:574`.
- Acceptance criteria: [.] A launch that never wrote its command writes nothing to the vault — `take_launched` en `pty_launch_prompt_tests.rs:570,589,612`. La cláusula positiva no tiene test ni walk (ver Important arriba).
- Acceptance criteria: [x] Retry after a failed launch spawns a new shell — `pty_launch_prompt_tests.rs:629`; `pty.rs:1154-1159`.

### Harness entropy check

- Speculative abstractions: no. `PromptAttach::Flag(&'static str)` tiene un solo valor, pero modela el caso de OpenCode sin hardcodear.
- WHAT-comments: no. Hay un WHY falso (`pty.rs:443-444`, ver Nit).
- New deps: no.

### Verdict

FAIL — El código cumple el invariante de rollback, el orden de locks y la entrega sin parser en POSIX. No encontré bugs Important en `src-tauri`. Los tres Important son de cumplimiento. Hay artefactos con `[x]` sobre una API inexistente. Hay specs principales que quedan en contradicción al archivar. Y nada verifica que las sesiones normales sigan finalizando en el vault al EOF. Los tres se corrigen sin tocar código de producción.

Verdict: FAIL (3 Important)

## Security

### Diff scope

- Mismo diff que `## Adversarial`. Revisé `src-tauri/src/pty.rs`, `src-tauri/src/agents/mod.rs`, los cuatro adapters, `src-tauri/src/platform/mod.rs` y `src-tauri/src/config.rs`.
- Probes en un PTY real, en un directorio temporal: bash 5.1.16 y zsh 5.8.1, con `magic-space` activo. No toqué `/run/user/<uid>/nergal`.

### Pass 2 — Security

- [Important] `src-tauri/src/pty.rs:673-675`, `src-tauri/src/pty.rs:499`, `src-tauri/src/pty.rs:468-474` — En PowerShell el prompt sigue inline, y `ps_quote` no lo encierra de forma segura.
  - Trust boundary: texto no confiable → parser del shell del usuario. Las fuentes son MCP `request_session_resume` y `create_worktree_session`, el deep link `nergal://session/new` y los issues de Linear o ClickUp.
  - Alcance: en Linux, `shell_kind` clasifica `pwsh` como PowerShell (`src-tauri/src/config.rs:56`). Basta con `$SHELL` o `default_shell` en `pwsh`. En Windows, PowerShell es el shell por defecto (`src-tauri/src/config.rs:25`).
  - Causa: PowerShell acepta cuatro comillas simples extra: U+2018, U+2019, U+201A y U+201B. `ps_quote` solo duplica `'`.
  - Resultado concreto: el prompt `’; <comando> #` se teclea como `-- '’; <comando> #'`. La `’` cierra el string. `<comando>` corre con la cuenta del usuario. `#` comenta el resto.
  - Efecto sin atacante: un prompt con "it’s" deja el string abierto. PSReadLine queda en `>>` y el agente no arranca.
  - Evidencia: `pwsh` no está instalado en este host, así que no hubo probe en vivo. La regla sale de la PowerShell Language Specification (`single-quote-character`).
  - Contexto: el riesgo ya existía en `main`, con el mismo `ps_maybe_quote` sobre `args`. D5 lo deja fuera como "frozen Windows shells". Esa premisa no cubre `pwsh` en Linux.
  - Este finding reemplaza el Nit de `pwsh` de `## Adversarial`. La ruptura no necesita saltos de línea ni `--`.
  - Fix mínimo: en `ps_quote`, duplicar cada una de las cuatro comillas extra (`c` → `cc`). En `ps_maybe_quote`, sumar U+2018..U+201B al set que obliga a quotear. Agregar un test con `’; touch x #` que verifique un solo string.
  - Fix preferido: en unix, usar también el archivo para PowerShell, o fallar cerrado con rollback y requeue. `cmd_quote` no escapa `"`, así que cmd (solo Windows) también debe fallar cerrado con prompt.
- [Nit] `src-tauri/src/agents/mod.rs:387-401` — Corrección del Nit de stem de `## Adversarial`. Su premisa es falsa en bash y zsh.
  - Probe: `echo "$(command cat -- '/n/a!!b!#$c"d`e.txt')"` con `magic-space` activo. En la misma sesión, `"x!!y"` sí expandió. La ruta llegó intacta.
  - La denylist (`'`, `/`, `..`, espacios, control) basta para POSIX. En fish, `\` es escape dentro de comillas simples. Un `\` en el stem solo cambia la ruta, y `test -r` falla cerrado.
  - Hoy el stem es `pty-<hex o uuid>-<millis>`. Ningún actor externo lo controla, así que no hay boundary.
  - → La allowlist `[A-Za-z0-9._-]` sigue siendo buena higiene, sin urgencia.

### Controles verificados (sin finding)

- POSIX: el typed line no lleva bytes del prompt. Lleva solo la ruta, fija y entre comillas simples. El line editor y los widgets ZLE o readline nunca ven el texto.
- Shells clasificados como POSIX sin serlo: fish 3.4+ trata `"$(…)"` como una palabra sin re-parseo. fish anterior a 3.4, nushell y elvish fallan al parsear y no corre nada. xonsh pasa la ruta literal al agente. En ningún caso el texto del prompt pasa por un parser.
- Opciones: el prompt va al final, después de `--`, o pegado a `--prompt=`. `--resume <id>`, `--continue` y `resume_pick` quedan antes de `--`. Codex y OpenCode no reciben prompt en resume.
- Archivo: `create_new` (`O_CREAT|O_EXCL`) con modo `0600` no sigue symlinks. El padre es `0700` y del mismo uid, validado por `ensure_ipc_dir_0700`. El fallback en home pasa la misma validación y `validate_base_dir_owned_by`.
- Otro usuario local no puede crear, cambiar ni leer el archivo entre la escritura y el `cat`. No hay TOCTOU entre usuarios.
- Historial: antes, zsh sin `HIST_IGNORE_SPACE` guardaba el prompt entero. Ahora guarda solo la ruta. Al repetir la línea, `test -r` falla y el agente no corre. El stem lleva millis, así que no choca con un launch nuevo.
- Rollback: el comando entra en un solo `write_all`, con `\n` como último byte. Una escritura parcial nunca envía la línea, y el rollback mata ese shell.
- Destino: el writer se elige bajo `session_ptys` e `instances`, por `pty_id` y reserva. El archivo usa el mismo `pty_id`. El requeue usa el mismo `session_id`. No hay entrega a otra sesión ni a otro PTY.
- `/proc/<pid>/cmdline`: la exposición no cambia (non-goal de D1).

### Verdict

FAIL — En POSIX el cambio cierra los dos vectores: el texto no pasa por el shell y el agente no lo lee como opción. El archivo y el rollback no cruzan ningún boundary nuevo. Queda un Important: en PowerShell, incluido `pwsh` en Linux, una comilla tipográfica rompe el quoting. Un issue de Linear u otro agente puede ejecutar comandos en el shell del usuario. El fix mínimo es local a `ps_quote` y se cubre con un test.

Verdict: FAIL (1 Important)
