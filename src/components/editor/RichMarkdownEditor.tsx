import { useRef, useEffect } from "react";
import {
  MDXEditor,
  headingsPlugin,
  listsPlugin,
  quotePlugin,
  linkPlugin,
  tablePlugin,
  thematicBreakPlugin,
  codeBlockPlugin,
  markdownShortcutPlugin,
  diffSourcePlugin,
  toolbarPlugin,
  DiffSourceToggleWrapper,
  UndoRedo,
  BoldItalicUnderlineToggles,
  ListsToggle,
  BlockTypeSelect,
  InsertTable,
  type MDXEditorMethods,
} from "@mdxeditor/editor";
import "@mdxeditor/editor/style.css";
import "./dark-editor.css";

interface RichMarkdownEditorProps {
  markdown: string;
  onChange: (value: string) => void;
  onSave?: () => void;
  readOnly?: boolean;
  placeholder?: string;
  className?: string;
}

export function RichMarkdownEditor({
  markdown,
  onChange,
  onSave,
  readOnly = false,
  placeholder,
  className,
}: RichMarkdownEditorProps) {
  const editorRef = useRef<MDXEditorMethods>(null);

  // Sync external markdown changes (e.g. file reload)
  useEffect(() => {
    const current = editorRef.current?.getMarkdown() ?? "";
    if (current !== markdown) {
      editorRef.current?.setMarkdown(markdown);
    }
  }, [markdown]);

  // Ctrl+S save handler
  useEffect(() => {
    if (!onSave) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        onSave?.();
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onSave]);

  return (
    <MDXEditor
      ref={editorRef}
      className={`dark-theme dark-editor ${className ?? ""}`}
      contentEditableClassName="prose-invert max-w-none text-[12px] leading-relaxed text-foreground px-3 py-2 min-h-[200px] outline-none"
      markdown={markdown}
      onChange={onChange}
      readOnly={readOnly}
      placeholder={placeholder}
      plugins={[
        headingsPlugin(),
        listsPlugin(),
        quotePlugin(),
        linkPlugin(),
        tablePlugin(),
        thematicBreakPlugin(),
        codeBlockPlugin({ defaultCodeBlockLanguage: "" }),
        markdownShortcutPlugin(),
        diffSourcePlugin({ viewMode: "rich-text" }),
        ...(readOnly
          ? []
          : [
              toolbarPlugin({
                toolbarContents: () => (
                  <DiffSourceToggleWrapper>
                    <UndoRedo />
                    <BoldItalicUnderlineToggles />
                    <ListsToggle />
                    <BlockTypeSelect />
                    <InsertTable />
                  </DiffSourceToggleWrapper>
                ),
              }),
            ]),
      ]}
    />
  );
}
