=== Round 1 (claude opus) ===
**Review: `local-branches-chip`**

Comprobé contra el repo los hechos de los que depende el plan. Los dos comportamientos de git que más pesan (el ref completo en `worktree add` y los worktrees borrados a mano) los reproduje en un repo de prueba.

1. **CRITICAL: "Open session" deja el worktree en detached HEAD.** Afecta a `design.md` D4 paso 5 y a `implementation.md` § Locked interfaces `add_worktree_for_branch`. `git worktree add <path> refs/heads/nergal/<slug>` toma el ref completo como commit-ish y hace checkout detached. Lo reproduje: `symbolic-ref HEAD` falla y `worktree list --porcelain` muestra `(detached HEAD)`. Las consecuencias:
   - Los commits de la sesión reabierta no avanzan `nergal/<slug>`.
   - `branch_checked_out` (`worktree.rs:334`) no ve el branch como en uso.
   - Al borrar esa sesión, `remove_worktree_reclaiming` borra el worktree y los commits nuevos quedan sin referencia. Se pierden datos.
   - La frase de D4 "Git itself refuses a branch checked out elsewhere" es falsa con un ref completo: en la prueba, un segundo `worktree add` del mismo branch funcionó.

   Fix: pasar `nergal/<slug>` (el nombre corto; `branch_state` ya garantizó `Exists`). Después del add, verificar con `git -C <path> symbolic-ref -q HEAD` que da `refs/heads/nergal/<slug>`; si no, `remove_worktree` y error. El test de 1.1 ("checks out the existing branch at its SHA") tiene que comprobar el symbolic-ref, no solo el SHA.

2. **HIGH: el orden de locks puede colgar la app.** Afecta a D6, D4 pasos 2–3 y `delete_local_branch_core` ("takes the repo lock; under the guard reads the repo path").
   - El lock se indexa por el repo path canónico, pero ese path solo se obtiene leyendo la DB bajo el guard. Tal como está escrito no se puede implementar.
   - Si quien implementa toma el DB guard y espera el repo lock dentro de él, se cruza con "Open session": open tiene el repo lock y espera el DB guard en el paso 6, mientras delete tiene el DB guard y espera el repo lock. El resultado es un deadlock sobre el `SharedDb` global y se congela toda la app.

   Hay que fijar la secuencia: guard → leer el repo path → soltar → repo lock → guard → `branch_owners` → soltar → git. La regla explícita es "nunca adquirir el repo lock con el DB guard tomado", con un test de dos threads.

3. **HIGH: los worktrees borrados a mano dejan el branch bloqueado para siempre.** Afecta a la Why de `proposal.md` ("A worktree removed outside Nergal also leaves its branch behind") y a la spec `local-branches`. Si el usuario borró el directorio con `rm -rf`, git mantiene el registro: `worktree list --porcelain` sigue mostrando `[nergal/x] prunable` (reproducido). Entonces:
   - `branch_checked_out` devuelve true, la fila queda "in use" sin acciones, y el delete devuelve `InUse`.
   - `git worktree add` falla con `already checked out at '<missing path>'` (reproducido).
   - `reconcile_worktrees` (`lib.rs:1256`) limpia la DB pero nunca hace `prune`.

   Justo el caso que motiva el change queda sin salida, y ni siquiera hay un marcador que lo explique. Hace falta detectar `prunable` y, o bien mostrar un estado "stale worktree" con una acción de prune, o bien correr `git worktree prune` bajo el repo lock antes de open/delete (prune respeta los worktrees bloqueados). Con un scenario en la spec y un test.

4. **MEDIUM: la promesa de D6 es falsa.** D6 dice que el lock cubre "every Nergal path", pero ningún otro camino que toca estos branches toma `BRANCH_LOCKS`: `create_worktree`, llamado desde `commands/workspaces_sessions.rs:471`, `linear/mod.rs:538`, `clickup/mod.rs:917` y `mcp/worktree_sessions.rs:888`, ni `drop_session_branch`. Además, `create_worktree` reutiliza un directorio existente (`worktree.rs:586`) y se cuelga de un `nergal/<slug>` existente si `-b` falla (`:603`). Hay dos opciones: tomar el lock también en `create_worktree` y en `drop_session_branch`, o corregir la afirmación y documentar la ventana en Risks.

5. **MEDIUM: la spec se contradice con el flowchart.** El scenario "Delete waits for a running open" de `specs/local-branches/spec.md` espera "the in-use reason". Pero en el flowchart de D5 el check de sesión en la DB va antes, así que el resultado sería `Kept SessionUses`, nombrando la sesión. Además, ningún task prueba ese scenario: 3.1 solo verifica que el mutex sea el mismo para un path y su alias symlink.

6. **MEDIUM: dependencia circular en el frontend.** `addAndActivateSessionAtom` en `src/stores/workspace.ts` tiene que hacer set de `focusZoneAtom`, que vive en `stores/shortcuts.ts:63`. Y `shortcuts.ts` ya importa de `./workspace` (`:5`) y arrastra `terminalService` y `appStore`. Se crea el ciclo workspace↔shortcuts, y `workspace.test.ts` cargaría `terminalService` en Node. Mejor ponerlo en un módulo nuevo (por ejemplo `stores/sessionActivation.ts`).

7. **LOW: dos huecos de validación en git (security).**
   - **Pathspec:** `commit_file_diff` pasa `<rel>` como pathspec, así que `:(glob)**` o `:!x` funcionan como magic y devuelven otros archivos, aunque la spec dice "only a file path". Usar `--literal-pathspecs`.
   - **Hash:** un hex corto puede resolverse como refname (un branch llamado `beef`). Usar `<hash>^{commit}`.

   Aparte, `get_branch_commits` no valida `branch` con `nergal_slug`. No hay riesgo de inyección porque el prefijo `refs/heads/` lo evita, pero conviene validarlo igual.

