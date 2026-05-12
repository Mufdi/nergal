import { useEffect, useRef, useState } from "react";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { basicSetup } from "codemirror";
import { syntaxHighlighting } from "@codemirror/language";
import { currentHighlightStyle, useThemeName } from "@/lib/codemirrorHighlight";

const cluihudTheme = EditorView.theme({
  "&": {
    backgroundColor: "var(--card)",
    color: "var(--foreground)",
    height: "100%",
  },
  ".cm-scroller": { overflow: "auto" },
  ".cm-content": { caretColor: "#f97316" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#f97316" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "rgba(249, 115, 22, 0.2)",
  },
  ".cm-activeLine": { backgroundColor: "var(--cm-active-line)" },
  ".cm-gutters": {
    backgroundColor: "var(--card)",
    color: "var(--muted-foreground)",
    borderRight: "1px solid var(--border)",
  },
  ".cm-activeLineGutter": { backgroundColor: "var(--cm-active-line-gutter)" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 8px 0 4px" },
});
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { rust } from "@codemirror/lang-rust";
import { searchKeymap } from "@codemirror/search";
import { indentWithTab } from "@codemirror/commands";
import { invoke } from "@/lib/tauri";
import { Loader2 } from "lucide-react";

function getLanguageExtension(filePath: string) {
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "tsx":
      return javascript({ typescript: true, jsx: ext === "tsx" });
    case "js":
    case "jsx":
      return javascript({ jsx: ext === "jsx" });
    case "json":
      return json();
    case "md":
    case "mdx":
      return markdown();
    case "css":
    case "scss":
      return css();
    case "html":
    case "htm":
      return html();
    case "rs":
      return rust();
    default:
      return [];
  }
}

interface CodeEditorProps {
  filePath: string;
  sessionId: string;
  readOnly?: boolean;
}

export function CodeEditor({ filePath, sessionId, readOnly = false }: CodeEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const propsRef = useRef({ filePath, sessionId });
  propsRef.current = { filePath, sessionId };
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const theme = useThemeName();
  // Survives the destroy/recreate cycle triggered by a theme swap so the
  // user doesn't lose unsaved edits when toggling v1-light <-> v1-dark.
  // Keyed by file identity — when filePath/sessionId change we drop the
  // stash and re-read from disk.
  const docStashRef = useRef<{ key: string; doc: string } | null>(null);

  function saveFromView(v: EditorView) {
    if (readOnly) return;
    const { sessionId: sid, filePath: fp } = propsRef.current;
    const content = v.state.doc.toString();
    invoke<string>("write_file_content", { sessionId: sid, path: fp, content })
      .then(() => {
        setSaved(true);
        setTimeout(() => setSaved(false), 1500);
      })
      .catch((err) => {
        console.error("[CodeEditor] save failed:", err);
        setError(String(err));
      });
  }

  // Listen for global save event (from Ctrl+S when focus is NOT in editor)
  useEffect(() => {
    function handler() {
      const view = viewRef.current;
      if (view) saveFromView(view);
    }
    document.addEventListener("cluihud:save-file", handler);
    return () => document.removeEventListener("cluihud:save-file", handler);
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let cancelled = false;
    const stashKey = `${sessionId}::${filePath}`;

    // Destroy any leftover EditorView in the container
    if (viewRef.current) {
      viewRef.current.destroy();
      viewRef.current = null;
    }
    container.replaceChildren();

    function buildView(content: string) {
      if (cancelled || !container) return;
      const saveKeymap = keymap.of([
        {
          key: "Mod-s",
          run: (v) => {
            saveFromView(v);
            return true;
          },
        },
      ]);

      const state = EditorState.create({
        doc: content,
        extensions: [
          saveKeymap,
          basicSetup,
          cluihudTheme,
          syntaxHighlighting(currentHighlightStyle()),
          getLanguageExtension(filePath),
          keymap.of([indentWithTab, ...searchKeymap]),
          EditorView.editable.of(!readOnly),
          EditorView.theme({
            "&": { height: "100%", fontSize: "12px" },
            ".cm-scroller": { overflow: "auto" },
          }),
        ],
      });

      const view = new EditorView({ state, parent: container });
      viewRef.current = view;
      view.focus();
      setLoading(false);
    }

    const stashed = docStashRef.current;
    if (stashed && stashed.key === stashKey) {
      docStashRef.current = null;
      buildView(stashed.doc);
    } else {
      docStashRef.current = null;
      invoke<string>("read_file_content", { sessionId, path: filePath })
        .then((content) => buildView(content))
        .catch((err) => {
          if (!cancelled) {
            setError(String(err));
            setLoading(false);
          }
        });
    }

    return () => {
      cancelled = true;
      if (viewRef.current) {
        docStashRef.current = {
          key: stashKey,
          doc: viewRef.current.state.doc.toString(),
        };
        viewRef.current.destroy();
        viewRef.current = null;
      }
      container.replaceChildren();
    };
  }, [filePath, sessionId, readOnly, theme]);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-destructive">{error}</p>
      </div>
    );
  }

  return (
    <div className="relative h-full w-full overflow-hidden">
      {loading && (
        <div className="absolute inset-0 flex items-center justify-center bg-card">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      )}
      {saved && (
        <div className="absolute right-2 top-2 z-10 rounded bg-green-500/20 px-2 py-0.5 text-[10px] text-green-400">
          Saved
        </div>
      )}
      <div ref={containerRef} className="h-full w-full" />
    </div>
  );
}
