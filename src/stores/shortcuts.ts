import { atom } from "jotai";
import { appStore } from "./jotaiStore";
import { activeSessionIdAtom, activeWorkspaceAtom, workspacesAtom, freshSessionsAtom } from "./workspace";
import * as terminalService from "@/components/terminal/terminalService";
import {
  activeTabsAtom,
  activeTabAtom,
  activeTabIdAtom,
  closeTabAction,
  reopenTabAction,
  expandRightPanelAtom,
  activePanelViewAtom,
  tabStateMapAtom,
  type Tab,
} from "./rightPanel";

export type FocusZone = "sidebar" | "terminal" | "panel";

export interface ShortcutAction {
  id: string;
  label: string;
  keys: string;
  category: "navigation" | "session" | "panel" | "action";
  keywords: string[];
  handler: () => void;
}

export { type Tab };

export const focusZoneAtom = atom<FocusZone>("terminal");
export const previousNonTerminalZoneAtom = atom<FocusZone>("panel");
export const commandPaletteOpenAtom = atom(false);
export const closedTabsStackAtom = atom<Tab[]>([]);

export const toggleSidebarAtom = atom(0);
export const toggleRightPanelAtom = atom(0);
export const toggleActivityLogAtom = atom(0);
export const triggerNewSessionAtom = atom(0);
export const triggerMergeAtom = atom(0);
export const triggerCommitAtom = atom(0);
export const triggerResumeSessionAtom = atom<string | null>(null);

function store() {
  return appStore;
}

function switchToSession(index: number) {
  const s = store();
  let workspace = s.get(activeWorkspaceAtom);
  if (!workspace) {
    const workspaces = s.get(workspacesAtom);
    workspace = workspaces.find((w) => w.sessions.length > 0) ?? null;
  }
  if (!workspace) return;
  const sessions = workspace.sessions.filter((ses) => ses.status !== "completed");
  if (index >= sessions.length) return;
  const session = sessions[index];
  const fresh = s.get(freshSessionsAtom);
  if (!fresh.has(session.id) && !terminalService.hasTerminal(session.id)) {
    s.set(triggerResumeSessionAtom, session.id);
    return;
  }
  s.set(activeSessionIdAtom, session.id);
}

function togglePanel(type: Tab["type"], _label: string) {
  const s = store();
  const currentView = s.get(activePanelViewAtom);
  const currentActiveTab = s.get(activeTabAtom);

  if (currentActiveTab?.type === type || (currentView === type && !currentActiveTab)) {
    s.set(toggleRightPanelAtom, (p: number) => p + 1);
    return;
  }

  // Try to restore last open tab of this type
  const tabs = s.get(activeTabsAtom);
  const lastOfType = [...tabs].reverse().find((t: Tab) => t.type === type);
  if (lastOfType) {
    s.set(activeTabIdAtom, lastOfType.id);
  } else {
    const sessionId = s.get(activeSessionIdAtom);
    if (sessionId) {
      s.set(tabStateMapAtom, (prev) => {
        const state = prev[sessionId] ?? { tabs: [], activeTabId: null, previewTabId: null };
        return { ...prev, [sessionId]: { ...state, activeTabId: null } };
      });
    }
  }
  s.set(activePanelViewAtom, type);
  s.set(expandRightPanelAtom, (prev: number) => prev + 1);
}

function focusZone(zone: FocusZone) {
  if (zone === "terminal") {
    terminalService.focusActive();
  } else if (zone === "sidebar") {
    const el = document.querySelector("[data-focus-zone='sidebar']") as HTMLElement | null;
    el?.focus();
  } else {
    const el = document.querySelector("[data-focus-zone='panel']") as HTMLElement | null;
    el?.focus();
  }
}

function nextTab() {
  const s = store();
  const tabs = s.get(activeTabsAtom);
  const activeId = s.get(activeTabIdAtom);
  if (tabs.length === 0) return;
  const idx = tabs.findIndex((t) => t.id === activeId);
  const next = (idx + 1) % tabs.length;
  s.set(activeTabIdAtom, tabs[next].id);
}

function prevTab() {
  const s = store();
  const tabs = s.get(activeTabsAtom);
  const activeId = s.get(activeTabIdAtom);
  if (tabs.length === 0) return;
  const idx = tabs.findIndex((t) => t.id === activeId);
  const prev = idx <= 0 ? tabs.length - 1 : idx - 1;
  s.set(activeTabIdAtom, tabs[prev].id);
}

function closeCurrentTab() {
  const s = store();
  const activeId = s.get(activeTabIdAtom);
  if (activeId) s.set(closeTabAction, activeId);
}

function reopenLastTab() {
  store().set(reopenTabAction);
}

