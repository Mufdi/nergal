# REVIEW — fix-pty-double-spawn

## Reviewer: spec-reviewer (single-sequential, sonnet) · 2026-07-03/04 · 2 rounds

### Round 1: PASS con 1 MEDIUM a decidir

- Core race cerrada correctamente: `reserve_session_pty` check-and-reserve bajo un solo
  lock hold; interleavings recorridos; rollback en spawn-failure verificado con test.
- Todos los consumidores de `session_ptys` degradan con `.ok_or(...)`/no-op ante un
  instance faltante (18 sitios greppeados) — sin panics en la ventana de boot.
- **MEDIUM (pregunta explícita del orquestador, confirmada por el reviewer como
  regresión real)**: kill-during-spawn — la reserva pre-spawn hace que un
  `kill_session_pty` mid-spawn borre la reserva sin encontrar el instance; el instance
  que spawn_pty inserta después quedaba huérfano (corriendo hasta shutdown_all), PEOR
  que el pre-change (que se auto-sanaba con el insert tardío). Recomendación: fixear
  en este change.

### Fix (builder, round 2)

`confirm_reservation_or_extract` post-spawn-Ok: reserva intacta → sigue; desaparecida →
extrae el instance bajo su lock, el caller lo dropea FUERA de locks (kill_tree + reap)
y retorna `Err("session closed during PTY startup")`. Test end-to-end con PtyInstance
REAL (`build_test_pty_instance`: PTY + `sh -c "sleep 5"` vivos, sin AppHandle fakery —
el reviewer verificó en emitter.rs que `TerminalHandle::new` sin emitter no deja nada
colgando) + `assert_reaped` reutilizado. Reviewer corrió el módulo pty 3× sin flakes.

### Round 2: PASS (ventana cerrada, 3 puntos de llegada del kill recorridos) + 1
hallazgo adyacente: el check "intact" confundía "extraído por mí" con "ya matado
legítimamente post-insert" — ese caso devolvía false-success (`Ok` con pty_id muerto,
boot sequence corriendo contra un instance inexistente).

### Orchestrator post-PASS fix (remediación opción-b del reviewer)

`ReservationOutcome { Intact, Gone(Option<PtyInstance>) }`: CUALQUIER reserva
no-intacta ⇒ `Err`, dropeando lo que devuelva el remove (`Gone(None)` = el kill path
ya lo reapeó). Tercer test: `confirm_reservation_reports_gone_even_when_kill_already_extracted`.

## Gates

- Gate 1-3: PASS post-fix (clippy clean, 742 tests — 5 nuevos netos en el change —,
  fmt clean).
- Gate 6 (scope): 1 file = files_estimate 1.
