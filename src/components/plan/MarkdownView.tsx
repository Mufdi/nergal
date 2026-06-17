import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ComponentPropsWithoutRef } from "react";
import { useState } from "react";
import { useObsidianRemarkPlugin, isObsidianHref, openObsidianHref, obsidianUrlTransform } from "@/lib/markdown/obsidianMarkdown";

interface MarkdownViewProps {
  content: string;
  /// When set, obsidian:// wikilinks navigate in-place via this callback
  /// (unless Ctrl/Cmd is held → open in Obsidian). Other consumers omit it and
  /// keep the default open-in-Obsidian behavior.
  onWikilinkNavigate?: (href: string) => void;
  /// When true, remote <img> tags are replaced with a click-to-load placeholder.
  /// Prevents tracking pixels and SSRF from untrusted multi-writer content (e.g.
  /// Linear issue descriptions and comments). Default false.
  gateRemoteImages?: boolean;
}

/// Lazy image placeholder shown when gateRemoteImages=true. Loads the real src
/// only on explicit user click — no auto-fetch on render.
function GatedImage({ src, alt }: { src?: string; alt?: string }) {
  const [revealed, setRevealed] = useState(false);
  if (revealed && src) {
    return <img src={src} alt={alt} className="max-w-full rounded" />;
  }
  return (
    <button
      type="button"
      onClick={() => setRevealed(true)}
      className="inline-flex items-center gap-1 rounded border border-border/50 bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground hover:bg-secondary/70 hover:text-foreground transition-colors"
      title={src ?? "image"}
    >
      [image{alt ? `: ${alt}` : ""} — click to load]
    </button>
  );
}

export function MarkdownView({ content, onWikilinkNavigate, gateRemoteImages = false }: MarkdownViewProps) {
  const obsidianPlugin = useObsidianRemarkPlugin();
  return (
    <div className="prose-invert max-w-none px-4 py-3 text-[12px] text-text">
      <Markdown
        remarkPlugins={[remarkGfm, obsidianPlugin]}
        urlTransform={obsidianUrlTransform}
        components={{
          h1: (props: ComponentPropsWithoutRef<"h1">) => (
            <h1 className="mb-2 mt-4 border-b border-border pb-1 text-lg font-semibold text-text" {...props} />
          ),
          h2: (props: ComponentPropsWithoutRef<"h2">) => (
            <h2 className="mb-2 mt-3 text-base font-semibold text-text" {...props} />
          ),
          h3: (props: ComponentPropsWithoutRef<"h3">) => (
            <h3 className="mb-1 mt-2 text-sm font-semibold text-text" {...props} />
          ),
          p: (props: ComponentPropsWithoutRef<"p">) => (
            <p className="mb-2 leading-relaxed text-text" {...props} />
          ),
          ul: (props: ComponentPropsWithoutRef<"ul">) => (
            <ul className="mb-2 ml-4 list-disc text-text" {...props} />
          ),
          ol: (props: ComponentPropsWithoutRef<"ol">) => (
            <ol className="mb-2 ml-4 list-decimal text-text" {...props} />
          ),
          li: (props: ComponentPropsWithoutRef<"li">) => (
            <li className="mb-0.5 text-text" {...props} />
          ),
          code: ({ className, children, ...props }: ComponentPropsWithoutRef<"code">) => {
            const isBlock = className?.startsWith("language-");
            if (isBlock) {
              return (
                <pre className="my-2 overflow-x-auto bg-surface p-3 font-mono text-xs text-text">
                  <code className={className} {...props}>
                    {children}
                  </code>
                </pre>
              );
            }
            return (
              <code className="bg-surface-raised px-1 py-0.5 font-mono text-xs text-accent" {...props}>
                {children}
              </code>
            );
          },
          pre: ({ children }: ComponentPropsWithoutRef<"pre">) => <div>{children}</div>,
          a: ({ href, children, ...props }: ComponentPropsWithoutRef<"a">) => {
            if (isObsidianHref(href)) {
              return (
                <a
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    if (onWikilinkNavigate && !(e.metaKey || e.ctrlKey)) {
                      onWikilinkNavigate(href!);
                    } else {
                      openObsidianHref(href!);
                    }
                  }}
                  className="text-accent underline cursor-pointer"
                  role="link"
                >
                  {children}
                </a>
              );
            }
            return (
              <a href={href} className="text-accent underline" {...props}>
                {children}
              </a>
            );
          },
          blockquote: (props: ComponentPropsWithoutRef<"blockquote">) => (
            <blockquote className="my-2 border-l-2 border-accent pl-3 text-text-muted" {...props} />
          ),
          table: (props: ComponentPropsWithoutRef<"table">) => (
            <table className="my-2 w-full border-collapse text-xs" {...props} />
          ),
          th: (props: ComponentPropsWithoutRef<"th">) => (
            <th className="border border-border bg-surface-raised px-2 py-1 text-left font-medium" {...props} />
          ),
          td: (props: ComponentPropsWithoutRef<"td">) => (
            <td className="border border-border px-2 py-1" {...props} />
          ),
          ...(gateRemoteImages
            ? {
                img: ({ src, alt }: ComponentPropsWithoutRef<"img">) => (
                  <GatedImage src={src} alt={alt} />
                ),
              }
            : {}),
        }}
      >
        {content}
      </Markdown>
    </div>
  );
}
