import { invoke } from "@/lib/tauri";
import { appStore } from "@/stores/jotaiStore";
import { toastsAtom } from "@/stores/toast";
import { activeSessionIdAtom, workspacesAtom, type Workspace } from "@/stores/workspace";

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
      handleSessionRoute(parsed);
      break;
    case "open-file":
      handleOpenFile(parsed.searchParams.get("path"), parsed.searchParams.get("line"));
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
    const firstSession = match.sessions[0];
    if (firstSession) {
      appStore.set(activeSessionIdAtom, firstSession.id);
      appStore.set(toastsAtom, {
        type: "success",
        message: `Switched to ${match.name}`,
      });
    } else {
      appStore.set(toastsAtom, {
        type: "info",
        message: match.name,
        description: "Open the sidebar and start a session in this workspace.",
      });
    }
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

function handleSessionRoute(parsed: URL): void {
  const sub = parsed.pathname.replace(/^\//, "").split("/")[0];
  appStore.set(toastsAtom, {
    type: "info",
    message: "Deep link received",
    description: `cluihud://session/${sub || "?"} handler not yet implemented`,
  });
}

function handleOpenFile(path: string | null, line: string | null): void {
  const tail = path ? `${path}${line ? `:${line}` : ""}` : "?";
  appStore.set(toastsAtom, {
    type: "info",
    message: "Deep link received",
    description: `cluihud://open-file (${tail}) handler not yet implemented`,
  });
}
