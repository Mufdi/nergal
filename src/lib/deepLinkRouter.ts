import { invoke } from "@/lib/tauri";
import { appStore } from "@/stores/jotaiStore";
import { toastsAtom } from "@/stores/toast";
import { openTabAction } from "@/stores/rightPanel";
import {
  activeSessionIdAtom,
  activeSessionAtom,
  expandedWorkspaceIdsAtom,
  freshSessionsAtom,
  workspacesAtom,
  type Session,
  type Workspace,
} from "@/stores/workspace";

/// Expand the workspace in the sidebar so a deep-link-spawned session lands
/// visible (and the active highlight is on screen), mirroring what creating a
/// session from the sidebar does.
function expandWorkspace(wsId: string): void {
  appStore.set(expandedWorkspaceIdsAtom, (prev) => new Set([...(prev ?? []), wsId]));
}

export function dispatchDeepLink(rawUrl: string): void {
  let parsed: URL;
  try {
    parsed = new URL(rawUrl);
  } catch {
    console.warn("[deeplink] invalid url:", rawUrl);
    return;
  }
  if (parsed.protocol !== "cluihud:") {
    console.warn("[deeplink] unknown protocol:", parsed.protocol);
    return;
  }

  const action = parsed.hostname || parsed.pathname.replace(/^\//, "").split("/")[0] || "";

  switch (action) {
    case "open-workspace":
      void handleOpenWorkspace(parsed.searchParams.get("path"));
      break;
    case "session":
      void handleSessionRoute(parsed);
      break;
    case "open-file":
      void handleOpenFile(parsed.searchParams.get("path"), parsed.searchParams.get("line"));
      break;
    default:
      appStore.set(toastsAtom, {
        type: "info",
        message: "Unknown deep link action",
        description: action || rawUrl,
      });
  }
}

async function handleOpenWorkspace(path: string | null): Promise<void> {
  if (!path) {
    appStore.set(toastsAtom, {
      type: "error",
      message: "Deep link missing path",
      description: "cluihud://open-workspace requires ?path=",
    });
    return;
  }
  const workspaces = appStore.get(workspacesAtom);
  const match = workspaces.find((w) => w.repo_path === path);
  if (match) {
    if (match.sessions.length === 0) {
      appStore.set(toastsAtom, {
        type: "info",
        message: match.name,
        description: "Open the sidebar and start a session in this workspace.",
      });
      return;
    }
    const active = appStore.get(activeSessionAtom);
    if (active && active.workspace_id === match.id) {
      appStore.set(toastsAtom, {
        type: "info",
        message: `Already on ${match.name}`,
        description: active.name,
      });
      return;
    }
    // DB returns sessions ASC by created_at; land on the one the user was
    // last touching, not the oldest.
    const target = [...match.sessions].sort((a, b) => b.updated_at - a.updated_at)[0];
    appStore.set(activeSessionIdAtom, target.id);
    appStore.set(toastsAtom, {
      type: "success",
      message: `Switched to ${match.name}`,
      description: target.name,
    });
    return;
  }
  try {
    const ws = await invoke<Workspace>("create_workspace", { repoPath: path });
    appStore.set(workspacesAtom, (prev) => [...prev, ws]);
    appStore.set(toastsAtom, {
      type: "success",
      message: `Workspace added: ${ws.name}`,
      description: path,
    });
  } catch (err) {
    const message = typeof err === "string" ? err : String(err);
    appStore.set(toastsAtom, {
      type: "error",
      message: "Failed to open workspace",
      description: `${path} — ${message}`,
    });
  }
}

async function handleSessionRoute(parsed: URL): Promise<void> {
  const sub = parsed.pathname.replace(/^\//, "").split("/")[0];
  if (sub !== "new") {
    appStore.set(toastsAtom, {
      type: "info",
      message: "Deep link received",
      description: `cluihud://session/${sub || "?"} is not a known action`,
    });
    return;
  }
  await handleSessionNew(parsed.searchParams.get("cwd"), parsed.searchParams.get("prompt"));
}

/// Honor the user's configured default agent for deep-link sessions instead of
/// always launching CC. null falls back to the backend's own CC default.
async function resolveAgentId(repoPath: string): Promise<string | null> {
  try {
    return await invoke<string>("resolve_default_agent", { projectPath: repoPath });
  } catch {
    return null;
  }
}

/// Derive a short session name from the prompt's first line, so the sidebar row
/// is recognizable instead of a generic placeholder.
function sessionNameFromPrompt(prompt: string): string {
  const firstLine = prompt.split("\n")[0]?.trim() ?? "";
  if (!firstLine) return "New session";
  return firstLine.length > 40 ? `${firstLine.slice(0, 40)}…` : firstLine;
}

async function handleSessionNew(cwd: string | null, prompt: string | null): Promise<void> {
  if (!cwd) {
    appStore.set(toastsAtom, {
      type: "error",
      message: "Deep link missing cwd",
      description: "cluihud://session/new requires ?cwd=",
    });
    return;
  }
  let workspace = appStore.get(workspacesAtom).find((w) => w.repo_path === cwd);
  if (!workspace) {
    try {
      workspace = await invoke<Workspace>("create_workspace", { repoPath: cwd });
      const created = workspace;
      appStore.set(workspacesAtom, (prev) => [...prev, created]);
    } catch (err) {
      appStore.set(toastsAtom, {
        type: "error",
        message: "Failed to open workspace",
        description: `${cwd} — ${typeof err === "string" ? err : String(err)}`,
      });
      return;
    }
  }
  const ws = workspace;
  const promptText = prompt ?? "";
  try {
    const session = await invoke<Session>("create_session", {
      workspaceId: ws.id,
      name: sessionNameFromPrompt(promptText),
      agentId: await resolveAgentId(ws.repo_path),
    });
    appStore.set(workspacesAtom, (prev) =>
      prev.map((w) => (w.id === ws.id ? { ...w, sessions: [...w.sessions, session] } : w)),
    );
    appStore.set(freshSessionsAtom, (prev) => new Set([...prev, session.id]));
    // Stash before activating: activation triggers the PTY spawn that consumes
    // the prompt, so it must already be queued when start_claude_session runs.
    if (promptText) {
      await invoke("queue_session_prompt", { sessionId: session.id, prompt: promptText });
    }
    expandWorkspace(ws.id);
    appStore.set(activeSessionIdAtom, session.id);
    appStore.set(toastsAtom, {
      type: "success",
      message: `New session: ${ws.name}`,
      description: session.name,
    });
  } catch (err) {
    appStore.set(toastsAtom, {
      type: "error",
      message: "Failed to create session",
      description: typeof err === "string" ? err : String(err),
    });
  }
}

function ownsPath(ws: Workspace, path: string): boolean {
  return path === ws.repo_path || path.startsWith(`${ws.repo_path}/`);
}

async function handleOpenFile(path: string | null, lineRaw: string | null): Promise<void> {
  if (!path) {
    appStore.set(toastsAtom, {
      type: "error",
      message: "Deep link missing path",
      description: "cluihud://open-file requires ?path=",
    });
    return;
  }
  const parsedLine = lineRaw ? Number.parseInt(lineRaw, 10) : null;
  const line =
    parsedLine != null && Number.isFinite(parsedLine) && parsedLine >= 1 ? parsedLine : undefined;

  // A file tab is always bound to a session, so a file from a project that
  // isn't a workspace yet needs both created before the tab can attach.
  let workspace = appStore.get(workspacesAtom).find((w) => ownsPath(w, path));
  if (!workspace) {
    let root: string | null;
    try {
      root = await invoke<string | null>("resolve_repo_root", { path });
    } catch {
      root = null;
    }
    if (!root) {
      appStore.set(toastsAtom, {
        type: "info",
        message: "File is not inside a git project",
        description: "Open it in your editor, or add the project as a workspace first.",
      });
      return;
    }
    workspace = appStore.get(workspacesAtom).find((w) => w.repo_path === root);
    if (!workspace) {
      try {
        const created = await invoke<Workspace>("create_workspace", { repoPath: root });
        appStore.set(workspacesAtom, (prev) => [...prev, created]);
        workspace = created;
      } catch (err) {
        appStore.set(toastsAtom, {
          type: "error",
          message: "Failed to open workspace",
          description: `${root} — ${typeof err === "string" ? err : String(err)}`,
        });
        return;
      }
    }
  }
  const ws = workspace;

  let sessionId = appStore.get(activeSessionIdAtom);
  const activeBelongs = sessionId != null && ws.sessions.some((s) => s.id === sessionId);
  if (!activeBelongs) {
    const recent =
      ws.sessions.length > 0
        ? [...ws.sessions].sort((a, b) => b.updated_at - a.updated_at)[0]
        : null;
    if (recent) {
      sessionId = recent.id;
    } else {
      try {
        const session = await invoke<Session>("create_session", {
          workspaceId: ws.id,
          name: "New session",
          agentId: await resolveAgentId(ws.repo_path),
        });
        appStore.set(workspacesAtom, (prev) =>
          prev.map((w) => (w.id === ws.id ? { ...w, sessions: [...w.sessions, session] } : w)),
        );
        appStore.set(freshSessionsAtom, (prev) => new Set([...prev, session.id]));
        sessionId = session.id;
      } catch (err) {
        appStore.set(toastsAtom, {
          type: "error",
          message: "Failed to create session",
          description: typeof err === "string" ? err : String(err),
        });
        return;
      }
    }
    // Activating the session spawns its PTY (empty session starts the agent).
    expandWorkspace(ws.id);
    appStore.set(activeSessionIdAtom, sessionId);
  }

  const name = path.split("/").pop() ?? path;
  appStore.set(openTabAction, {
    tab: { id: `file:${path}`, type: "file", label: name, data: { path, sessionId, line } },
  });
  appStore.set(toastsAtom, {
    type: "success",
    message: "Opened file",
    description: line ? `${name}:${line}` : name,
  });
}
