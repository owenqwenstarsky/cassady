import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { ModelSelector } from "@/components/ModelSelector";
import { ArrowUp, Square } from "lucide-react";
import { cn } from "@/lib/utils";
import type { AccessMode, ModelOption, ReasoningEffort } from "@/lib/tauri";

export function Composer({
  disabled,
  running,
  onSend,
  onCancel,
  accessMode,
  reasoningEffort,
  model,
  modelOptions,
  onCycleAccessMode,
  onCycleReasoning,
  onSelectModel,
}: {
  disabled?: boolean;
  running?: boolean;
  onSend: (message: string) => void;
  onCancel: () => void;
  accessMode: AccessMode;
  reasoningEffort: ReasoningEffort;
  model: string;
  modelOptions: ModelOption[];
  onCycleAccessMode: () => void;
  onCycleReasoning: () => void;
  onSelectModel: (model: string) => void;
}) {
  const [value, setValue] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [value]);

  const submit = () => {
    const trimmed = value.trim();
    if (!trimmed || running) return;
    onSend(trimmed);
    setValue("");
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !(e.shiftKey || e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape" && running) {
      e.preventDefault();
      onCancel();
    }
  };

  return (
    <div className="border-t border-[var(--color-line)] bg-[var(--color-bg-soft)]/60 px-4 py-3">
      <div className="mb-2 flex items-center gap-2 font-mono text-[11px] text-[var(--color-fg-dim)]">
        <button
          type="button"
          onClick={onCycleAccessMode}
          disabled={running}
          className="border border-[var(--color-line)] px-2 py-0.5 uppercase tracking-[0.16em] hover:border-[var(--color-accent)]/50 hover:text-[var(--color-accent)] disabled:opacity-50 disabled:hover:border-[var(--color-line)] disabled:hover:text-[var(--color-fg-dim)] transition-colors"
        >
          {accessMode}
        </button>
        <button
          type="button"
          onClick={onCycleReasoning}
          disabled={running}
          className="border border-[var(--color-line)] px-2 py-0.5 uppercase tracking-[0.16em] hover:border-[var(--color-accent)]/50 hover:text-[var(--color-accent)] disabled:opacity-50 disabled:hover:border-[var(--color-line)] disabled:hover:text-[var(--color-fg-dim)] transition-colors"
        >
          reasoning:{reasoningEffort}
        </button>
        <ModelSelector
          model={model}
          modelOptions={modelOptions}
          disabled={running}
          onSelect={onSelectModel}
        />
      </div>
      <div className="flex items-end gap-2">
        <span className="font-mono text-[var(--color-accent)] pb-2">›</span>
        <textarea
          ref={ref}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onKeyDown}
          disabled={disabled}
          rows={1}
          placeholder="message cass…"
          className={cn(
            "flex-1 resize-none bg-transparent font-mono text-sm text-[var(--color-fg)] placeholder:text-[var(--color-fg-dim)] focus:outline-none",
            "py-2",
          )}
        />
        {running ? (
          <Button size="sm" variant="outline" onClick={onCancel}>
            <Square className="h-3 w-3" />
            stop
          </Button>
        ) : (
          <Button size="sm" onClick={submit} disabled={disabled || !value.trim()}>
            <ArrowUp className="h-3 w-3" />
            send
          </Button>
        )}
      </div>
    </div>
  );
}

