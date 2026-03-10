import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ComponentPropsWithoutRef } from "react";

interface MarkdownViewProps {
  content: string;
}

export function MarkdownView({ content }: MarkdownViewProps) {
  return (
    <div className="prose-invert max-w-none px-4 py-3 text-sm text-text">
      <Markdown
        remarkPlugins={[remarkGfm]}
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
          a: (props: ComponentPropsWithoutRef<"a">) => (
            <a className="text-accent underline" {...props} />
          ),
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
        }}
      >
        {content}
      </Markdown>
    </div>
  );
}
