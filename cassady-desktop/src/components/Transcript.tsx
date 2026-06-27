import { useEffect, useRef } from "react";
import { type TranscriptBlock } from "@/hooks/useTurn";
import { UserBlock } from "@/components/blocks/UserBlock";
import { AssistantBlock } from "@/components/blocks/AssistantBlock";
import { ReasoningBlock } from "@/components/blocks/ReasoningBlock";
import { ToolBlock } from "@/components/blocks/ToolBlock";
import { StatusBlock } from "@/components/blocks/StatusBlock";

export function Transcript({ blocks }: { blocks: TranscriptBlock[] }) {
  const endRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [blocks]);

  return (
    <div
      ref={containerRef}
      className="relative flex-1 overflow-y-auto px-5 py-4 font-mono text-[13px] leading-relaxed"
    >
      <div className="pointer-events-none absolute inset-0 z-0 opacity-[0.04] bg-[repeating-linear-gradient(to_bottom,transparent_0,transparent_2px,rgba(255,255,255,1)_2px,rgba(255,255,255,1)_3px)]" />
      <div className="relative z-10">
        {blocks.length === 0 ? (
          <div className="font-mono text-xs text-[var(--color-fg-dim)]">
            send a message to start.
          </div>
        ) : (
          blocks.map((b) => {
            switch (b.kind) {
              case "user":
                return <UserBlock key={b.id} text={b.text} />;
              case "assistant":
                return <AssistantBlock key={b.id} text={b.text} />;
              case "reasoning":
                return <ReasoningBlock key={b.id} text={b.text} />;
              case "tool":
                return <ToolBlock key={b.id} block={b} />;
              case "status":
                return <StatusBlock key={b.id} text={b.text} />;
              case "error":
                return <StatusBlock key={b.id} text={b.text} marker="✗" />;
              default:
                return null;
            }
          })
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}
