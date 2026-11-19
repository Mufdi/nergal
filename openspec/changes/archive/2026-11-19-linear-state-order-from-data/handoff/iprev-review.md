1. **Alta: el orden esperado `Duplicate, Canceled` depende de un type `duplicate` que Linear probablemente no emite.**
   - El enum `WorkflowState.type` de Linear es triage, backlog, unstarted, started, completed y canceled. Duplicate llega como `canceled`. Lo dice el comentario actual de `src/lib/linearStateOrder.ts:18` y lo asume el test existente `s("Duplicate", "canceled")`.
   - `mirror.rs:68` guarda `state_type` tal cual viene de la API, sin derivar nada.
   - Con la regla settled (closed asc por position) y type real `canceled`, Canceled (4) queda antes que Duplicate (5). El resultado sería `…Shipped, Canceled, Duplicate`, no `…Duplicate, Canceled`.
   - El escenario RR y la tarea 2.2 afirman un orden que la regla no produce con datos reales. Eso rompería el walk.
   - Acción: leer los types reales de `linear_workflow_states` en la DB para team RR. Después, o se corrige el orden esperado en el spec y los tests (Canceled antes que Duplicate), o se documenta explícitamente que `duplicate` existe en datos. No hay que inventarlo en las fixtures.
   - `LinearStatusIcon.tsx:59` ya acepta `duplicate`, pero eso no prueba que el mirror lo reciba.

2. **Alta: `position` es por team y los grupos se mezclan por nombre entre teams.**
   - `groupByState` (`linearStateOrder.ts:103`) y `stateFilterOptions` (`:72`) agrupan por nombre normalizado. Usan como `state`/`sample` el primer issue visto.
   - Si hay dos teams con "In Progress" y positions distintas, el rank del grupo depende del orden de llegada de los issues. Antes eso era determinista por nombre.
   - En `groupIssues` (`LinearPanel.tsx:211`) la key es `stateId`, así que cada team tiene su propio grupo. Positions de teams distintos se interleavan sin sentido, por ejemplo "In Review" de un team contra "In Progress" de otro.
   - Acción: decidir y documentar el comportamiento multi-team. Opciones: ordenar por (type, position) y desempatar por nombre, o usar la mediana o el mínimo de position por grupo. Agregar un test de dos teams.

3. **Media: faltan tiebreak y manejo de position ausente.**
   - `statePosition` es `Option<f64>`. Los placeholders sintéticos (`mirror.rs:80`) tienen position NULL, type `backlog` y nombre `(state id)`. Con `?? 0` y orden desc en tipos abiertos, un placeholder cae al final del backlog. En tipos cerrados caería primero. Hay que decidir si eso es intencional.
   - Definir el orden de los empates: position igual o ausente se resuelve por nombre. Sin eso, el orden depende de la inserción del Map.
   - Agregar tests de position null y de empate.

4. **Media: el closed set debe conservar el spelling `cancelled`.** El plan lista `completed`, `duplicate` y `canceled`, pero hoy `RANK_BY_TYPE` incluye `cancelled` y el test de la línea 57 lo cubre. Si se pierde, un state con ese type pasa a ser abierto y el H deja de plegarlo. Conservarlo en la tabla de types y en el test.

5. **Media: el rank pasa de un número a (type, position), y la tarea 1.2 no lo define.**
   - `stateRank` es exportado y los tests lo comparan con `toBe`. Con position, `stateRank` es solo el rank del type y `compareStates` aplica position y dirección. Queda por especificar qué significa `stateRank(Shipped) === stateRank(Done)` después del cambio.
   - Aclararlo en la tarea. Los tests de igualdad de rank deben seguir siendo solo de type.
   - Los callers (`LinearPanel.tsx:230`, `:262`, `:386`, `:420`, `:514`, `:1520`, `:1524` y `linearFold.ts:17`) usan únicamente `compareStates` e `isClosedState`, así que la API no cambia y nada más necesita edits. Eso es correcto.

