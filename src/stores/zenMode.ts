import { atom } from "jotai";

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
