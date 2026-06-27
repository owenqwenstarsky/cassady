import { useEffect, useState } from "react";
import logo from "@/assets/cass-logo-transparent.png";
import { Button } from "@/components/ui/button";
import { Plus, FolderOpen, TerminalSquare } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ChatSummary, ConversationInfo } from "@/lib/tauri";
import { listChats, newSession, resumeSession } from "@/lib/tauri";

interface TopBarProps {
  chat: ConversationInfo | null;
  onNewChat: () => void;
  onOpenChat: () => void;
  chats: ChatSummary[];
  cwd: string;
}

export function TopBar({ chat, onNewChat, onOpenChat }: TopBarProps) {
  return (
    <header className="sticky top-0 z-30 flex h-14 items-center justify-between border-b border-[var(--color-line)] bg-[var(--color-bg)]/85 px-4 backdrop-blur-md">
      <div className="flex items-center gap-2">
        <img src={logo} alt="cass" className="h-6 w-6" />
        <span className="font-mono text-sm font-medium tracking-tight text-[var(--color-fg)]">
          cassady
        </span>
        {chat && (
          <span className="ml-3 font-mono text-xs text-[var(--color-fg-dim)]">
            {chat.id.slice(0, 8)}…
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        <Button size="sm" variant="ghost" onClick={onNewChat}>
          <Plus className="h-3 w-3" />
          new
        </Button>
        <Button size="sm" variant="ghost" onClick={onOpenChat}>
          <FolderOpen className="h-3 w-3" />
          open
        </Button>
      </div>
    </header>
  );
}

export function OpenChatList({
  chats,
  onPick,
  onClose,
}: {
  chats: ChatSummary[];
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className={cn(
          "w-full max-w-md border border-[var(--color-line)] bg-[var(--color-bg-soft)]",
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-[var(--color-line)] px-4 py-3 font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-fg-dim)]">
          open chat
        </div>
        <div className="max-h-80 overflow-y-auto">
          {chats.length === 0 ? (
            <div className="px-4 py-6 font-mono text-xs text-[var(--color-fg-dim)]">
              no chats for this directory.
            </div>
          ) : (
            chats.map((c) => (
              <button
                key={c.id}
                type="button"
                onClick={() => onPick(c.id)}
                className="flex w-full flex-col gap-0.5 border-b border-[var(--color-line)] px-4 py-3 text-left hover:bg-[var(--color-bg-elev)]/50 transition-colors"
              >
                <span className="font-mono text-xs text-[var(--color-fg)]">
                  {c.id}
                </span>
                <span className="font-mono text-xs text-[var(--color-fg-dim)]">
                  {c.model} · {c.firstUserPreview.slice(0, 60) || "(empty)"}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

export function NoConfigCard({ cwd }: { cwd: string }) {
  return (
    <div className="flex flex-1 items-center justify-center p-6">
      <div className="max-w-md border border-[var(--color-line)] bg-[var(--color-bg-soft)]/60 p-8 text-center backdrop-blur-sm">
        <TerminalSquare className="mx-auto h-8 w-8 text-[var(--color-accent)]" />
        <h2 className="mt-4 font-mono text-base font-medium text-[var(--color-fg)]">
          run <span className="text-[var(--color-accent)]">cass setup</span> first
        </h2>
        <p className="mt-2 font-sans text-sm text-[var(--color-fg-muted)]">
          Cassady needs a provider and model configured under{" "}
          <code className="font-mono text-[var(--color-fg)]">~/.cass</code>.
          Run the setup wizard in a terminal:
        </p>
        <pre className="mt-4 overflow-x-auto border border-[var(--color-line)] bg-[var(--color-bg)] px-3 py-2 text-left font-mono text-xs text-[var(--color-fg)]">
          <span className="text-[var(--color-accent)]">▸</span> cass setup
        </pre>
        <p className="mt-4 font-mono text-xs text-[var(--color-fg-dim)]">
          cwd: {cwd.replace(/^\/Users\/[^/]+/, "~")}
        </p>
      </div>
    </div>
  );
}

export function useSessionManager(cwd: string) {
  const [chat, setChat] = useState<ConversationInfo | null>(null);
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [showOpen, setShowOpen] = useState(false);
  const [configError, setConfigError] = useState<string | null>(null);

  const refreshChats = async () => {
    try {
      setChats(await listChats(cwd));
    } catch {
      // ignore list errors
    }
  };

  useEffect(() => {
    void refreshChats();
  }, [cwd]);

  const handleNew = async () => {
    try {
      const info = await newSession({ cwd });
      setChat(info);
      setConfigError(null);
      await refreshChats();
    } catch (e) {
      setConfigError(String(e));
    }
  };

  const handleOpen = async () => {
    await refreshChats();
    setShowOpen(true);
  };

  const handlePick = async (id: string) => {
    setShowOpen(false);
    try {
      const info = await resumeSession(id, cwd);
      setChat(info);
      setConfigError(null);
    } catch (e) {
      setConfigError(String(e));
    }
  };

  return {
    chat,
    setChat,
    chats,
    showOpen,
    setShowOpen,
    configError,
    handleNew,
    handleOpen,
    handlePick,
    refreshChats,
  };
}
