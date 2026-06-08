# Tasks — quake shell cwd

- [x] 1. Backend: retain the shell child pid in `PtyInstance`; read `/proc/<pid>/cwd` at Enter (`cfg(target_os = "linux")` helper); `shell:command` payload gains `cwd`. `EnvShellDef.cwd` (serde default, no migration). `spawn_aux_shell` takes a per-shell cwd and resolves it (~ expands, relative → against workspace root, missing → fallback).
- [x] 2. Frontend: `QuakeShell.cwd` + capture comparison vs session cwd in the `shell:command` listener; spawn paths (`showShell`, `spawnEnvShells`) pass it; persist it.
- [x] 3. UI: optional cwd input on env-shell rows (modal, keyboard chain label↔cwd↔command↔✕, live validation that blocks creation on a missing dir) and on the suggestions library rows; quick-pick carries cwd.
- [x] 4. Verify: full check green (clippy, 322 tests, fmt, tsc); manual walk pending user validation.
