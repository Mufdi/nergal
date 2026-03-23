import { useEffect, useRef, useState } from "react";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { basicSetup } from "codemirror";
import { oneDark } from "@codemirror/theme-one-dark";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { rust } from "@codemirror/lang-rust";
import { searchKeymap } from "@codemirror/search";
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
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  function doSave() {
    const view = viewRef.current;
    if (!view || readOnly) return;
    const content = view.state.doc.toString();
    console.log("[CodeEditor] saving", { sessionId, filePath, contentLength: content.length });
    invoke<string>("write_file_content", { sessionId, path: filePath, content })
      .then((absPath) => {
        console.log("[CodeEditor] saved OK to", absPath);
        setSaved(true);
        setTimeout(() => setSaved(false), 1500);
      })
      .catch((err) => {
        console.error("[CodeEditor] save failed:", err);
        setError(String(err));
      });
  }

  // Listen for global save event (from Ctrl+S shortcut)
  useEffect(() => {
    const handler = () => doSave();
    document.addEventListener("cluihud:save-file", handler);
    return () => document.removeEventListener("cluihud:save-file", handler);
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let view: EditorView | null = null;

    invoke<string>("read_file_content", { sessionId, path: filePath })
      .then((content) => {
        const saveKeymap = keymap.of([
          {
            key: "Mod-s",
            run: (v) => {
              const doc = v.state.doc.toString();
              console.log("[CodeEditor:keymap] saving via Mod-s", { contentLength: doc.length });
              invoke<string>("write_file_content", { sessionId, path: filePath, content: doc })
                .then((absPath) => {
                  console.log("[CodeEditor:keymap] saved OK to", absPath);
                  setSaved(true);
                  setTimeout(() => setSaved(false), 1500);
                })
                .catch((err) => console.error("[CodeEditor:keymap] save failed:", err));
              return true;
            },
          },
        ]);

        const state = EditorState.create({
          doc: content,
          extensions: [
            basicSetup,
            oneDark,
            getLanguageExtension(filePath),
            saveKeymap,
            keymap.of(searchKeymap),
            EditorView.editable.of(!readOnly),
            EditorView.theme({
              "&": { height: "100%", fontSize: "12px" },
              ".cm-scroller": { overflow: "auto" },
            }),
          ],
        });

        view = new EditorView({ state, parent: container });
        viewRef.current = view;
        setLoading(false);
      })
      .catch((err) => {
        setError(String(err));
        setLoading(false);
      });

    return () => {
      view?.destroy();
      viewRef.current = null;
    };
  }, [filePath, sessionId, readOnly]);

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
