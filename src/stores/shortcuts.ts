import { atom } from "jotai";
import { invoke } from "@/lib/tauri";
import { appStore } from "./jotaiStore";
import { configAtom, settingsOpenAtom } from "./config";
import { activeSessionIdAtom, activeWorkspaceAtom, workspacesAtom, freshSessionsAtom, sessionTabIdsAtom } from "./workspace";
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
  currentSpecArtifactAtom,
  type Tab,
} from "./rightPanel";
import { layoutPresetAtom, sessionLayoutPresetAtom, applyPresetSignalAtom, type LayoutPreset } from "./layout";
import { activityDrawerOpenAtom } from "./activity";
import { activeConflictedFilesAtom, refreshGitInfoAtom } from "./git";
import { conflictsZenOpenAtom, selectedConflictFileMapAtom } from "./conflict";
import { gitChipModeAtom } from "./git";
import { triggerShipAtom } from "./ship";
import { toastsAtom } from "./toast";
import { softCloseSessionAction, undoSessionCloseAction, hasPendingSessionCloseAtom } from "./sessionTabs";
import { invoke as invokeCmd } from "@/lib/tauri";
import { scratchpadOpenAtom } from "./scratchpad";

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
export const triggerAddWorkspaceAtom = atom(0);
export const triggerMergeAtom = atom(0);
export const triggerCommitAtom = atom(0);
export const triggerPushAtom = atom(0);
export const triggerJumpToProjectAtom = atom<{ tick: number; index: number }>({ tick: 0, index: -1 });
export const sidebarSelectedIdxAtom = atom<number>(-1);
export const triggerResumeSessionAtom = atom<string | null>(null);
/// Workspace that owns the session-numbers (1..9) currently displayed in the
/// sidebar. Set by Ctrl+Alt+N to the jumped-to workspace; cleared when focus
/// leaves the sidebar, at which point the numbers fall back to the workspace
/// that owns the active session.
export const focusedWorkspaceIdAtom = atom<string | null>(null);

function store() {
  return appStore;
}

/// Workspace whose sessions currently show shortcut numbers. Prefers the
/// Ctrl+Alt+N-focused workspace while the sidebar still owns focus, otherwise
/// falls back to the workspace containing the active session.
function numericTargetWorkspace() {
  const s = store();
  const workspaces = s.get(workspacesAtom);
  const focusedId = s.get(focusedWorkspaceIdAtom);
  const zone = s.get(focusZoneAtom);
  if (zone === "sidebar" && focusedId) {
    const ws = workspaces.find((w) => w.id === focusedId);
    if (ws) return ws;
  }
  const active = s.get(activeWorkspaceAtom);
  if (active) return active;
  return workspaces.find((w) => w.sessions.length > 0) ?? null;
}

function switchToSession(index: number) {
  const s = store();
  const workspace = numericTargetWorkspace();
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
  // Selecting clears the "I'm browsing another workspace" state.
  s.set(focusedWorkspaceIdAtom, null);
}

function switchToSessionInFocused(index: number) {
  const s = store();
  const focusedId = s.get(focusedWorkspaceIdAtom);
  const workspaces = s.get(workspacesAtom);
  const workspace = focusedId
    ? workspaces.find((w) => w.id === focusedId) ?? null
    : s.get(activeWorkspaceAtom);
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
  s.set(focusedWorkspaceIdAtom, null);
}

