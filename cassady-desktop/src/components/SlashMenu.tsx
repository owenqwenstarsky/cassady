import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import type { AutoFillMenu } from "@/lib/tauri";

export function SlashMenu({
  menu,
  selected,
  onSelect,
  onHover,
}: {
  menu: AutoFillMenu;
  selected: number;
  onSelect: (index: number) => void;
  onHover: (index: number) => void;
}) {
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-index="${selected}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  return (
    <div className="absolute bottom-full left-0 z-40 mb-2 w-[min(90vw,30rem)] border border-[var(--color-line)] bg-[var(--color-bg-elev)] shadow-[0_24px_60px_-20px_rgba(0,0,0,0.7)]">
      <div className="flex items-center justify-between border-b border-[var(--color-line)] px-3 py-1.5">
        <span className="font-mono text-[10px] uppercase tracking-[0.2em] text-[var(--color-fg-dim)]">
          {menu.title}
        </span>
        <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-[var(--color-fg-dim)]">
          {menu.items.length}
        </span>
      </div>
      <div ref={listRef} className="max-h-60 overflow-y-auto py-1">
        {menu.items.map((item, idx) => {
          const active = idx === selected;
          return (
            <button
              key={idx}
              type="button"
              data-index={idx}
              onClick={() => onSelect(idx)}
              onMouseEnter={() => onHover(idx)}
              className={cn(
                "flex w-full flex-col gap-0.5 px-3 py-2 text-left transition-colors",
                active && "bg-[var(--color-accent-glow)]",
              )}
            >
              <span className="truncate font-mono text-xs text-[var(--color-fg)]">
                {item.label}
              </span>
              {item.detail && (
                <span className="truncate font-mono text-[10px] text-[var(--color-fg-dim)]">
                  {item.detail}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
