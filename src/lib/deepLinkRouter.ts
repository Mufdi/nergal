import { invoke } from "@/lib/tauri";
import { confirm } from "@/lib/confirm";
import { escapeHtml } from "@/lib/escapeHtml";
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

interface WorkspaceProbe {
  is_dir: boolean;
  is_git_repo: boolean;
  resolved: string;
}

/// Pre-creation, side-effect-free git-repo check so the deep-link confirm can
/// warn before anything is created (see confirm-deep-link-session-spawn D3).
/// Fails toward showing the warning rather than assuming a trusted git repo.
async function probeWorkspacePath(path: string): Promise<WorkspaceProbe> {
  try {
    return await invoke<WorkspaceProbe>("probe_workspace_path", { path });
  } catch {
    return { is_dir: false, is_git_repo: false, resolved: path };
  }
}

/// Content-aware confirmation gate for a deep-link-initiated workspace/session
/// spawn (D5: the proceed action must not default to Enter — see
/// `enterConfirms` in `@/lib/confirm`). Returns true only on an explicit
/// pointer/keyed action on the proceed control.
async function confirmDeepLinkSpawn(opts: {
  title: string;
  resolvedPath: string;
  isGitRepo: boolean;
  isDir?: boolean;
  prompt?: string;
  confirmLabel: string;
}): Promise<boolean> {
  // A non-existent path (probe couldn't resolve it) reads is_dir=false; say so
  // rather than the misleading "Not a git repository" for a path that isn't
  // even there. Inlined into the confirm() call so the confirmBodyEscaping
  // scanner covers this attacker-facing sink (see that test's blind spot).
  const warning =
    opts.isDir === false
      ? `<div class="mt-1 text-amber-500">Path does not exist.</div>`
      : opts.isGitRepo
        ? ""
        : `<div class="mt-1 text-amber-500">Not a git repository.</div>`;
  return confirm({
    title: opts.title,
    body:
      `<div>Directory: <code>${escapeHtml(opts.resolvedPath)}</code></div>` +
      warning +
      (opts.prompt
        ? `<div class="mt-2"><div class="mb-1 font-medium">Prompt</div><div class="whitespace-pre-wrap">${escapeHtml(opts.prompt)}</div></div>`
        : ""),
    confirmLabel: opts.confirmLabel,
    cancelLabel: "Cancel",
    enterConfirms: false,
  });
}

export function dispatchDeepLink(rawUrl: string): void {
  let parsed: URL;
  try {
    parsed = new URL(rawUrl);
  } catch {
    console.warn("[deeplink] invalid url:", rawUrl);
    return;
  }
  if (parsed.protocol !== "nergal:") {
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
      description: "nergal://open-workspace requires ?path=",
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

  // Unknown path: registering a new workspace from an external link is a
  // state change worth confirming (D2).
  const probe = await probeWorkspacePath(path);
  const proceed = await confirmDeepLinkSpawn({
    title: "Open workspace from deep link?",
    resolvedPath: probe.resolved,
    isGitRepo: probe.is_git_repo,
    isDir: probe.is_dir,
    confirmLabel: "Open workspace",
  });
  if (!proceed) {
    appStore.set(toastsAtom, {
      type: "info",
      message: "Deep link cancelled",
      description: `nergal://open-workspace — ${path}`,
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
      description: `nergal://session/${sub || "?"} is not a known action`,
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
      description: "nergal://session/new requires ?cwd=",
    });
    return;
  }

  const promptText = prompt ?? "";
  // Always gate: session/new always spawns a new session (and, when a prompt
  // is present, auto-submits it on spawn) regardless of whether the workspace
  // is already known.
  const probe = await probeWorkspacePath(cwd);
  const proceed = await confirmDeepLinkSpawn({
    title: "Start session from deep link?",
    resolvedPath: probe.resolved,
    isGitRepo: probe.is_git_repo,
    isDir: probe.is_dir,
    prompt: promptText || undefined,
    confirmLabel: "Start session",
  });
  if (!proceed) {
    appStore.set(toastsAtom, {
      type: "info",
      message: "Deep link cancelled",
      description: `nergal://session/new — ${probe.resolved}`,
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
      description: "nergal://open-file requires ?path=",
    });
    return;
  }
  const parsedLine = lineRaw ? Number.parseInt(lineRaw, 10) : null;
  const line =
    parsedLine != null && Number.isFinite(parsedLine) && parsedLine >= 1 ? parsedLine : undefined;

  // A file tab is always bound to a session, so a file from a project that
  // isn't a workspace yet needs both created before the tab can attach.
  // `confirmed` tracks whether the spawn was already gated by the
  // workspace-creation branch, so a known workspace that simply has no session
  // yet still gets gated below rather than spawning an agent silently.
  let workspace = appStore.get(workspacesAtom).find((w) => ownsPath(w, path));
  let confirmed = false;
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
      // Unknown path: this branch creates the workspace AND (since a brand
      // new workspace has no sessions) forces the session-creation + spawn
      // below — gate once, here, before either happens (D2).
      const proceed = await confirmDeepLinkSpawn({
        title: "Open file from deep link?",
        resolvedPath: root,
        // resolve_repo_root only returns Some when it found a `.git` dir
        // walking up from `root`, so this branch is always a git repo.
        isGitRepo: true,
        confirmLabel: "Open workspace",
      });
      if (!proceed) {
        appStore.set(toastsAtom, {
          type: "info",
          message: "Deep link cancelled",
          description: `nergal://open-file — ${root}`,
        });
        return;
      }
      confirmed = true;
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
      // A known workspace with no session yet still needs the spawn gated —
      // creating + activating a session below starts the agent's PTY. Skip
      // the re-confirm only when the workspace-creation branch above already
      // gated this same call.
      if (!confirmed) {
        const probe = await probeWorkspacePath(ws.repo_path);
        const proceed = await confirmDeepLinkSpawn({
          title: "Start a session from deep link?",
          resolvedPath: probe.resolved,
          isGitRepo: probe.is_git_repo,
          isDir: probe.is_dir,
          confirmLabel: "Start session",
        });
        if (!proceed) {
          appStore.set(toastsAtom, {
            type: "info",
            message: "Deep link cancelled",
            description: `nergal://open-file — ${ws.repo_path}`,
          });
          return;
        }
      }
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