function togglePanel(type: Tab["type"], _label: string) {
  const s = store();
  const currentView = s.get(activePanelViewAtom);
  const currentActiveTab = s.get(activeTabAtom);

  if (currentActiveTab?.type === type || (currentView === type && !currentActiveTab)) {
    // Same panel pressed twice = close it. Clear the per-session view so the
    // layout-preset effect collapses the right panel and a later session
    // switch back doesn't auto-reopen this panel. Splitter-drag and the
    // generic Ctrl+Shift+B toggle deliberately don't clear the view (those
    // are "peek" interactions, not a close intent).
    if (currentView === type && !currentActiveTab) {
      s.set(activePanelViewAtom, null);
    } else {
      s.set(toggleRightPanelAtom, (p: number) => p + 1);
    }
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
  // Move focus to panel after opening
  requestAnimationFrame(() => focusZone("panel"));
}

function getVisibleZones(): FocusZone[] {
  const zones: FocusZone[] = ["sidebar", "terminal"];
  if (document.querySelector("[data-focus-zone='panel']")) zones.push("panel");
  return zones;
}

function detectCurrentZone(): FocusZone {
  const active = document.activeElement;
  if (active?.closest("[data-focus-zone='panel']")) return "panel";
  if (active?.closest("[data-focus-zone='sidebar']")) return "sidebar";
  if (active?.closest("[data-focus-zone='terminal']")) return "terminal";
  // Fallback to stored zone
  return store().get(focusZoneAtom);
}

function focusZone(zone: FocusZone) {
  store().set(focusZoneAtom, zone);

  const el = document.querySelector(`[data-focus-zone='${zone}']`) as HTMLElement | null;
  if (!el) return;

  if (zone === "terminal") {
    terminalService.focusActive();
  } else {
    // Blur the terminal's input textarea first so it doesn't immediately
    // recapture focus after we move it to the target zone.
    const terminalInput = document.querySelector(
      "[data-focus-zone='terminal'] textarea",
    ) as HTMLElement | null;
    terminalInput?.blur();
    // Focus the deepest focusable container (e.g. file picker inside panel)
    const focusTarget = el.querySelector<HTMLElement>("[data-nav-container]") ?? el;
    focusTarget.focus();
    const items = focusTarget.querySelectorAll<HTMLElement>("[data-nav-item]");
    if (items.length === 0) {
      const outerItems = el.querySelectorAll<HTMLElement>("[data-nav-item]");
      for (const item of outerItems) item.removeAttribute("data-nav-selected");
      if (outerItems[0]) outerItems[0].setAttribute("data-nav-selected", "true");
    } else {
      for (const item of items) item.removeAttribute("data-nav-selected");
      if (items[0]) items[0].setAttribute("data-nav-selected", "true");
    }
  }
  flashZone(el);
}

function flashZone(el: HTMLElement | null) {
  if (!el) return;
  el.classList.remove("zone-flash");
  void el.offsetWidth;
  el.classList.add("zone-flash");
  el.addEventListener("animationend", () => el.classList.remove("zone-flash"), { once: true });
}

function navigateItems(direction: "up" | "down") {
  const zone = detectCurrentZone();
  if (zone === "terminal") return;

  const container = document.querySelector(`[data-focus-zone='${zone}']`) as HTMLElement | null;
  if (!container) return;

  const items = Array.from(container.querySelectorAll<HTMLElement>("[data-nav-item]"));
  if (items.length === 0) return;

  const active = document.activeElement as HTMLElement;
  const currentItem = active?.closest("[data-nav-item]") as HTMLElement | null;
  const currentIdx = currentItem ? items.indexOf(currentItem) : -1;

  let nextIdx: number;
  if (currentIdx === -1) {
    nextIdx = direction === "down" ? 0 : items.length - 1;
  } else {
    nextIdx = direction === "down"
      ? (currentIdx + 1) % items.length
      : (currentIdx - 1 + items.length) % items.length;
  }
  items[nextIdx].focus();
  items[nextIdx].scrollIntoView({ block: "nearest" });
}

function nextTab() {
  const zone = store().get(focusZoneAtom);
  if (zone === "panel") {
    nextPanelTab();
  } else {
    nextSessionTab();
  }
}

function prevTab() {
  const zone = store().get(focusZoneAtom);
  if (zone === "panel") {
    prevPanelTab();
  } else {
    prevSessionTab();
  }
}

function nextSessionTab() {
  const s = store();
  const tabIds = s.get(sessionTabIdsAtom);
  const activeId = s.get(activeSessionIdAtom);
  if (tabIds.length === 0) return;
  const idx = tabIds.indexOf(activeId ?? "");
  // When the active session isn't in the tab bar (e.g. user jumped workspaces
  // without opening a tab), landing on the first tab is the least surprising
  // behavior rather than staying on a non-tab session.
  const next = idx < 0 ? 0 : (idx + 1) % tabIds.length;
  s.set(activeSessionIdAtom, tabIds[next]);
}

function prevSessionTab() {
  const s = store();
  const tabIds = s.get(sessionTabIdsAtom);
  const activeId = s.get(activeSessionIdAtom);
  if (tabIds.length === 0) return;
  const idx = tabIds.indexOf(activeId ?? "");
  const prev = idx < 0 ? tabIds.length - 1 : (idx <= 0 ? tabIds.length - 1 : idx - 1);
  s.set(activeSessionIdAtom, tabIds[prev]);
}

function nextPanelTab() {
  const s = store();
  const tabs = s.get(activeTabsAtom);
  const activeId = s.get(activeTabIdAtom);
  if (tabs.length === 0) return;
  const idx = tabs.findIndex((t) => t.id === activeId);
  const next = (idx + 1) % tabs.length;
  s.set(activeTabIdAtom, tabs[next].id);
}

function prevPanelTab() {
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
  const zone = s.get(focusZoneAtom);
  if (zone === "panel") {
    const activeId = s.get(activeTabIdAtom);
    if (activeId) s.set(closeTabAction, activeId);
    return;
  }
  // Terminal/sidebar zones close the active session tab via soft-close: the
  // PTY stays alive for SOFT_CLOSE_TTL_MS so Ctrl+Shift+T (or the toast's
  // Undo button) can restore it instantly. Mirrors how Ctrl+Tab cycles
  // session-tabs vs panel-tabs based on focused zone.
  const activeSessionId = s.get(activeSessionIdAtom);
  if (activeSessionId) s.set(softCloseSessionAction, activeSessionId);
}

function reopenLastTab() {
  const s = store();
  const zone = s.get(focusZoneAtom);
  // In terminal/sidebar zones prefer reviving a soft-closed session if one
  // is still in its TTL window; only fall through to panel-tab reopen when
  // no session is pending. Panel zone always operates on panel tabs.
  if (zone !== "panel" && s.get(hasPendingSessionCloseAtom)) {
    s.set(undoSessionCloseAction);
    return;
  }
  s.set(reopenTabAction);
}

export const shortcutRegistryAtom = atom<ShortcutAction[]>([
  // -- Navigation --
  { id: "toggle-sidebar", label: "Toggle Sidebar", keys: "ctrl+b", category: "navigation", keywords: ["sidebar", "left", "panel"], handler: () => store().set(toggleSidebarAtom, (p: number) => p + 1) },
  { id: "toggle-right-panel", label: "Toggle Right Panel", keys: "ctrl+shift+b", category: "navigation", keywords: ["right", "panel"], handler: () => store().set(toggleRightPanelAtom, (p: number) => p + 1) },
  { id: "focus-left", label: "Focus Left", keys: "alt+arrowleft", category: "navigation", keywords: ["move", "focus", "left"], handler: () => {
    const zones = getVisibleZones();
    const current = store().get(focusZoneAtom);
    const idx = zones.indexOf(current);
    const prev = idx <= 0 ? zones.length - 1 : idx - 1;
    focusZone(zones[prev]);
  }},
  { id: "focus-right", label: "Focus Right", keys: "alt+arrowright", category: "navigation", keywords: ["move", "focus", "right"], handler: () => {
    const zones = getVisibleZones();
    const current = store().get(focusZoneAtom);
    const idx = zones.indexOf(current);
    const next = (idx + 1) % zones.length;
    focusZone(zones[next]);
  }},
  { id: "focus-terminal", label: "Focus Terminal", keys: "ctrl+ñ", category: "navigation", keywords: ["terminal", "pty", "cli"], handler: () => focusZone("terminal") },
  { id: "nav-up", label: "Navigate Up", keys: "alt+arrowup", category: "navigation", keywords: ["navigate", "up", "item"], handler: () => navigateItems("up") },
  { id: "nav-down", label: "Navigate Down", keys: "alt+arrowdown", category: "navigation", keywords: ["navigate", "down", "item"], handler: () => navigateItems("down") },

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
  { id: "focused-session-1", label: "Select Session 1 in Focused Workspace", keys: "ctrl+shift+1", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(0) },
  { id: "focused-session-2", label: "Select Session 2 in Focused Workspace", keys: "ctrl+shift+2", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(1) },
  { id: "focused-session-3", label: "Select Session 3 in Focused Workspace", keys: "ctrl+shift+3", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(2) },
  { id: "focused-session-4", label: "Select Session 4 in Focused Workspace", keys: "ctrl+shift+4", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(3) },
  { id: "focused-session-5", label: "Select Session 5 in Focused Workspace", keys: "ctrl+shift+5", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(4) },
  { id: "focused-session-6", label: "Select Session 6 in Focused Workspace", keys: "ctrl+shift+6", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(5) },
  { id: "focused-session-7", label: "Select Session 7 in Focused Workspace", keys: "ctrl+shift+7", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(6) },
  { id: "focused-session-8", label: "Select Session 8 in Focused Workspace", keys: "ctrl+shift+8", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(7) },
  { id: "focused-session-9", label: "Select Session 9 in Focused Workspace", keys: "ctrl+shift+9", category: "session", keywords: ["session", "switch", "focused"], handler: () => switchToSessionInFocused(8) },
  { id: "new-session", label: "New Session", keys: "ctrl+n", category: "session", keywords: ["create", "session", "new"], handler: () => store().set(triggerNewSessionAtom, (p: number) => p + 1) },
  { id: "project-1", label: "Jump to Project 1", keys: "ctrl+alt+1", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 0 }) },
  { id: "project-2", label: "Jump to Project 2", keys: "ctrl+alt+2", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 1 }) },
  { id: "project-3", label: "Jump to Project 3", keys: "ctrl+alt+3", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 2 }) },
  { id: "project-4", label: "Jump to Project 4", keys: "ctrl+alt+4", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 3 }) },
  { id: "project-5", label: "Jump to Project 5", keys: "ctrl+alt+5", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 4 }) },
  { id: "project-6", label: "Jump to Project 6", keys: "ctrl+alt+6", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 5 }) },
  { id: "project-7", label: "Jump to Project 7", keys: "ctrl+alt+7", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 6 }) },
  { id: "project-8", label: "Jump to Project 8", keys: "ctrl+alt+8", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 7 }) },
  { id: "project-9", label: "Jump to Project 9", keys: "ctrl+alt+9", category: "navigation", keywords: ["project", "workspace", "jump"], handler: () => store().set(triggerJumpToProjectAtom, { tick: Date.now(), index: 8 }) },
  { id: "add-workspace", label: "Add Workspace", keys: "ctrl+shift+n", category: "session", keywords: ["workspace", "add", "folder"], handler: () => store().set(triggerAddWorkspaceAtom, (p: number) => p + 1) },

  // -- Panel (tabs) --
  { id: "next-tab", label: "Next Tab", keys: "ctrl+tab", category: "panel", keywords: ["tab", "next", "session"], handler: nextTab },
  { id: "prev-tab", label: "Previous Tab", keys: "ctrl+shift+tab", category: "panel", keywords: ["tab", "previous", "prev", "session"], handler: prevTab },
  { id: "save-file", label: "Save File", keys: "ctrl+s", category: "action", keywords: ["save", "file", "write"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:save-file"));
  }},
  { id: "close-tab", label: "Close Active Tab (panel tab or session, by focused zone)", keys: "ctrl+w", category: "panel", keywords: ["tab", "close", "session"], handler: closeCurrentTab },
  { id: "reopen-tab", label: "Reopen Last Closed (session if pending, else panel tab)", keys: "ctrl+shift+t", category: "panel", keywords: ["tab", "reopen", "undo", "session"], handler: reopenLastTab },
  { id: "open-plan", label: "Open Plan Panel", keys: "ctrl+shift+p", category: "panel", keywords: ["plan", "panel"], handler: () => togglePanel("plan", "Plan") },
  { id: "open-files", label: "Open Files Panel", keys: "ctrl+shift+f", category: "panel", keywords: ["files", "modified", "panel"], handler: () => togglePanel("file", "Files") },
  { id: "open-diff", label: "Open Diff Panel", keys: "ctrl+shift+d", category: "panel", keywords: ["diff", "changes", "panel"], handler: () => togglePanel("diff", "Diff") },
  { id: "open-spec", label: "Open Spec Panel", keys: "ctrl+shift+s", category: "panel", keywords: ["spec", "openspec", "panel"], handler: () => togglePanel("spec", "Spec") },
  { id: "open-git", label: "Open Git Panel", keys: "ctrl+shift+g", category: "panel", keywords: ["git", "branch", "panel"], handler: () => togglePanel("git", "Git") },
  { id: "toggle-file-picker", label: "Toggle File Picker", keys: "ctrl+shift+k", category: "panel", keywords: ["file", "picker", "browse", "explorer"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:toggle-file-picker"));
  }},
  { id: "toggle-activity", label: "Toggle Activity Drawer", keys: "ctrl+shift+l", category: "panel", keywords: ["activity", "log", "timeline", "drawer"], handler: () => store().set(activityDrawerOpenAtom, (prev: boolean) => !prev) },
  { id: "toggle-scratchpad", label: "Toggle Scratchpad", keys: "ctrl+alt+l", category: "panel", keywords: ["scratchpad", "notes", "scratch", "buffer"], handler: () => store().set(scratchpadOpenAtom, (prev: boolean) => !prev) },
  { id: "toggle-annotations", label: "Toggle Annotations Drawer", keys: "ctrl+shift+j", category: "panel", keywords: ["annotations", "drawer", "comments", "plan"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:toggle-annotations-drawer"));
  }},

  // -- Plan review (contextual — only active during pending_review) --
  { id: "global-comment", label: "Add Global Comment", keys: "ctrl+shift+o", category: "action", keywords: ["comment", "global", "plan", "annotation"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:toggle-global-comment"));
  }},
  { id: "clear-annotations", label: "Clear All Annotations", keys: "ctrl+shift+x", category: "action", keywords: ["clear", "annotations", "plan", "remove"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:clear-annotations"));
  }},
  { id: "approve-plan", label: "Approve Plan", keys: "ctrl+shift+a", category: "action", keywords: ["approve", "plan", "review", "accept"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:approve-plan"));
  }},
  { id: "revise-or-resolve", label: "Revise Plan / Resolve Conflict / Apply PR Annotations (contextual)", keys: "ctrl+shift+r", category: "action", keywords: ["revise", "plan", "review", "reject", "feedback", "resolve", "conflict", "apply", "pr", "claude"], handler: () => {
    const s = store();
    const activeTab = s.get(activeTabAtom);
    const conflicted = s.get(activeConflictedFilesAtom);
    const sid = s.get(activeSessionIdAtom);
    if (sid && conflicted.length > 0) {
      const chipMap = s.get(gitChipModeAtom);
      const currentChip = chipMap[sid] ?? "files";
      // Already on the Conflicts chip → forward to the panel's own resolver.
      if (currentChip === "conflicts" && activeTab?.type === "git") {
        document.dispatchEvent(new CustomEvent("cluihud:resolve-conflict-active-tab"));
        return;
      }
      s.set(selectedConflictFileMapAtom, (prev) => ({ ...prev, [sid]: conflicted[0] }));
      s.set(gitChipModeAtom, (prev) => ({ ...prev, [sid]: "conflicts" }));
      document.dispatchEvent(new CustomEvent("cluihud:open-first-conflict", { detail: { path: conflicted[0] } }));
      return;
    }
    // PR context — dispatch apply-pr-annotations so the active PrViewer (chip
    // body or PR Zen) sends its annotations to the owning session terminal.
    // PrViewer's listener no-ops if there are no annotations or the owning
    // session isn't active, so the shortcut is safe to fire from anywhere.
    if (sid) {
      const chipMap = s.get(gitChipModeAtom);
      const currentChip = chipMap[sid] ?? "files";
      if (currentChip === "prs") {
        document.dispatchEvent(new CustomEvent("cluihud:apply-pr-annotations"));
        return;
      }
    }
    if (activeTab?.type === "plan" || activeTab?.type === "spec") {
      document.dispatchEvent(new CustomEvent("cluihud:revise-plan"));
    }
  }},
  { id: "toggle-annotation-mode", label: "Toggle Annotation Mode", keys: "ctrl+shift+h", category: "action", keywords: ["annotate", "annotation", "keyboard", "plan", "review", "highlight"], handler: () => {
    document.dispatchEvent(new CustomEvent("cluihud:toggle-annotation-mode"));
  }},

  { id: "expand-zen", label: "Expand active panel to Zen", keys: "ctrl+shift+0", category: "navigation", keywords: ["zen", "expand", "maximize", "fullscreen"], handler: () => {
    const s = store();
    const sessionId = s.get(activeSessionIdAtom);
    if (!sessionId) return;
    const tab = s.get(activeTabAtom);
    const panelView = s.get(activePanelViewAtom);
    // Resolve the effective panel kind: an active tab wins, otherwise the
    // standalone panel view (e.g. Git panel opened via Ctrl+Shift+G with
    // no tab) drives the Zen target. Without this fallback the Files /
    // Conflicts chips can't trigger Zen because no tab is ever created.
    const panelType = tab?.type ?? panelView;
    if (!panelType) return;
    if (panelType === "diff" || panelType === "file") {
      const filePath = tab?.data?.path as string | undefined;
      if (filePath) document.dispatchEvent(new CustomEvent("cluihud:expand-zen", { detail: { filePath, sessionId } }));
      return;
    }
    if (panelType === "git") {
      const chipMap = s.get(gitChipModeAtom);
      const chip = chipMap[sessionId] ?? "files";
      if (chip === "conflicts") {
        s.set(conflictsZenOpenAtom, (v) => !v);
        return;
      }
      if (chip === "prs") {
        // PRs chip owns the Zen target (it knows which PR is selected). The
        // workspaceId lets the chip filter its listener so other workspaces'
        // chips don't react to a Zen request meant for this one.
        const workspaces = s.get(workspacesAtom);
        const ws = workspaces.find((w) => w.sessions.some((sx) => sx.id === sessionId));
        if (!ws) return;
        document.dispatchEvent(new CustomEvent("cluihud:expand-zen-pr", { detail: { workspaceId: ws.id } }));
        return;
      }
      document.dispatchEvent(new CustomEvent("cluihud:expand-zen-git", { detail: { sessionId } }));
    }
  }},
  { id: "cycle-layout", label: "Cycle Layout Preset", keys: "ctrl+shift+i", category: "navigation", keywords: ["layout", "preset", "cycle", "resize"], handler: () => {
    const s = store();
    const presets: LayoutPreset[] = ["terminal-focus", "doc-review", "tool-workspace"];
    const current = s.get(layoutPresetAtom);
    const idx = presets.indexOf(current);
    const next = presets[(idx + 1) % presets.length];
    s.set(sessionLayoutPresetAtom, next);
    s.set(applyPresetSignalAtom, (p: number) => p + 1);
  }},

  // -- Action --
  { id: "open-ide", label: "Open in IDE", keys: "ctrl+shift+e", category: "action", keywords: ["ide", "editor", "vscode", "zed"], handler: () => {
    const sid = store().get(activeSessionIdAtom);
    if (!sid) return;
    const config = store().get(configAtom);
    const editorId = config.preferred_editor || "zed";
    const tab = store().get(activeTabAtom);
    let filePath: string | null = null;
    let specChangeName: string | null = null;
    let specArtifactPath: string | null = null;

    if (tab?.type === "diff" || tab?.type === "file" || tab?.type === "plan") {
      filePath = (tab.data?.path as string) ?? null;
    } else if (tab?.type === "spec") {
      const specCtx = store().get(currentSpecArtifactAtom);
      if (specCtx) {
        specChangeName = specCtx.changeName;
        specArtifactPath = specCtx.artifactPath;
      }
    }

    invoke("open_in_editor", { sessionId: sid, editorId, filePath, specChangeName, specArtifactPath }).catch(() => {});
  }},
  // Local-merge shortcut intentionally removed — user's flow is Ship-driven;
  // local merge is a rare GitPanel button click. The `triggerMergeAtom` export
  // stays in case other surfaces want to invoke programmatically.
  // Commit shortcut intentionally removed — committing requires a message,
  // and the GitPanel textarea already binds Ctrl+Enter locally for that flow.
  { id: "ship-session", label: "Ship (commit + push + PR)", keys: "ctrl+shift+y", category: "action", keywords: ["ship", "pr", "push", "commit", "deploy", "yeet"], handler: () => {
    const s = store();
    const sid = s.get(activeSessionIdAtom);
    if (!sid) {
      s.set(toastsAtom, { message: "Ship", description: "No active session", type: "info" });
      return;
    }
    // Pre-check before opening the modal: don't make the user open and
    // dismiss an empty Ship dialog when there's nothing to do. Open the
    // dialog only when there's actually something to ship (commits in
    // base..HEAD, staged changes, OR unstaged work that could be staged).
    Promise.all([
      invokeCmd<{ commits: unknown[]; staged_count: number }>("get_pr_preview_data", { sessionId: sid }).catch(() => null),
      invokeCmd<{ unstaged: unknown[]; untracked: string[] }>("get_git_status", { sessionId: sid }).catch(() => null),
    ]).then(([preview, status]) => {
      const commits = preview?.commits.length ?? 0;
      const staged = preview?.staged_count ?? 0;
      const unstaged = (status?.unstaged.length ?? 0) + (status?.untracked.length ?? 0);
      if (commits === 0 && staged === 0 && unstaged === 0) {
        s.set(toastsAtom, {
          message: "Ship",
          description: "Nothing to ship — no commits ahead, no staged changes, no unstaged work.",
          type: "info",
        });
        return;
      }
      s.set(triggerShipAtom, { tick: Date.now(), sessionId: sid, inlineMessage: null });
    });
  }},
  { id: "complete-merge", label: "Complete Merge", keys: "ctrl+alt+enter", category: "action", keywords: ["merge", "complete", "finish"], handler: () => {
    const s = store();
    const sid = s.get(activeSessionIdAtom);
    if (!sid) return;
    invokeCmd<boolean>("has_pending_merge", { sessionId: sid })
      .then((pending) => {
        if (!pending) {
          s.set(toastsAtom, { message: "Merge", description: "No pending merge", type: "info" });
          return;
        }
        return invokeCmd<string>("complete_pending_merge", { sessionId: sid })
          .then(() => s.set(toastsAtom, { message: "Merge", description: "Merge commit created", type: "success" }))
          .catch((e: unknown) => {
            // Print full error to console so the user can inspect even if
            // the toast text gets clipped (git stderr is often multi-line).
            console.error("complete_pending_merge failed", e);
            s.set(toastsAtom, {
              message: "Complete merge failed",
              description: `${String(e).slice(0, 220)} — full error in DevTools console.`,
              type: "error",
            });
          });
      });
  }},
  { id: "push-session", label: "Push Session", keys: "ctrl+alt+p", category: "action", keywords: ["push", "upload", "remote"], handler: () => {
    const s = store();
    const sid = s.get(activeSessionIdAtom);
    if (!sid) {
      s.set(toastsAtom, { message: "Push", description: "No active session", type: "info" });
      return;
    }
    invokeCmd<boolean>("git_push", { sessionId: sid })
      .then((pushed) => {
        s.set(toastsAtom, {
          message: "Push",
          description: pushed ? "Pushed to remote" : "Nothing to push",
          type: pushed ? "success" : "info",
        });
        s.set(refreshGitInfoAtom, sid);
      })
      .catch((err: unknown) => s.set(toastsAtom, { message: "Push failed", description: String(err), type: "error" }));
  }},
  { id: "command-palette", label: "Command Palette", keys: "ctrl+k", category: "navigation", keywords: ["command", "palette", "search", "find"], handler: () => {} },
  { id: "open-settings", label: "Open Settings", keys: "ctrl+,", category: "navigation", keywords: ["settings", "preferences", "config", "options"], handler: () => store().set(settingsOpenAtom, (prev: boolean) => !prev) },
]);
