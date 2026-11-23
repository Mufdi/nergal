Round 2 changes (addressing every round-1 item):
1. Option injection: new requirement "Launch prompt is never read as an agent option". Claude Code and Codex render ` -- <ref>`; OpenCode renders ` --prompt=<ref>` as one word. Verified on the real binaries 2026-11-23 (claude 2.1.330 with -p and with --resume before --, codex 0.142, opencode 2.2.7). Design D2.
2. Frozen adapters: USER DECISION 2026-11-23 - Codex and OpenCode get the same fix (SpawnSpec.prompt with PromptAttach). Pi takes no prompt.
3. Fail-closed cleanup: new requirement "A failed launch leaves no half-started session" + design D3 + `abort_launch` (remove reservation if owned, mark_dead_if, remove instance and drop off main thread, requeue prompt with entry().or_insert, delete prompt file; aux shells untouched). Applied also to the existing adapter.spawn error path. Retry scenario added.
4. Missing file: typed line now guards with `test -r '<file>' &&` before the binary; scenario "Missing prompt file skips the agent".
5. Confidentiality claim corrected: Non-Goal states /proc/<pid>/cmdline exposure is unchanged; goal limited to 0600 file between write and read.
6. Tests use explicit-dir functions (`clear_launch_prompts_in`, `write_launch_prompt_file(dir, …)`) on tempfile dirs; never the real ipc dir.
7. Shell test: `--noprofile --rcfile <tmp rc> -i`, HISTFILE=/dev/null, single PTY write, sentinel written by a fake agent, bounded poll; zsh variant skipped in CI (stated in Risks).
8. Unwritable dir: parent path is a regular file.
9. Stem validation in write_launch_prompt_file (reject empty, '/', '..', quote, whitespace, control chars).
10. D1(a) rejection reworded (variable persists in the tab and is inherited).
11. D4: tmpfs vs on-disk fallback acknowledged; kill_session_pty_inner now deletes the PTY's prompt file.
12. Manual walks added: option-like resume message (2.4) and create_worktree_session with agent codex (2.6).
