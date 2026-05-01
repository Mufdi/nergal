import { atom } from "jotai";

export type ZenZone = "viewer" | "sidebar";

/// PR Zen target. When non-null, ZenMode renders PrViewer fullscreen instead
/// of DiffView — mirroring how `conflictsZenOpenAtom` swaps in ConflictsPanel.
/// We carry the full PR row so the viewer can render its merge button, CI
/// pill, etc. without re-querying. Set via `cluihud:expand-zen-pr` from the
/// PRs chip; cleared by closing Zen.
export interface PrZenTarget {
  workspaceId: string;
  prNumber: number;
  title: string;
  state: string;
  url: string;
  baseRefName: string;
  headRefName: string;
  updatedAt: string;
}

export const prZenAtom = atom<PrZenTarget | null>(null);

/// The Zen overlay is split into a viewer (DiffView / PrViewer / ConflictsPanel)
/// and a git sidebar. This atom tracks which side currently owns keyboard input,
/// so the active component's listener can fire while the other one stays out.
/// We can't rely on DOM focus alone because Tauri's WebKit on Linux loses
/// `tabIndex=-1` focus on rapid React commits — using state is deterministic.
/// The default ("viewer") matches the natural intent: Zen opens, you read.
export const zenActiveZoneAtom = atom<ZenZone>("viewer");

export interface ZenModeState {
  open: boolean;
  filePath: string | null;
  sessionId: string | null;
  files: string[];
  currentIndex: number;
}

const defaultState: ZenModeState = {
  open: false,
  filePath: null,
  sessionId: null,
  files: [],
  currentIndex: 0,
};

export const zenModeAtom = atom<ZenModeState>(defaultState);

export const openZenModeAtom = atom(
  null,
  (_get, set, params: { filePath: string; sessionId: string; files: string[] }) => {
    const idx = params.files.indexOf(params.filePath);
    set(zenModeAtom, {
      open: true,
      filePath: params.filePath,
      sessionId: params.sessionId,
      files: params.files,
      currentIndex: idx >= 0 ? idx : 0,
    });
  },
);

export const closeZenModeAtom = atom(null, (_get, set) => {
  set(zenModeAtom, defaultState);
  set(zenActiveZoneAtom, "viewer");
  set(prZenAtom, null);
});

export const zenModeNavigateAtom = atom(
  null,
  (get, set, direction: "prev" | "next") => {
    const state = get(zenModeAtom);
    if (!state.open || state.files.length === 0) return;
    let idx = state.currentIndex;
    if (direction === "next") {
      idx = (idx + 1) % state.files.length;
    } else {
      idx = idx <= 0 ? state.files.length - 1 : idx - 1;
    }
    set(zenModeAtom, {
      ...state,
      currentIndex: idx,
      filePath: state.files[idx],
    });
  },
);

export const zenModeSelectFileAtom = atom(
  null,
  (get, set, filePath: string) => {
    const state = get(zenModeAtom);
    if (!state.open) return;
    const idx = state.files.indexOf(filePath);
    set(zenModeAtom, {
      ...state,
      filePath,
      currentIndex: idx >= 0 ? idx : state.currentIndex,
    });
  },
);
