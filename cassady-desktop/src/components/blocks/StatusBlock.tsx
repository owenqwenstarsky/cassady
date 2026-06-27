import { cn } from "@/lib/utils";

export function StatusBlock({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  return (
    <div className={cn("py-1 font-mono text-xs text-[var(--color-amber)]", className)}>
      · {text}
    </div>
  );
}
