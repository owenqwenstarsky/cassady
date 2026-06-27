import { cn } from "@/lib/utils";

export function UserBlock({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  return (
    <div className={cn("py-1.5", className)}>
      <span className="text-[var(--color-accent)]">› you</span>
      <div className="mt-0.5 pl-4 text-[var(--color-fg-muted)] whitespace-pre-wrap break-words">
        {text}
      </div>
    </div>
  );
}
