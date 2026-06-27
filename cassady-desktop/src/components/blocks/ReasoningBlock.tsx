import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

export function ReasoningBlock({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className={cn("py-1", className)}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1 font-mono text-xs text-[var(--color-fg-dim)] hover:text-[var(--color-fg-muted)] transition-colors"
      >
        <ChevronRight
          className={cn(
            "h-3 w-3 transition-transform duration-200",
            open && "rotate-90",
          )}
        />
        reasoning
      </button>
      {open && (
        <div className="mt-1 pl-4 font-mono text-xs text-[var(--color-fg-dim)] whitespace-pre-wrap break-words">
          {text}
        </div>
      )}
    </div>
  );
}
