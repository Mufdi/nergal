import { useEffect, useRef, useState } from "react";
import { FileText, X } from "lucide-react";
import { invoke } from "@/lib/tauri";

/// Subset of the `search` command's SearchHit we consume (src/stores/search.ts).
interface SearchHit {
  path: string;
  title: string | null;
  lineText: string;
}

function noteName(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

/// Type-to-search vault-note picker that collects notes as removable chips.
/// Drives the workspace-scoped `search` command (same as the vault finder /
/// @@ picker) — no session needed. Selected notes are stored as absolute paths,
/// matching the format the pin flow uses.
export function VaultNoteChipInput({
  value,
  onChange,
  workspaceId,
  vaultSubdir,
}: {
  value: string[];
  onChange: (paths: string[]) => void;
  workspaceId: string | null;
  vaultSubdir: string | null;
}) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [activeIdx, setActiveIdx] = useState(0);
  const [open, setOpen] = useState(false);
  const seqRef = useRef(0);

  useEffect(() => {
    const q = query.trim();
    if (!q || !workspaceId) {
      setHits([]);
      setOpen(false);
      return;
    }
    const seq = ++seqRef.current;
    const t = setTimeout(() => {
      invoke<SearchHit[]>("search", {
        query: { text: q, scopes: [{ kind: "vault" }], titlesOnly: true, maxResults: 8 },
        activeWorkspaceId: workspaceId,
        vaultSubdir: vaultSubdir ?? null,
      })
        .then((res) => {
          if (seq !== seqRef.current) return;
          setHits(res.filter((h) => !value.includes(h.path)));
          setActiveIdx(0);
          setOpen(true);
        })
        .catch(() => {});
    }, 200);
    return () => clearTimeout(t);
  }, [query, workspaceId, vaultSubdir, value]);

  function add(path: string) {
    if (!value.includes(path)) onChange([...value, path]);
    setQuery("");
    setHits([]);
    setOpen(false);
  }

  function remove(path: string) {
    onChange(value.filter((p) => p !== path));
  }

  const showList = open && hits.length > 0;

  return (
    <div className="grid gap-1.5">
      {value.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {value.map((p) => (
            <span
              key={p}
              className="inline-flex items-center gap-1 rounded-full border border-border/60 bg-secondary/40 px-2 py-0.5 text-[11px] text-foreground"
            >
              <FileText className="size-3 text-muted-foreground" />
              {noteName(p)}
              <button
                type="button"
                aria-label={`Remove ${noteName(p)}`}
                onClick={() => remove(p)}
                className="text-muted-foreground/70 outline-none hover:text-foreground focus-visible:text-foreground"
              >
                <X className="size-3" />
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="relative">
        <input
          role="combobox"
          aria-expanded={showList}
          aria-controls="vault-note-suggestions"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onBlur={() => setTimeout(() => setOpen(false), 120)}
          onKeyDown={(e) => {
            if (!showList) return;
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setActiveIdx((i) => Math.min(i + 1, hits.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setActiveIdx((i) => Math.max(i - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              const h = hits[activeIdx];
              if (h) add(h.path);
            } else if (e.key === "Escape") {
              e.stopPropagation();
              setOpen(false);
            }
          }}
          placeholder="Type to search vault notes…"
          className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-xs text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring"
        />
        {showList && (
          <div
            id="vault-note-suggestions"
            role="listbox"
            className="absolute inset-x-0 top-full z-10 mt-1 max-h-52 overflow-y-auto rounded-md border border-border bg-popover shadow-lg"
          >
            {hits.map((h, i) => (
              <button
                key={h.path}
                type="button"
                role="option"
                aria-selected={i === activeIdx}
                onMouseEnter={() => setActiveIdx(i)}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => add(h.path)}
                className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs outline-none ${
                  i === activeIdx ? "bg-secondary text-foreground" : "text-foreground/80 hover:bg-secondary/50"
                }`}
              >
                <FileText className="size-3 shrink-0 text-muted-foreground" />
                <span className="truncate">{h.title ?? noteName(h.path)}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
