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
import {
  applyProviderLogin,
  listProviderCatalog,
  type ProviderApplyResult,
  type ProviderCatalogEntry,
  type SetupSelection,
} from "@/lib/tauri";

const CHATGPT_CODEX_ID = "chatgpt-codex";

export function LoginModal({
  onResolved,
  onClose,
}: {
  onResolved: (result: ProviderApplyResult) => void;
  onClose: () => void;
}) {
  const [catalog, setCatalog] = useState<ProviderCatalogEntry[]>([]);
  const [selection, setSelection] = useState<SetupSelection | null>(null);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        setCatalog(await listProviderCatalog());
      } catch {
        setCatalog([]);
      }
    })();
  }, []);

  const isCodex = selection?.providerId === CHATGPT_CODEX_ID;

  const apply = async () => {
    if (!selection) return;
    if (!selection.providerId.trim() || !selection.modelId.trim()) {
      setError("provider id and model id are required");
      return;
    }
    if (!isCodex && !selection.apiKeyEnv.trim()) {
      setError("API key environment variable is required for this provider");
      return;
    }
    setApplying(true);
    setError(null);
    try {
      const result = await applyProviderLogin([selection], 0);
      onResolved(result);
    } catch (e) {
      setError(String(e));
      setApplying(false);
    }
  };

  const pick = (entry: ProviderCatalogEntry) => {
    setError(null);
    setSelection({
      providerId: entry.id,
      providerName: entry.name,
      baseUrl: entry.baseUrl,
      apiKeyEnv: entry.apiKeyEnv,
      modelId: "",
      supportsTools: true,
      supportsReasoning: false,
    });
  };

  const update = (patch: Partial<SetupSelection>) => {
    setSelection((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <Card className="w-full max-w-lg">
        <CardHeader>
          <CardTitle className="text-[var(--color-amber)]">login</CardTitle>
          <CardDescription>
            {selection ? "configure provider and model" : "choose a provider"}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!selection ? (
            <div className="grid max-h-80 grid-cols-1 gap-1 overflow-y-auto">
              {catalog.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  onClick={() => pick(entry)}
                  className="flex flex-col gap-0.5 border border-[var(--color-line)] px-3 py-2 text-left transition-colors hover:border-[var(--color-accent)]/50"
                >
                  <span className="font-mono text-xs text-[var(--color-fg)]">
                    {entry.name}
                  </span>
                  <span className="font-mono text-[10px] text-[var(--color-fg-dim)]">
                    {entry.id} · {entry.baseUrl}
                  </span>
                </button>
              ))}
              {catalog.length === 0 && (
                <p className="font-mono text-xs text-[var(--color-fg-dim)]">
                  provider catalog unavailable.
                </p>
              )}
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              <Field label="provider id">
                <input
                  value={selection.providerId}
                  onChange={(e) => update({ providerId: e.target.value })}
                  className={inputClass}
                />
              </Field>
              <Field label="base url">
                <input
                  value={selection.baseUrl}
                  onChange={(e) => update({ baseUrl: e.target.value })}
                  className={inputClass}
                />
              </Field>
              <Field label="api key env">
                <input
                  value={selection.apiKeyEnv}
                  onChange={(e) => update({ apiKeyEnv: e.target.value })}
                  placeholder={isCodex ? "uses local codex auth" : "$MY_API_KEY"}
                  disabled={isCodex}
                  className={cn(inputClass, isCodex && "opacity-50")}
                />
              </Field>
              <Field label="model id">
                <input
                  value={selection.modelId}
                  onChange={(e) => update({ modelId: e.target.value })}
                  placeholder="e.g. gpt-5.5"
                  className={inputClass}
                />
              </Field>
            </div>
          )}
          {error && (
            <p className="mt-3 whitespace-pre-wrap font-mono text-xs text-[var(--color-amber)]">
              {error}
            </p>
          )}
        </CardContent>
        <CardFooter className="justify-end gap-3">
          <Button
            variant="outline"
            onClick={() => {
              if (selection) setSelection(null);
              else onClose();
            }}
            disabled={applying}
          >
            {selection ? "back" : "cancel"}
          </Button>
          {selection && (
            <Button onClick={apply} disabled={applying}>
              apply
            </Button>
          )}
        </CardFooter>
      </Card>
    </div>
  );
}

const inputClass =
  "w-full border border-[var(--color-line)] bg-[var(--color-bg)] px-2 py-1.5 font-mono text-xs text-[var(--color-fg)] placeholder:text-[var(--color-fg-dim)] focus:border-[var(--color-accent)] focus:outline-none";

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-[var(--color-fg-dim)]">
        {label}
      </span>
      {children}
    </label>
  );
}