8. **LOW: nombres de branch ambiguos.** En `nergal_branch_candidates`, `%(refname:short)` produce `heads/nergal/x` cuando existe un tag `nergal/x`. Esa fila tendría acciones que siempre fallan. Usar `%(refname)` y quitar el prefijo `refs/heads/`.

9. **LOW: workspaces que no son repos git.** `for-each-ref` falla y el chip mostraría un error. La spec no dice nada; debería devolver una lista vacía cuando `!is_git_repo`.

10. **LOW: comando de restore sin repo.** El toast `git branch <name> <sha>` no dice en qué repo correrlo; con varios workspaces abiertos es fácil hacerlo en el equivocado. Usar `git -C <repo> branch …`; el backend tiene el path.

11. **LOW: la delta de `git-panel-v2` reafirma un scenario falso.** El MODIFIED requirement repite como normativo "Active chip persists per workspace", y el propio `design.md` admite que no coincide con el código. Además, `get_commit_files` queda sin uso tras el cambio en `HistoryChip`, y el plan no dice si se borra.

**Rollback:** el refactor de `drop_branch` está cubierto por los tests con hash bloqueado. El cleanup cuando el insert falla está bien diseñado. Con el fix del punto 1, una falla parcial deja como mucho un worktree que el error nombra. Sin ese fix, se pueden perder commits.

VERDICT: REVISE



=== Round 2 (claude sonnet; opus weekly limit, codex auth expired) ===
**Round 2 review: `local-branches-chip`**

I checked the revised plan against the repo. All 11 round-1 findings are addressed, and I found no new critical or high defect. Four of the repo facts I checked:

- **Short branch name in `worktree add`:** `create_worktree`'s fallback at `worktree.rs:601-607` already passes the short `nergal/<slug>` name. Combined with the post-add `symbolic-ref` check, this closes the detached-HEAD data-loss risk.
- **Lock order:** guard, repo path, drop, repo lock, guard, owners, drop, git. This is implementable, and the two-thread test makes the "never wait for the repo lock under the DB guard" rule checkable.
- **Missing worktrees:** the `prunable` / `locked` handling, with a scoped `git worktree remove --force` on a path that no longer exists, resolves the permanently blocked branch.
- **`git-panel-v2` delta:** the REMOVED header, "Chip-strip navigation in the GitPanel", matches `openspec/specs/git-panel-v2/spec.md:150` exactly. The replacement requirement restores all three scenarios and corrects the per-session claim.

Remaining issues, none blocking:

1. **MEDIUM: the chip count only exists after the chip has been opened.**
   - Where: design D7, `tasks.md` 5.3 and 5.5.
   - Problem: the orphan count comes from `localBranchesCacheAtom`, but only `BranchesChip` loads that cache, and only on mount. D7 says the count makes the chip "draw attention when there is orphaned work". With the per-session default of `Files`, the badge stays at 0 until the user visits the chip. That defeats the discoverability purpose.
   - Fix: have `GitPanel` trigger one `list_local_branches` load when it mounts, or per workspace when the cache is empty. Alternatively, drop the claim and the count. This also affects the manual walk at 6.4, which says "the chip count is 1".

2. **LOW: `clear_missing_worktrees` runs before the SHA check, so a refused delete can still change the repo.**
   - Where: design D5 and `delete_local_branch_core` step 5.
   - Problem: the stale registration is removed before `branch_state`, `expected_sha` and the in-use re-check. A result of `Kept Moved`, `Kept InUse` or `Missing` can therefore leave the repo changed, though only by removing an entry that pointed at a path that no longer exists.
   - Fix: this is harmless, so just state it. Add a one-line note to the spec ("registrations are cleared even when the delete is refused"). Alternatively, move the cleanup to just before the first `branch_checked_out` check.

3. **LOW: Git version dependency for `diff-tree -m --first-parent`.**
   - Where: `commit_files` and `commit_file_diff`.
   - Problem: the plan verifies this on git 2.34.1 only. Older git may ignore `--first-parent` for `diff-tree`, so merge commits would show a combined or duplicated per-parent output.
   - Fix: note the minimum git version in the Risks section, or add a test that fails loudly if the output for a merge commit contains more than one parent's diff.

4. **LOW: the worktree list is parsed without `-z`.**
   - Where: `worktree_entries`.
   - Problem: `git worktree list --porcelain` cannot represent a path with a newline unambiguously. The failure direction is safe, because an unparsed path leaves the entry as an existing worktree that counts as "in use". It is worth one sentence in the design.

5. **LOW: no automated test for the `BranchesChip` UI wiring.**
   - Where: `tasks.md` 5.4.
   - Problem: Vitest has no DOM, so the chip itself is covered only by the manual walks at 6.4–6.10. The pure helpers (`branchActionsAllowed`, `deleteBranchConfirm`, `branchDeleteToast`) are tested.
   - Fix: acceptable, since the guards are enforced in the backend and the manual walks cover the chip.

6. **LOW: a refused or failed delete can leave a restore-record line for a branch that still exists.**
   - Where: design D5, write-ahead record followed by a CAS failure.
   - Problem: this is the same window `drop_session_branch` already accepts, but the new UI makes it user-visible.
   - Fix: make sure the `Kept Moved` message does not tell the user the branch was deleted, and document the leftover record line.

Rollback and failure handling are coherent. Every step after `worktree add` removes the worktree on failure, the delete CAS refuses a moved ref, and the record is written before the delete. The hash-locked tests (`worktree_branch_tests.rs`, `worktree_review_tests.rs`) guard the `drop_session_branch` refactor.

VERDICT: APPROVED
