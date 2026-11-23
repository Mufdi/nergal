Linear: FRI-602

## Why

Nergal types the launch prompt into the user's interactive shell as a quoted argument. The quoting is correct, but the shell's line editor can still rewrite the text. With Space bound to `magic-space`, zsh and bash expand history on every space typed. On a continuation line the editor does not see the open quote from the line above. A `!` in a multi-line prompt then pulls in old history text, breaks the quote and makes the shell parse the rest.

The FRI-601 walk hit this with `<!--` on line 3 of a relayed message: `zsh: parse error near ()`. The same path carries text the user did not write: agent messages from `request_session_resume` and `create_worktree_session`, deep-link prompts, and Linear or ClickUp issue bodies. A crafted `!#` can break the quote on purpose and run commands in the user's shell. A prompt that starts with `-` has a second path: the agent reads it as an option, and `--settings={…}` can register a hook command. This is a security fix, not only a delivery bug.

## What Changes

- The launch prompt no longer travels inside the typed command line. Nergal writes it to a private file, and the typed line only reads that file.
- On POSIX shells the typed line references the file through a double-quoted command substitution. The shell expands it without parsing the prompt text, and deletes the file as it reads it.
- The prompt always reaches the agent as text: `--` precedes it for Claude Code and Codex, and OpenCode gets `--prompt=<value>`.
- `SpawnSpec` gets a dedicated field for the launch prompt, separate from `args`. The Claude Code, Codex and OpenCode adapters move their prompt to that field. User decision 2026-11-23: the frozen Codex and OpenCode adapters get this fix because an MCP caller can choose them.
- A launch that fails after the shell started now rolls back: no reservation, no shell, no live mark, and the prompt goes back to the queue. When the prompt file cannot be written, the launch fails; it never falls back to typing the prompt.
- Startup and PTY kill delete leftover prompt files.
- Out of scope: the frozen Windows shells (PowerShell, cmd) keep the inline argument.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-adapter`: adds three requirements. The launch prompt reaches the agent without shell parsing, is never read as an agent option, and a failed launch leaves no half-started session.

## Impact

- `src-tauri/src/agents/mod.rs`: `SpawnSpec` gains the prompt field; prompt-file helpers and the startup sweep.
- `src-tauri/src/agents/{claude_code,codex,opencode}/adapter.rs`: the prompt moves from `args` to the new field. `pi/adapter.rs`: struct update only.
- `src-tauri/src/pty.rs`: `build_launch_command` renders the prompt reference per shell family; `start_claude_session` writes the private file and rolls back failed launches; `kill_session_pty_inner` deletes the PTY's prompt file.
- `src-tauri/src/lib.rs`: the startup sweep call.
- No schema change, no new dependency, no frontend change.