export const shortcutRegistryAtom = atom<ShortcutAction[]>([
  // -- Navigation --
  { id: "toggle-sidebar", label: "Toggle Sidebar", keys: "ctrl+b", category: "navigation", keywords: ["sidebar", "left", "panel"], handler: () => store().set(toggleSidebarAtom, (p: number) => p + 1) },
  { id: "toggle-right-panel", label: "Toggle Right Panel", keys: "ctrl+shift+b", category: "navigation", keywords: ["right", "panel"], handler: () => store().set(toggleRightPanelAtom, (p: number) => p + 1) },
  { id: "focus-left", label: "Focus Left", keys: "alt+arrowleft", category: "navigation", keywords: ["move", "focus", "left"], handler: () => {
    const zones: FocusZone[] = ["sidebar", "terminal", "panel"];
    const active = document.activeElement;
    let current = 1;
    if (active?.closest("[data-focus-zone='sidebar']")) current = 0;
    else if (active?.closest("[data-focus-zone='panel']")) current = 2;
    else if (active?.closest(".xterm")) current = 1;
    const prev = current <= 0 ? zones.length - 1 : current - 1;
    focusZone(zones[prev]);
  }},
  { id: "focus-right", label: "Focus Right", keys: "alt+arrowright", category: "navigation", keywords: ["move", "focus", "right"], handler: () => {
    const zones: FocusZone[] = ["sidebar", "terminal", "panel"];
    const active = document.activeElement;
    let current = 1;
    if (active?.closest("[data-focus-zone='sidebar']")) current = 0;
    else if (active?.closest("[data-focus-zone='panel']")) current = 2;
    else if (active?.closest(".xterm")) current = 1;
    const next = (current + 1) % zones.length;
    focusZone(zones[next]);
  }},
  { id: "focus-terminal", label: "Focus Terminal", keys: "ctrl+ñ", category: "navigation", keywords: ["terminal", "pty", "cli"], handler: () => focusZone("terminal") },

  // -- Session --
  { id: "session-1", label: "Switch to Session 1", keys: "ctrl+1", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(0) },
  { id: "session-2", label: "Switch to Session 2", keys: "ctrl+2", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(1) },
  { id: "session-3", label: "Switch to Session 3", keys: "ctrl+3", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(2) },
  { id: "session-4", label: "Switch to Session 4", keys: "ctrl+4", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(3) },
  { id: "session-5", label: "Switch to Session 5", keys: "ctrl+5", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(4) },
  { id: "session-6", label: "Switch to Session 6", keys: "ctrl+6", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(5) },
  { id: "session-7", label: "Switch to Session 7", keys: "ctrl+7", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(6) },
  { id: "session-8", label: "Switch to Session 8", keys: "ctrl+8", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(7) },
  { id: "session-9", label: "Switch to Session 9", keys: "ctrl+9", category: "session", keywords: ["session", "switch"], handler: () => switchToSession(8) },
  { id: "new-session", label: "New Session", keys: "ctrl+n", category: "session", keywords: ["create", "session", "new"], handler: () => store().set(triggerNewSessionAtom, (p: number) => p + 1) },
  { id: "add-workspace", label: "Add Workspace", keys: "ctrl+shift+n", category: "session", keywords: ["workspace", "add", "folder"], handler: () => {} },

  // -- Panel (tabs) --
  { id: "next-tab", label: "Next Tab", keys: "ctrl+tab", category: "panel", keywords: ["tab", "next"], handler: nextTab },
  { id: "prev-tab", label: "Previous Tab", keys: "ctrl+shift+tab", category: "panel", keywords: ["tab", "previous", "prev"], handler: prevTab },
  { id: "close-tab", label: "Close Tab", keys: "ctrl+w", category: "panel", keywords: ["tab", "close"], handler: closeCurrentTab },
  { id: "reopen-tab", label: "Reopen Closed Tab", keys: "ctrl+shift+t", category: "panel", keywords: ["tab", "reopen", "undo"], handler: reopenLastTab },
  { id: "open-plan", label: "Open Plan Panel", keys: "ctrl+shift+p", category: "panel", keywords: ["plan", "panel"], handler: () => togglePanel("plan", "Plan") },
  { id: "open-files", label: "Open Files Panel", keys: "ctrl+shift+f", category: "panel", keywords: ["files", "modified", "panel"], handler: () => togglePanel("file", "Files") },
  { id: "open-diff", label: "Open Diff Panel", keys: "ctrl+shift+d", category: "panel", keywords: ["diff", "changes", "panel"], handler: () => togglePanel("diff", "Diff") },
  { id: "open-spec", label: "Open Spec Panel", keys: "ctrl+shift+s", category: "panel", keywords: ["spec", "openspec", "panel"], handler: () => togglePanel("spec", "Spec") },
  { id: "open-git", label: "Open Git Panel", keys: "ctrl+shift+g", category: "panel", keywords: ["git", "branch", "panel"], handler: () => togglePanel("git", "Git") },
  { id: "open-tasks", label: "Open Tasks Panel", keys: "ctrl+shift+k", category: "panel", keywords: ["tasks", "todo", "panel"], handler: () => togglePanel("tasks", "Tasks") },
  { id: "toggle-activity", label: "Toggle Activity Log", keys: "ctrl+shift+l", category: "panel", keywords: ["activity", "log", "panel"], handler: () => store().set(toggleActivityLogAtom, (p: number) => p + 1) },

  // -- Action --
  { id: "open-ide", label: "Open in IDE", keys: "ctrl+shift+e", category: "action", keywords: ["ide", "editor", "vscode", "zed"], handler: () => {} },
  { id: "merge-session", label: "Merge Session", keys: "ctrl+shift+m", category: "action", keywords: ["merge", "git", "branch"], handler: () => store().set(triggerMergeAtom, (p: number) => p + 1) },
  { id: "commit-session", label: "Commit Session", keys: "ctrl+shift+c", category: "action", keywords: ["commit", "git"], handler: () => store().set(triggerCommitAtom, (p: number) => p + 1) },
  { id: "command-palette", label: "Command Palette", keys: "ctrl+k", category: "navigation", keywords: ["command", "palette", "search", "find"], handler: () => {} },
]);
