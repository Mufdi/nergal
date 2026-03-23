import { useEffect, useRef, useCallback, useState } from "react";
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

  const saveFile = useCallback(async () => {
    const view = viewRef.current;
    if (!view || readOnly) return;
    const content = view.state.doc.toString();
    try {
      await invoke("write_file_content", { sessionId, path: filePath, content });
    } catch (err) {
      setError(String(err));
    }
  }, [filePath, sessionId, readOnly]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let view: EditorView | null = null;

    invoke<string>("read_file_content", { sessionId, path: filePath })
      .then((content) => {
        const state = EditorState.create({
          doc: content,
          extensions: [
            basicSetup,
            oneDark,
            getLanguageExtension(filePath),
            keymap.of([
              ...searchKeymap,
              {
                key: "Ctrl-s",
                run: () => { saveFile(); return true; },
              },
            ]),
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
  }, [filePath, sessionId, readOnly, saveFile]);

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
      <div ref={containerRef} className="h-full w-full" />
    </div>
  );
}
