import { atom } from "jotai";
import { invoke } from "@/lib/tauri";
import { activeWorkspaceAtom } from "./workspace";

export type SearchScope =
  | { kind: "vault" }
  | { kind: "sessionTranscripts" }
  | { kind: "openSpec" }
  | { kind: "workspaceFiles"; workspaceId: string }
  | { kind: "all" };

export interface SearchHit {
  path: string;
  lineNumber: number | null;
  lineText: string;
  scope: string;
  score: number;
  title: string | null;
}

export interface SearchQuery {
  text: string;
  scopes: SearchScope[];
  caseSensitive?: boolean;
  titlesOnly?: boolean;
  maxResults?: number;
}

export const searchModalOpenAtom = atom(false);
export const searchScopeAtom = atom<SearchScope>({ kind: "vault" });
export const searchQueryAtom = atom("");
export const searchResultsAtom = atom<SearchHit[]>([]);
export const searchLoadingAtom = atom(false);

// Rapid typing fires overlapping invokes; a monotonic seq lets the newest call
// win and drops every stale response. The optional AbortSignal lets a consumer
// (e.g. an unmounting modal) bail before committing results.
let searchSeq = 0;

export interface RunSearchOptions {
  titlesOnly?: boolean;
  maxResults?: number;
  caseSensitive?: boolean;
  signal?: AbortSignal;
}

export const runSearchAtom = atom(
  null,
  async (get, set, opts: RunSearchOptions = {}) => {
    const text = get(searchQueryAtom).trim();
    const seq = ++searchSeq;

    if (!text) {
      set(searchResultsAtom, []);
      set(searchLoadingAtom, false);
      return;
    }

    set(searchLoadingAtom, true);
    const scope = get(searchScopeAtom);
    const workspace = get(activeWorkspaceAtom);

    const query: SearchQuery = {
      text,
      scopes: [scope],
      caseSensitive: opts.caseSensitive ?? false,
      titlesOnly: opts.titlesOnly ?? false,
      maxResults: opts.maxResults ?? 50,
    };

    try {
      const hits = await invoke<SearchHit[]>("search", {
        query,
        activeWorkspaceId: workspace?.id ?? null,
      });
      if (seq !== searchSeq || opts.signal?.aborted) return;
      set(searchResultsAtom, hits);
    } catch (err) {
      if (seq !== searchSeq) return;
      set(searchResultsAtom, []);
      console.error("search failed", err);
    } finally {
      if (seq === searchSeq) set(searchLoadingAtom, false);
    }
  },
);
