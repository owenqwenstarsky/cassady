import { cn } from "@/lib/utils";

export function StatusBlock({
  text,
  marker = "·",
  className,
}: {
  text: string;
  marker?: string;
  className?: string;
}) {
  return (
    <div className={cn("whitespace-pre-wrap py-1 font-mono text-xs text-[var(--color-amber)]", className)}>
      {marker} {text}
    </div>
  );
}
