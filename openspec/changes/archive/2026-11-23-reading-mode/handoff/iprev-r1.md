1. **Medium: arranque, el config llega después del spawn.** Lo cubren "Per-phase risk" y el escenario "Flags survive a restart". El plan acepta que una terminal arranque antes de que el config cargue y que el refit la corrija después. Eso cumple el spec, pero con dos efectos que el plan no menciona:
   - Con el flag guardado en on, en cada arranque Claude Code primero se lanza a ancho completo (`terminalService.ts:408`) y unos ms después recibe un SIGWINCH a 100. El banner inicial se dibuja a 180 y luego se re-envuelve: hay flicker en cada restart. El comentario en `terminalService.ts:1158-1161` ya advierte que `claude` cachea las dimensiones al iniciar.
   - Si el user aprieta `leader f` antes de que resuelva `get_config` (`App.tsx:124-127`), `setConfig(cfg)` pisa el `configAtom` con el valor viejo. La UI vuelve a off, pero el disco quedó en `true` (el `save_config` parcial ya se mandó), así que el próximo arranque contradice lo que el user vio.

   No hay ninguna señal de "config hidratado" en `src/`. El plan debe aceptar y documentar ambos efectos, o hacer que el primer spawn espere la hidratación.

2. **Medium: import circular frágil.** Lo cubren "Store" y "Terminal". `shortcuts.ts:6` importa `terminalService`. Con el plan, `terminalService` importa `readingMode.ts`, y `readingMode.ts` necesita `focusZoneAtom` de `shortcuts.ts` para que `toggleReadingMode(store)` lea la zona. Eso arma el ciclo `shortcuts → terminalService → readingMode → shortcuts`. Hoy funciona solo porque `focusZoneAtom` se lee dentro de la función. Si alguien lo usa a nivel de módulo (por ejemplo un atom derivado), falla con TDZ según qué módulo cargue primero. Fix concreto:
   - `toggleReadingMode(store, zone)` recibe la zona desde el handler del registry.
   - `readingMode.ts` usa solo `import type { FocusZone }`.

3. **Medium: los escenarios de terminal no tienen test automático.** Lo cubren el spec "Terminal column cap" y la tarea 3.1. El spec exige `resize_session_terminal SHALL receive cols = 100` (y `cols = 180` al apagar) y un borde izquierdo a 320 px, pero solo se testea `capReadingCols(cols, region, enabled)`, un `min()` trivial. Lo que realmente carga el riesgo es el wiring:
   - el cap en ambos call sites (`:408` y `:1215`);
   - el orden respecto al fallback de 80×24 en `computeCols`;
   - `justifyContent` solo cuando el cap redujo las columnas;
   - el payload deduplicado por `nextResize`.

   Nada de eso se verifica fuera de un walk manual, y el walk no puede ver el payload IPC. Recomendación: un helper puro `readingLayout(containerWidth, containerHeight, metrics, region, enabled) → { cols, rows, centered }` que absorba `computeCols` más el cap, con tests para 1440 px / 8 px → `{cols:100, centered:true}`, pane angosto, flag off, quake y ancho de fallback menor a 10 columnas. Más un test de `nextResize(lastRequested(cols=180), …cols=100)`, que debe devolver un payload no nulo.

4. **Low: la suscripción a nivel de módulo en `terminalService.ts`.** Lo cubre "Terminal": `appStore.sub(readingTerminalAtom, () => requestAnimationFrame(...))` al cargar el módulo.
   - Nunca se desuscribe, y con HMR se duplica en dev.
   - Corre en cualquier test que importe `shortcuts.ts`: ya hay 5 (`shortcuts.test.ts`, `leader.test.ts`, etc.). Si un test invoca el handler del registry sobre `appStore`, el listener llama a `requestAnimationFrame`, que no existe en el env node de vitest (`vitest.config.ts` no configura DOM), y tira `ReferenceError` dentro del flush de Jotai.

   Es mejor ponerla en `TerminalManager.tsx`, como `useEffect(() => appStore.sub(...), [])` junto al `ResizeObserver` (`TerminalManager.tsx:73-81`). Así tiene cleanup y vive en el mismo ciclo que el refit existente. Como `TerminalManager` monta antes de que resuelva `get_config`, el caso de arranque sigue cubierto.

5. **Low: las sesiones en background no cumplen el flag "global" hasta mostrarse.** Lo cubre "Per-phase risk / Hidden center terminals". `fitActive` solo ajusta la entry activa. Los PTY de las otras sesiones siguen en 180 columnas y sus agentes escriben a ese ancho hasta que se muestran (`showTerminal` → `fit`, `terminalService.ts:327-333`). Al cambiar de sesión llega un SIGWINCH y el contenido se re-envuelve. Es aceptable, pero el spec dice "global per surface" y no lo aclara: hay que documentarlo en el requirement o en el walk.

6. **Low: el toggle no avisa cuando no se ve ningún cambio.** Lo cubre "Reading mode toggle follows the focused surface". Hay tres casos:
   - con el pane en 100 columnas o menos, `leader f` en la zona terminal cambia el flag y lo persiste sin efecto visible;
   - lo mismo en la zona panel con un tab que no es markdown (Browser, Git, Activity);
   - y cuando el panel mide menos de `72ch`.

   El user vuelve a apretar y deja el flag al revés de lo que cree. Conviene un toast (`toastsAtom` ya se usa en `shortcuts.ts:27`) que diga qué flag quedó en on u off. Si el feedback se descarta, que sea explícito en el spec.

7. **Low: el walk de markdown cubre menos que el spec.** Lo cubren la tarea 5.4 y "Markdown width cap". El walk lista Plan, Spec, nota de Obsidian y un issue de Linear. Faltan las descripciones de ClickUp y los comentarios de Linear y ClickUp (`LinearTaskView.tsx:1331`, `ClickUpTaskView.tsx:1193,1282`). En los comentarios el cap es por view: el cuerpo queda centrado dentro de una card de ancho completo, con el autor y la metadata alineados a la izquierda. Es el caso visual más raro y no se revisa. Además, con `px-4` en el mismo root y `box-sizing: border-box`, el texto real queda en unos 67ch. Cumple "at most 72ch", pero el walk dice "about 72 characters".

8. **Low: rollback.** `Config` no usa `deny_unknown_fields` (`config.rs:119`), así que un downgrade lee el config sin error y solo pierde los dos flags en el próximo save. Es reversible y no hay migración de datos. Bien resuelto, sin cambios necesarios.

Verifiqué y están correctos: el mouse mapea contra el rect del canvas (`terminalService.ts:997-998`) y el `onMouseDown` del host re-enfoca el textarea cuando se hace click en el margen (`TerminalManager.tsx:114-119`). `save_config` mergea keys parciales (`config_misc.rs:35-59`). La toolbar de anotaciones es `fixed` con coordenadas de viewport (`PlanAnnotationToolbar.tsx:202`), así que centrar el root no la mueve. `leader f` está libre y el grupo `surfaces` existe. Los ocho usos de markdown en el panel derecho pasan por las dos views.

VERDICT: APPROVED
