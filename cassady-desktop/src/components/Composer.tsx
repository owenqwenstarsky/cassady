import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { ModelSelector } from "@/components/ModelSelector";
import { SlashMenu } from "@/components/SlashMenu";
import { ArrowUp, Square } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  slashAutofill,
  type AccessMode,
  type AutoFillMenu,
  type ModelOption,
  type ReasoningEffort,
} from "@/lib/tauri";

export function Composer({
  disabled,
  running,
  cwd,
  onSend,
  onCancel,
  onSlashCommand,
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
  cwd: string;
  onSend: (message: string) => void;
  onCancel: () => void;
  onSlashCommand: (input: string) => void;
  accessMode: AccessMode;
  reasoningEffort: ReasoningEffort;
  model: string;
  modelOptions: ModelOption[];
  onCycleAccessMode: () => void;
  onCycleReasoning: () => void;
  onSelectModel: (model: string) => void;
}) {
  const [value, setValue] = useState("");
  const [slashMenu, setSlashMenu] = useState<AutoFillMenu | null>(null);
  const [slashSelected, setSlashSelected] = useState(0);
  const ref = useRef<HTMLTextAreaElement>(null);
  const rowRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [value]);

  useEffect(() => {
    if (!value.startsWith("/") || value.includes("\n")) {
      setSlashMenu(null);
      return;
    }
    let active = true;
    void (async () => {
      try {
        const menu = await slashAutofill(value, cwd);
        if (active) {
          setSlashMenu(menu);
          setSlashSelected(menu?.selected ?? 0);
        }
      } catch {
        if (active) setSlashMenu(null);
      }
    })();
    return () => {
      active = false;
    };
  }, [value, cwd]);

  useEffect(() => {
    if (!slashMenu) return;
    const handler = (e: MouseEvent) => {
      if (rowRef.current && !rowRef.current.contains(e.target as Node)) {
        setSlashMenu(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [slashMenu]);

  const submit = () => {
    const trimmed = value.trim();
    if (!trimmed || running) return;
    onSend(trimmed);
    setValue("");
  };

  const applySlashSelection = () => {
    if (!slashMenu) return;
    const item = slashMenu.items[slashSelected];
    if (!item) return;
    const newInput =
      value.slice(0, slashMenu.replacementStart) +
      item.insert +
      value.slice(slashMenu.replacementEnd);
    if (item.insert.endsWith(" ")) {
      // Value command name selected (e.g. "/model "); keep the menu open for
      // argument selection.
      setValue(newInput);
    } else {
      // Complete command; auto-run and clear the input.
      setSlashMenu(null);
      setValue("");
      onSlashCommand(newInput.trim());
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (slashMenu) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSlashSelected((s) => Math.min(s + 1, slashMenu.items.length - 1));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSlashSelected((s) => Math.max(s - 1, 0));
        return;
      }
      if (e.key === "Enter" && !(e.shiftKey || e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        applySlashSelection();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setSlashMenu(null);
        return;
      }
    } else {
      if (e.key === "Enter" && !(e.shiftKey || e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        const trimmed = value.trim();
        if (trimmed.startsWith("/")) {
          setValue("");
          onSlashCommand(trimmed);
        } else {
          submit();
        }
        return;
      }
      if (e.key === "Escape" && running) {
        e.preventDefault();
        onCancel();
      }
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
      <div ref={rowRef} className="relative flex items-end gap-2">
        {slashMenu && (
          <SlashMenu
            menu={slashMenu}
            selected={slashSelected}
            onSelect={(idx) => {
              setSlashSelected(idx);
              applySlashSelection();
            }}
            onHover={setSlashSelected}
          />
        )}
        <span className="font-mono text-[var(--color-accent)] pb-2">›</span>
        <textarea
          ref={ref}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onKeyDown}
          disabled={disabled}
          rows={1}
          placeholder="message cass…  (type / for commands)"
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

