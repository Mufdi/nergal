Round 5 changes (addressing every round-4 item):
1. Gone path keeps the guard armed (idempotent rollback: reservation not owned, id-guarded mark_dead_if, instance already extracted, no file, no prompt). Never disarm there. D3, implementation and the task-1.1 test bullet updated (Gone-style rollback test).
2. Lock order fixed explicitly: `session_ptys` taken first, `instances` nested inside (existing order of kill_session_aux_shells, pty.rs:1342-1343), both held through write_all + flush; writer None when the reservation is not this pty_id or the instance is gone; never take session_ptys inside instances. This closes the kill window: the accepted-risk line was removed.
3. Stem validation moved into `launch_prompt_file_path` (io::Result), shared by the writer and both deletes; test bullet added.
4. Risk added: shell exiting on its own during the startup wait leaves a harmless id in launched_ptys.
5. Walk 2.7 compares `pgrep -P <pid> -a` lists after ~1 s instead of a count.
