import { atom } from "jotai";
import { invoke } from "@/lib/tauri";
import { activeSessionIdAtom } from "./workspace";

export type BrowserMode = "dock" | "floating";
export type BrowserColorScheme = "light" | "dark";

export interface BrowserTab {
  id: string;
  url: string;
  back: string[];
  forward: string[];
  /// Bumped to force iframe re-fetch (key prop).
  reloadKey: number;
  /// Bumped on hard-reload to append a `?_cb=<n>` query param. Forces a
  /// fresh fetch bypassing any HTTP cache the embedded WebKit holds —
  /// matches Ctrl+Shift+R semantics in real browsers. Soft reload keeps
  /// this stable so it doesn't pollute the URL on every F5.
  cacheBust: number;
  /// Best-effort label derived from URL hostname or last path segment.
  label: string;
}

export interface BrowserSessionState {
  tabs: BrowserTab[];
  activeTabId: string | null;
  mode: BrowserMode;
  colorScheme: BrowserColorScheme;
}

const DEFAULT_URL = "about:blank";

function emptyState(): BrowserSessionState {
  // Default `dark` matches cluihud's chrome aesthetic. Sites that respect
  // `prefers-color-scheme: dark` will render dark; light-only sites stay
  // light. User can flip via the Sun/Moon toolbar button.
  return { tabs: [], activeTabId: null, mode: "dock", colorScheme: "dark" };
}

