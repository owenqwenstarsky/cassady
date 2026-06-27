import { useState } from "react";
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
import { removeProviders, type LogoutResult, type ProviderLogoutCandidate } from "@/lib/tauri";

export function LogoutModal({
  candidates,
  onResolved,
  onClose,
}: {
  candidates: ProviderLogoutCandidate[];
  onResolved: (result: LogoutResult) => void;
  onClose: () => void;
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const apply = async () => {
    if (selected.size === 0) return;
    setApplying(true);
    setError(null);
    try {
      const result = await removeProviders([...selected]);
      onResolved(result);
    } catch (e) {
      setError(String(e));
      setApplying(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <Card className="w-full max-w-lg">
        <CardHeader>
          <CardTitle className="text-[var(--color-amber)]">logout</CardTitle>
          <CardDescription>
            remove saved providers and their models
          </CardDescription>
        </CardHeader>
        <CardContent>
          {candidates.length === 0 ? (
            <p className="font-mono text-xs text-[var(--color-fg-dim)]">
              no providers configured.
            </p>
          ) : (
            <div className="flex flex-col gap-1">
              {candidates.map((c) => {
                const checked = selected.has(c.id);
                return (
                  <button
                    key={c.id}
                    type="button"
                    onClick={() => toggle(c.id)}
                    className={cn(
                      "flex items-center gap-2 border px-3 py-2 text-left transition-colors",
                      checked
                        ? "border-[var(--color-accent)]/60 bg-[var(--color-accent-glow)]"
                        : "border-[var(--color-line)] hover:border-[var(--color-accent)]/40",
                    )}
                  >
                    <span
                      className={cn(
                        "flex h-3 w-3 shrink-0 items-center justify-center border text-[10px]",
                        checked
                          ? "border-[var(--color-accent)] text-[var(--color-accent)]"
                          : "border-[var(--color-fg-dim)]",
                      )}
                    >
                      {checked ? "✓" : ""}
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate font-mono text-xs text-[var(--color-fg)]">
                        {c.id}
                      </span>
                      {c.name && (
                        <span className="truncate font-mono text-[10px] text-[var(--color-fg-dim)]">
                          {c.name}
                        </span>
                      )}
                    </span>
                    <span className="shrink-0 font-mono text-[10px] text-[var(--color-fg-dim)]">
                      {c.modelCount} models
                    </span>
                  </button>
                );
              })}
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
          <Button
            onClick={apply}
            disabled={applying || selected.size === 0}
          >
            remove {selected.size > 0 ? `(${selected.size})` : ""}
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}
