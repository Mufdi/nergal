# OpenCode SSE event schema (captured from `opencode serve` v1.2.15)

Discovery method: ran `opencode serve --port 14096`, fetched `GET /doc`
(an OpenAPI document), enumerated `Event.*` variants and key payload
schemas. Recorded 2026-05-04. **Refresh whenever the OpenCode binary's
major version changes** — OpenCode publishes the OpenAPI spec at
`/doc` so this is reproducible.

## Server endpoints we consume

| Method | Path                                                       | Purpose                                  |
|--------|------------------------------------------------------------|------------------------------------------|
| GET    | `/event`                                                   | SSE stream of `Event` records.           |
| POST   | `/session/{sessionID}/permissions/{permissionID}`          | Submit `permission.replied` (once / always / reject). |
| GET    | `/session`                                                  | List sessions.                           |
| POST   | `/session`                                                  | Create a new session.                    |
| POST   | `/session/{sessionID}/prompt_async`                         | Submit a user prompt asynchronously.     |
| GET    | `/session/{sessionID}/message`                              | List messages for a session.             |

## Event wire shape

Each SSE event is JSON of shape:

```json
{ "type": "<event-name>", "properties": <payload> }
```

`<payload>` is variant-specific. The list of `<event-name>`s we currently
care about:

- `permission.asked` — agent requests a tool/file permission.
  Payload: [`PermissionRequest`](#permissionrequest).
- `permission.replied` — confirmation that a reply was applied.
  Payload: `{ sessionID, requestID, reply: "once" | "always" | "reject" }`.
- `session.idle` — agent is idle (analog to CC's `Stop`).
  Payload: `{ sessionID }`.
- `message.updated` — full message changed (created or completed).
  Payload: `{ info: Message }` where `Message = UserMessage | AssistantMessage`.
- `message.part.updated` — incremental message-part change.
  Payload: `{ part: Part }`. Part variants include `text`, `tool`, etc.
- `todo.updated` — todo list changed.
  Payload: `{ sessionID, todos: Todo[] }`.

Other event variants emitted but currently ignored: `server.connected`,
`server.instance.disposed`, `global.disposed`, `tui.*` (TUI-specific —
we use chat panel, not TUI), `installation.*`, `project.updated`,
`worktree.*`, `lsp.*`, `pty.*`, `file.*`, `mcp.*`, `vcs.branch.updated`,
`command.executed`, `question.*`, `session.status`.

## PermissionRequest

```jsonc
{
  "id": "per_…",          // permission id (used in REST POST URL)
  "sessionID": "ses_…",   // OpenCode session id
  "permission": "...",    // human-readable permission text
  "patterns": ["..."],    // patterns the permission grants
  "metadata": { ... },    // arbitrary metadata from the agent
  "always": ["..."],      // patterns the user can grant always
  "tool": {               // optional — present when triggered by a tool call
    "messageID": "...",
    "callID": "..."
  }
}
```

The reply (`POST /session/{sessionID}/permissions/{id}`) takes a body with:

```jsonc
{ "reply": "once" | "always" | "reject" }
```

## AssistantMessage cost / tokens

`AssistantMessage` carries:

```jsonc
{
  "id": "msg_…",
  "sessionID": "ses_…",
  "role": "assistant",
  "modelID": "...",       // e.g. "claude-sonnet-4"
  "providerID": "...",    // e.g. "anthropic"
  "cost": 0.0034,         // already in USD!
  "tokens": {
    "input":  123,
    "output": 456,
    "reasoning": 0,
    "cache": { "read": 12, "write": 3 },
    "total": 591
  },
  "time": { "created": 1714857000, "completed": 1714857010 }
}
```

So OpenCode resolves USD server-side; the adapter forwards it directly
without needing the cluihud `pricing` module.

## Capability mapping (this informs `OpenCodeAdapter::capabilities()`)

| Capability                  | Supported | Source                                        |
|-----------------------------|-----------|-----------------------------------------------|
| `PLAN_REVIEW`               | ❌        | OpenCode has no plan-mode equivalent.          |
| `ASK_USER_BLOCKING`         | ✅        | `permission.asked` + REST POST.                |
| `TOOL_CALL_EVENTS`          | ✅        | `message.part.updated` with `Part.tool`.       |
| `STRUCTURED_TRANSCRIPT`     | ✅        | `message.updated` carries full Message.        |
| `RAW_COST_PER_MESSAGE`      | ✅        | `AssistantMessage.tokens` + `.cost`.           |
| `TASK_LIST`                 | ✅        | `todo.updated` event.                          |
| `SESSION_RESUME`            | ✅        | `GET /session/{id}` + `POST /session/{id}/prompt_async`. |
| `ANNOTATIONS_INJECT`        | ❌        | No `UserPromptSubmit` analog.                  |