function makeTabId(): string {
  return `bt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function deriveLabel(url: string): string {
  if (!url || url === DEFAULT_URL) return "New tab";
  try {
    const u = new URL(url);
    if (u.hostname === "localhost" || u.hostname === "127.0.0.1") {
      return u.port ? `:${u.port}` : "localhost";
    }
    return u.hostname.replace(/^www\./, "");
  } catch {
    return url.slice(0, 24);
  }
}

export const browserSessionsAtom = atom<Record<string, BrowserSessionState>>({});

export const browserSessionForActiveAtom = atom<BrowserSessionState>((get) => {
  const sid = get(activeSessionIdAtom);
  if (!sid) return emptyState();
  return get(browserSessionsAtom)[sid] ?? emptyState();
});

export const browserActiveTabAtom = atom<BrowserTab | null>((get) => {
  const state = get(browserSessionForActiveAtom);
  if (!state.activeTabId) return null;
  return state.tabs.find((t) => t.id === state.activeTabId) ?? null;
});

export const browserModeForSessionAtom = atom<BrowserMode>((get) => {
  return get(browserSessionForActiveAtom).mode;
});

export const browserColorSchemeForSessionAtom = atom<BrowserColorScheme>((get) => {
  return get(browserSessionForActiveAtom).colorScheme;
});

/// Globally detected localhost dev-server ports (emitted by the Rust scanner).
/// Not per-session.
export const localhostPortsAtom = atom<number[]>([]);

/// Bounding box (viewport coords) of the active browser slot. Used by
/// `BrowserHost` to position itself fixed-overlay so the iframe never
/// DOM-moves between parents (which would force a reload per HTML spec).
export interface BrowserSlotBbox {
  x: number;
  y: number;
  w: number;
  h: number;
}

function updateSession(
  prev: Record<string, BrowserSessionState>,
  sid: string,
  updater: (s: BrowserSessionState) => BrowserSessionState,
): Record<string, BrowserSessionState> {
  const current = prev[sid] ?? emptyState();
  return { ...prev, [sid]: updater(current) };
}

function updateTab(
  state: BrowserSessionState,
  tabId: string,
  updater: (t: BrowserTab) => BrowserTab,
): BrowserSessionState {
  return {
    ...state,
    tabs: state.tabs.map((t) => (t.id === tabId ? updater(t) : t)),
  };
}

/// Open a new internal tab (and create the panel state if first). If a tab
/// with the same URL already exists AND it isn't the placeholder
/// `about:blank`, focus it instead of duplicating. about:blank tabs are
/// always created fresh — the user clicked `+` for a reason.
export const browserNewTabAction = atom(
  null,
  async (_get, set, params: { sessionId: string; url?: string }) => {
    const initialUrl = params.url ?? DEFAULT_URL;
    const validatedUrl =
      initialUrl === DEFAULT_URL
        ? DEFAULT_URL
        : await invoke<string>("browser_validate_url", { url: initialUrl });
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) => {
        if (validatedUrl !== DEFAULT_URL) {
          const existing = state.tabs.find((t) => t.url === validatedUrl);
          if (existing) {
            return { ...state, activeTabId: existing.id };
          }
        }
        const newTab: BrowserTab = {
          id: makeTabId(),
          url: validatedUrl,
          back: [],
          forward: [],
          reloadKey: 0,
          cacheBust: 0,
          label: deriveLabel(validatedUrl),
        };
        return {
          ...state,
          tabs: [...state.tabs, newTab],
          activeTabId: newTab.id,
        };
      });
    });
  },
);

export const browserCloseTabAction = atom(
  null,
  (_get, set, params: { sessionId: string; tabId: string }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) => {
        const idx = state.tabs.findIndex((t) => t.id === params.tabId);
        if (idx < 0) return state;
        const remaining = state.tabs.filter((t) => t.id !== params.tabId);
        let nextActive = state.activeTabId;
        if (state.activeTabId === params.tabId) {
          // Activate the next-best tab: the one to the right, else to the left.
          const fallback = remaining[idx] ?? remaining[idx - 1] ?? null;
          nextActive = fallback?.id ?? null;
        }
        return { ...state, tabs: remaining, activeTabId: nextActive };
      });
    });
  },
);

export const browserActivateTabAction = atom(
  null,
  (_get, set, params: { sessionId: string; tabId: string }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) => ({
        ...state,
        activeTabId: params.tabId,
      }));
    });
  },
);

/// Validate via backend then commit the navigation to the active tab (or a
/// specific tabId). Pushes the previous URL to history.back, clears
/// history.forward.
export const browserNavigateAction = atom(
  null,
  async (
    _get,
    set,
    params: { sessionId: string; url: string; tabId?: string },
  ) => {
    const validated = await invoke<string>("browser_validate_url", {
      url: params.url,
    });
    set(browserSessionsAtom, (prev) => {
      const state = prev[params.sessionId] ?? emptyState();
      const tabId = params.tabId ?? state.activeTabId;
      if (!tabId) return prev;
      return updateSession(prev, params.sessionId, (s) =>
        updateTab(s, tabId, (t) => {
          const back =
            t.url && t.url !== DEFAULT_URL && t.url !== validated
              ? [...t.back, t.url]
              : t.back;
          return {
            ...t,
            url: validated,
            back,
            forward: [],
            label: deriveLabel(validated),
          };
        }),
      );
    });
  },
);

export const browserGoBackAction = atom(
  null,
  (_get, set, params: { sessionId: string; tabId: string }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) =>
        updateTab(state, params.tabId, (t) => {
          const target = t.back[t.back.length - 1];
          if (!target) return t;
          return {
            ...t,
            url: target,
            back: t.back.slice(0, -1),
            forward: [...t.forward, t.url],
            label: deriveLabel(target),
          };
        }),
      );
    });
  },
);

export const browserGoForwardAction = atom(
  null,
  (_get, set, params: { sessionId: string; tabId: string }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) =>
        updateTab(state, params.tabId, (t) => {
          const target = t.forward[t.forward.length - 1];
          if (!target) return t;
          return {
            ...t,
            url: target,
            back: [...t.back, t.url],
            forward: t.forward.slice(0, -1),
            label: deriveLabel(target),
          };
        }),
      );
    });
  },
);

export const browserReloadAction = atom(
  null,
  (_get, set, params: { sessionId: string; tabId: string }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) =>
        updateTab(state, params.tabId, (t) => ({
          ...t,
          reloadKey: t.reloadKey + 1,
        })),
      );
    });
  },
);

/// Hard reload: bumps `reloadKey` AND `cacheBust` so the iframe `src`
/// gains a fresh `?_cb=<n>` query param, forcing the embedded WebKit to
/// fetch from origin instead of serving from cache. Matches the semantics
/// of Ctrl+Shift+R in real browsers.
export const browserHardReloadAction = atom(
  null,
  (_get, set, params: { sessionId: string; tabId: string }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) =>
        updateTab(state, params.tabId, (t) => ({
          ...t,
          reloadKey: t.reloadKey + 1,
          cacheBust: t.cacheBust + 1,
        })),
      );
    });
  },
);

export const browserSetModeAction = atom(
  null,
  (_get, set, params: { sessionId: string; mode: BrowserMode }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) => ({
        ...state,
        mode: params.mode,
      }));
    });
  },
);

export const browserToggleModeAction = atom(
  null,
  (_get, set, sessionId: string) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, sessionId, (state) => ({
        ...state,
        mode: state.mode === "dock" ? "floating" : "dock",
      }));
    });
  },
);

/// Cycle the active internal tab forward (+1) or backward (-1). Used by
/// Ctrl+Tab / Ctrl+Shift+Tab when focus is inside the browser panel.
export const browserCycleTabAction = atom(
  null,
  (_get, set, params: { sessionId: string; direction: 1 | -1 }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) => {
        if (state.tabs.length <= 1) return state;
        const idx = state.tabs.findIndex((t) => t.id === state.activeTabId);
        if (idx < 0) return { ...state, activeTabId: state.tabs[0]?.id ?? null };
        const nextIdx =
          (idx + params.direction + state.tabs.length) % state.tabs.length;
        return { ...state, activeTabId: state.tabs[nextIdx].id };
      });
    });
  },
);

export const browserSetColorSchemeAction = atom(
  null,
  (_get, set, params: { sessionId: string; scheme: BrowserColorScheme }) => {
    set(browserSessionsAtom, (prev) => {
      return updateSession(prev, params.sessionId, (state) => ({
        ...state,
        colorScheme: params.scheme,
      }));
    });
  },
);
