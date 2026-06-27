import { useEffect, useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { BranchFamily, Checkpoint } from "@/lib/tauri";

export function BranchModal({
  family,
  onSwitch,
  onBranch,
  onClose,
}: {
  family: BranchFamily;
  onSwitch: (branchId: string) => void;
  onBranch: (checkpoint: Checkpoint, restoreFiles: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const [selectedCheckpoint, setSelectedCheckpoint] = useState<Checkpoint | null>(
    null,
  );
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !applying) {
        if (selectedCheckpoint) setSelectedCheckpoint(null);
        else onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [applying, selectedCheckpoint, onClose]);

  const runBranch = async (restoreFiles: boolean) => {
    if (!selectedCheckpoint) return;
    setApplying(true);
    setError(null);
    try {
      await onBranch(selectedCheckpoint, restoreFiles);
    } catch (e) {
      setError(String(e));
      setApplying(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <Card className="w-full max-w-lg">
        <CardHeader>
          <CardTitle className="text-[var(--color-amber)]">
            {selectedCheckpoint ? "branch from checkpoint" : "branch / restore"}
          </CardTitle>
          <CardDescription>
            {selectedCheckpoint
              ? `${selectedCheckpoint.chatId} · ${selectedCheckpoint.label}`
              : "switch to a branch or branch from a checkpoint"}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!selectedCheckpoint ? (
            <div className="flex flex-col gap-3">
              {family.branches.length > 0 && (
                <div>
                  <p className="mb-1 font-mono text-[10px] uppercase tracking-[0.2em] text-[var(--color-fg-dim)]">
                    branches
                  </p>
                  <div className="flex flex-col gap-1">
                    {family.branches.map((b) => (
                      <button
                        key={b.id}
                        type="button"
                        onClick={() => onSwitch(b.id)}
                        disabled={applying || b.current}
                        className={cn(
                          "flex items-center justify-between border px-3 py-2 text-left transition-colors",
                          b.current
                            ? "border-[var(--color-line)] text-[var(--color-fg-dim)]"
                            : "border-[var(--color-line)] hover:border-[var(--color-accent)]/50 hover:text-[var(--color-fg)]",
                        )}
                      >
                        <span className="flex flex-col gap-0.5">
                          <span className="truncate font-mono text-xs text-[var(--color-fg)]">
                            {b.current ? `current · ${b.id}` : b.id}
                          </span>
                          {b.branchLabel && (
                            <span className="truncate font-mono text-[10px] text-[var(--color-fg-dim)]">
                              {b.branchLabel}
                            </span>
                          )}
                        </span>
                        <span className="shrink-0 font-mono text-[10px] text-[var(--color-fg-dim)]">
                          {b.recordCount} records
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              )}
              {family.checkpoints.length > 0 && (
                <div>
                  <p className="mb-1 font-mono text-[10px] uppercase tracking-[0.2em] text-[var(--color-fg-dim)]">
                    checkpoints
                  </p>
                  <div className="flex max-h-56 flex-col gap-1 overflow-y-auto">
                    {family.checkpoints.map((c) => (
                      <button
                        key={c.id}
                        type="button"
                        onClick={() => setSelectedCheckpoint(c)}
                        disabled={applying}
                        className="flex flex-col gap-0.5 border border-[var(--color-line)] px-3 py-2 text-left transition-colors hover:border-[var(--color-accent)]/50"
                      >
                        <span className="truncate font-mono text-xs text-[var(--color-fg)]">
                          {c.chatId} · {c.label}
                        </span>
                        {c.detail && (
                          <span className="truncate font-mono text-[10px] text-[var(--color-fg-dim)]">
                            {c.detail}
                          </span>
                        )}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              {family.branches.length === 0 &&
                family.checkpoints.length === 0 && (
                  <p className="font-mono text-xs text-[var(--color-fg-dim)]">
                    no branches or checkpoints found.
                  </p>
                )}
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <button
                type="button"
                onClick={() => setSelectedCheckpoint(null)}
                disabled={applying}
                className="self-start font-mono text-[10px] uppercase tracking-[0.16em] text-[var(--color-fg-dim)] hover:text-[var(--color-fg)]"
              >
                ‹ back
              </button>
              <button
                type="button"
                onClick={() => void runBranch(false)}
                disabled={applying}
                className="flex flex-col gap-0.5 border border-[var(--color-line)] px-3 py-2 text-left transition-colors hover:border-[var(--color-accent)]/50"
              >
                <span className="font-mono text-xs text-[var(--color-fg)]">
                  branch conversation only
                </span>
                <span className="font-mono text-[10px] text-[var(--color-fg-dim)]">
                  safe default; leaves files unchanged
                </span>
              </button>
              <button
                type="button"
                onClick={() => void runBranch(true)}
                disabled={applying}
                className="flex flex-col gap-0.5 border border-[var(--color-line)] px-3 py-2 text-left transition-colors hover:border-[var(--color-accent)]/50"
              >
                <span className="font-mono text-xs text-[var(--color-fg)]">
                  branch and restore tracked files
                </span>
                <span className="font-mono text-[10px] text-[var(--color-fg-dim)]">
                  applies safe write/edit snapshots; conflicts are skipped
                </span>
              </button>
            </div>
          )}
          {error && (
            <p className="mt-3 whitespace-pre-wrap font-mono text-xs text-[var(--color-amber)]">
              {error}
            </p>
          )}
        </CardContent>
        <CardFooter className="justify-end gap-3">
          <Button variant="outline" onClick={onClose} disabled={applying}>
            cancel
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}
