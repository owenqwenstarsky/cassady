import { useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, Search, Sparkles } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ModelOption } from "@/lib/tauri";

interface ModelSelectorProps {
  model: string;
  modelOptions: ModelOption[];
  disabled?: boolean;
  onSelect: (model: string) => void;
}

function modelLabel(option: ModelOption): string {
  return option.displayName && option.displayName !== option.id
    ? option.displayName
    : option.id;
}

type Entry =
  | { kind: "header"; provider: string }
  | { kind: "item"; option: ModelOption; index: number };

export function ModelSelector({
  model,
  modelOptions,
  disabled,
  onSelect,
}: ModelSelectorProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const current = modelOptions.find((o) => o.id === model);

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = modelOptions.filter((o) => {
      if (!q) return true;
      return (
        o.id.toLowerCase().includes(q) ||
        o.provider.toLowerCase().includes(q) ||
        (o.displayName ?? "").toLowerCase().includes(q)
      );
    });
    const map = new Map<string, ModelOption[]>();
    for (const o of filtered) {
      const arr = map.get(o.provider) ?? [];
      arr.push(o);
      map.set(o.provider, arr);
    }
    return Array.from(map.entries()).sort((a, b) =>
      a[0].localeCompare(b[0]),
    );
  }, [modelOptions, query]);

  const flat = useMemo(
    () => grouped.flatMap(([, opts]) => opts),
    [grouped],
  );

  const entries = useMemo<Entry[]>(() => {
    const out: Entry[] = [];
    let idx = 0;
    for (const [provider, opts] of grouped) {
      out.push({ kind: "header", provider });
      for (const option of opts) {
        out.push({ kind: "item", option, index: idx++ });
      }
    }
    return out;
  }, [grouped]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  useEffect(() => {
    if (open) {
      const i = flat.findIndex((o) => o.id === model);
      setActiveIndex(i >= 0 ? i : 0);
      const id = requestAnimationFrame(() => inputRef.current?.focus());
      return () => cancelAnimationFrame(id);
    }
    setQuery("");
  }, [open]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    if (!open) return;
    const el = listRef.current?.querySelector(
      `[data-index="${activeIndex}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  const select = (id: string) => {
    onSelect(id);
    setOpen(false);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, flat.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = flat[activeIndex];
      if (opt) select(opt.id);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
    }
  };

  const triggerLabel = current ? modelLabel(current) : model || "no model";
  const isEmpty = modelOptions.length === 0;

  return (
    <div ref={wrapperRef} className="relative">
      <button
        type="button"
        onClick={() => !disabled && setOpen((o) => !o)}
        disabled={disabled || isEmpty}
        title="model"
        className={cn(
          "group flex items-center gap-1.5 border px-2 py-0.5 font-mono text-[11px] transition-colors",
          "border-[var(--color-line)] text-[var(--color-fg-dim)]",
          "hover:border-[var(--color-accent)]/50 hover:text-[var(--color-fg)]",
          "focus:outline-none focus:border-[var(--color-accent)]",
          "disabled:opacity-50 disabled:hover:border-[var(--color-line)] disabled:hover:text-[var(--color-fg-dim)]",
          open && "border-[var(--color-accent)]/60 text-[var(--color-fg)]",
        )}
      >
        {current?.reasoningSupported && (
          <Sparkles
            className={cn(
              "h-2.5 w-2.5 shrink-0",
              current.reasoningRequired
                ? "text-[var(--color-amber)]"
                : "text-[var(--color-accent)]/70",
            )}
          />
        )}
        <span className="max-w-[min(40vw,22rem)] truncate tracking-tight">
          {triggerLabel}
        </span>
        <ChevronDown
          className={cn(
            "h-3 w-3 shrink-0 transition-transform",
            open && "rotate-180",
          )}
        />
      </button>

      {open && (
        <div className="absolute bottom-full left-0 z-40 mb-2 w-[min(90vw,28rem)] border border-[var(--color-line)] bg-[var(--color-bg-elev)] shadow-[0_24px_60px_-20px_rgba(0,0,0,0.7)]">
          <div className="flex items-center gap-2 border-b border-[var(--color-line)] px-3 py-2">
            <Search className="h-3 w-3 shrink-0 text-[var(--color-fg-dim)]" />
            <input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onKeyDown}
              placeholder="filter models…"
              className="flex-1 bg-transparent font-mono text-xs text-[var(--color-fg)] placeholder:text-[var(--color-fg-dim)] focus:outline-none"
            />
            <span className="shrink-0 font-mono text-[10px] uppercase tracking-[0.16em] text-[var(--color-fg-dim)]">
              {flat.length}
            </span>
          </div>
          <div ref={listRef} className="max-h-72 overflow-y-auto py-1">
            {flat.length === 0 ? (
              <div className="px-3 py-6 text-center font-mono text-xs text-[var(--color-fg-dim)]">
                no models match.
              </div>
            ) : (
              entries.map((entry) => {
                if (entry.kind === "header") {
                  return (
                    <div
                      key={`h:${entry.provider}`}
                      className="sticky top-0 bg-[var(--color-bg-elev)] px-3 py-1.5 font-mono text-[10px] uppercase tracking-[0.2em] text-[var(--color-fg-dim)]"
                    >
                      {entry.provider}
                    </div>
                  );
                }
                const opt = entry.option;
                const isActive = entry.index === activeIndex;
                const isSelected = opt.id === model;
                const showId =
                  opt.displayName && opt.displayName !== opt.id;
                return (
                  <button
                    key={`${opt.provider}:${opt.id}`}
                    type="button"
                    data-index={entry.index}
                    onClick={() => select(opt.id)}
                    onMouseEnter={() => setActiveIndex(entry.index)}
                    className={cn(
                      "flex w-full items-center gap-2 px-3 py-2 text-left transition-colors",
                      isActive && "bg-[var(--color-accent-glow)]",
                    )}
                  >
                    <span className="flex h-3 w-3 shrink-0 items-center justify-center">
                      {isSelected && (
                        <Check className="h-3 w-3 text-[var(--color-accent)]" />
                      )}
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate font-mono text-xs text-[var(--color-fg)]">
                        {modelLabel(opt)}
                      </span>
                      {showId && (
                        <span className="truncate font-mono text-[10px] text-[var(--color-fg-dim)]">
                          {opt.id}
                        </span>
                      )}
                    </span>
                    {opt.reasoningSupported && (
                      <span
                        className={cn(
                          "shrink-0 font-mono text-[10px] uppercase tracking-[0.16em]",
                          opt.reasoningRequired
                            ? "text-[var(--color-amber)]"
                            : "text-[var(--color-accent)]/60",
                        )}
                      >
                        reason
                      </span>
                    )}
                  </button>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
