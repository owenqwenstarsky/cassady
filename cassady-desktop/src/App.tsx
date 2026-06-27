import { useEffect, useState } from "react";
import { ChatShell } from "@/components/ChatShell";
import { getCwd } from "@/lib/tauri";

export default function App() {
  const [cwd, setCwd] = useState<string>("");

  useEffect(() => {
    void (async () => {
      try {
        const c = await getCwd();
        setCwd(c);
      } catch {
        setCwd("");
      }
    })();
  }, []);

  if (!cwd) {
    return (
      <div className="vignette scanlines grain flex h-screen w-screen items-center justify-center bg-[var(--color-bg)] font-mono text-xs text-[var(--color-fg-dim)]">
        loading…
      </div>
    );
  }

  return <ChatShell cwd={cwd} />;
}
