import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";

export function AssistantBlock({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  return (
    <div className={cn("py-1.5", className)}>
      <span className="text-[var(--color-accent)] text-glow">cass</span>
      <div className="mt-1 pl-4 text-[var(--color-fg)] prose-cass">
        {text ? (
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              code({ className: c, children, ...props }) {
                const isInline = !c;
                return isInline ? (
                  <code
                    className="font-mono text-[0.85em] text-[var(--color-accent)] bg-[var(--color-bg-soft)] px-1 py-0.5 rounded-[3px]"
                    {...props}
                  >
                    {children}
                  </code>
                ) : (
                  <code className={cn("font-mono", c)} {...props}>
                    {children}
                  </code>
                );
              },
              pre({ children }) {
                return (
                  <pre className="my-2 overflow-x-auto border border-[var(--color-line)] bg-[var(--color-bg-soft)] p-3 text-[13px] font-mono text-[var(--color-fg-muted)] rounded-[6px]">
                    {children}
                  </pre>
                );
              },
              a({ children, href }) {
                return (
                  <a
                    href={href}
                    target="_blank"
                    rel="noreferrer"
                    className="text-[var(--color-accent)] underline underline-offset-2 hover:opacity-80"
                  >
                    {children}
                  </a>
                );
              },
            }}
          >
            {text}
          </ReactMarkdown>
        ) : (
          <span className="text-[var(--color-fg-dim)]">…</span>
        )}
      </div>
    </div>
  );
}