6. **Media: el type nulo con nombre sigue ranking como abierto desconocido.** Se conserva `UNKNOWN_OPEN_RANK` para `stateType == null` con nombre, y `NO_STATE_RANK` sin nombre. Dejarlo explícito en la tarea 1.2 y en un test. Hoy esa rama está mezclada con la lógica de nombres que se va a borrar.

7. **Baja: cobertura en `linearFold`.** `linearFold.test.ts` no se menciona en las tareas. Agregar casos de `foldedByStateFilter` y `isGroupFolded` con Shipped (completed) y con un state `started` llamado "Pending", que ahora debe quedar abierto, o sea no folded. `withoutPeeks(isClosedState)` en `LinearPanel.tsx:514` hereda el cambio, pero conviene un test. Los snapshots en `peeked` guardan type y position viejos. Es inocuo, porque `drop` solo usa `isClosedState` y el type no cambia.

8. **Baja: el spec y el walk.**
   - REMOVED+ADDED en vez de MODIFIED es válido si los nombres coinciden con `openspec/specs/linear-mirror`. Correr `openspec instructions` y `openspec validate`, como exige el flujo del proyecto.
   - La tarea 2.2 es el único chequeo del orden real. Hacerla contra datos reales resuelve el punto 1.

9. **Baja: rollback.** Es un cambio puro de frontend y no toca el esquema ni el Rust. Revertir un archivo y su test basta. El plan debería decirlo en una línea.

VERDICT: REVISE

=== Round 2 (sonnet) ===
1. **Alta, resuelto: el tipo `duplicate`.** El plan ahora dice que ambos teams guardan Duplicate con type `duplicate` en `linear_workflow_states`, con las positions que diste (Canceled 4, Duplicate 5). Con esos datos, el orden `…Shipped, Duplicate, Canceled` sale de la regla por tipo. Lo acepto. Hay que actualizar el comentario de `src/lib/linearStateOrder.ts:18` y las fixtures de los tests, que hoy asumen `Duplicate` con type `canceled`. Al reescribir con la tarea 1.1 y 1.2 se cubre solo.

2. **Media, parcial: multi-team.**
   - El desempate por nombre ya cubre el caso de nombres distintos.
   - Sigue abierto el caso de dos teams con un state del mismo nombre y positions distintas. `groupByState` y `stateFilterOptions` agrupan por nombre normalizado y toman el primer issue como `sample`, así que el rank del grupo depende del orden de llegada.
   - Esto contradice la frase "the order does not depend on which issue arrived first".
   - Basta con decidirlo en una línea: usar el mínimo o la mediana de position por grupo, o aceptar la limitación. Conviene añadir un test de dos teams.
   - No bloquea la aprobación.

3. **Baja, resuelto: tiebreak y position ausente.**
   - Quedan definidos: `0` para position ausente y nombre ascendente para empates.
   - Los placeholders `(state id)` (backlog, position NULL) caen con position 0, al final del backlog. Es razonable.

4. **Baja, resuelto: spelling `cancelled`.** Se conserva en la tarea 1.2.

5. **Baja, resuelto: `stateRank` y `compareStates`.** `stateRank` queda como rank solo por type y `compareStates` aplica position y dirección. Los tests de igualdad de rank siguen siendo por type (tarea 1.1b).

6. **Baja, resuelto: type nulo.** Con nombre es abierto desconocido, y sin nombre va al final. Está en la tarea 1.2 y en 1.1b.

7. **Baja, resuelto: cobertura de `linearFold`.** Se reemplaza el caso Pending: Shipped se pliega y un `started` llamado "Pending" queda abierto.

8. **Baja, resuelto: rollback.** Está dicho en una línea.

9. **Baja, redacción.** En la tarea 1.1, la frase "false for a `completed`-free name such as 'Pending' of type `started`" es confusa. Conviene reescribirla como: `isClosedState` es false para un state `started` llamado "Pending".

10. **Baja, proceso.** Antes de implementar, correr `openspec instructions` y `openspec validate linear-state-order-from-data`, como exige el flujo del proyecto. El spec con REMOVED y ADDED necesita que los nombres de los requirements coincidan con los de `linear-mirror`.

VERDICT: APPROVED
