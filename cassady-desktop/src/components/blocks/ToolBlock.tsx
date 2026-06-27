import { cn } from "@/lib/utils";
import type { TranscriptBlock } from "@/hooks/useTurn";

export function ToolBlock({
  block,
  className,
}: {
  block: TranscriptBlock;
  className?: string;
}) {
  const ok = block.toolOk;
  const done = block.toolDone;
  const args = block.toolArguments
    ? JSON.stringify(block.toolArguments, null, 2)
    : "";
  return (
    <div className={cn("py-1.5", className)}>
      <div className="font-mono text-xs text-[var(--color-fg-dim)]">
        · {block.toolName}
        {done && (
          <span>
            {" "}
            <span
              className={
                ok
                  ? "text-[var(--color-accent)]"
                  : "text-[var(--color-amber)]"
              }
            >
              {ok ? "✓" : "✗"}
            </span>
          </span>
        )}
      </div>
      {args && (
        <pre className="mt-1 overflow-x-auto pl-4 font-mono text-xs text-[var(--color-fg-dim)] whitespace-pre-wrap break-words">
          {args}
        </pre>
      )}
      {block.toolOutput && (
        <pre className="mt-1 overflow-x-auto pl-4 font-mono text-xs text-[var(--color-fg-muted)] whitespace-pre-wrap break-words">
          {block.toolOutput}
        </pre>
      )}
      {!done && block.text && (
        <pre className="mt-1 overflow-x-auto pl-4 font-mono text-xs text-[var(--color-fg-dim)] whitespace-pre-wrap break-words">
          {block.text}
        </pre>
      )}
    </div>
  );
}
