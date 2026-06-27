import { cn } from "@/lib/utils";
import type { AccessMode, ReasoningEffort } from "@/lib/tauri";
import type { TurnState } from "@/hooks/useTurn";

export function StatusFooter({
  chatId,
  cwd,
  model,
  accessMode,
  reasoningEffort,
  state,
  status,
}: {
  chatId: string;
  cwd: string;
  model: string;
  accessMode: AccessMode;
  reasoningEffort: ReasoningEffort;
  state: TurnState;
  status: string;
}) {
  const shortId = chatId.length > 8 ? `${chatId.slice(0, 8)}…` : chatId;
  const shortCwd = cwd.replace(/^\/Users\/[^/]+/, "~");

  const stateLabel = state === "approval" ? "approval" : status;
  const stateColor =
    state === "running"
      ? "text-[var(--color-accent)]"
      : state === "approval"
        ? "text-[var(--color-amber)]"
        : state === "error"
          ? "text-[var(--color-amber)]"
          : "text-[var(--color-fg-dim)]";

  return (
    <div className="border-t border-[var(--color-line)] bg-[var(--color-bg-soft)]/60 px-5 py-2 font-mono text-[11px] text-[var(--color-fg-dim)]">
      <span className="text-[var(--color-fg-muted)]">cass</span>
      <span className="mx-1.5">·</span>
      <span>{accessMode}</span>
      <span className="mx-1.5">·</span>
      <span className={cn(stateColor)}>{stateLabel}</span>
      <span className="mx-1.5">·</span>
      <span>{model}</span>
      <span className="mx-1.5">·</span>
      <span>{shortCwd}</span>
      <span className="mx-1.5">·</span>
      <span>{shortId}</span>
      <span className="mx-1.5">·</span>
      <span>reasoning:{reasoningEffort}</span>
    </div>
  );
}
