import { useEffect, useMemo, useState } from "react";
import { TopBar, OpenChatList, NoConfigCard, useSessionManager } from "@/components/TopBar";
import { Transcript } from "@/components/Transcript";
import { Composer } from "@/components/Composer";
import { ACCESS_MODES, REASONING_EFFORTS } from "@/lib/sessionSettings";
import { StatusFooter } from "@/components/StatusFooter";
import { ApprovalDialog } from "@/components/ApprovalDialog";
import { useTurn } from "@/hooks/useTurn";
import type { AccessMode, ReasoningEffort } from "@/lib/tauri";
import { listModels, updateSessionSettings, type ModelOption } from "@/lib/tauri";

export function ChatShell({ cwd }: { cwd: string }) {
  const {
    chat,
    setChat,
    chats,
    showOpen,
    setShowOpen,
    configError,
    handleNew,
    handleOpen,
    handlePick,
  } = useSessionManager(cwd);
  const [modelOptions, setModelOptions] = useState<ModelOption[]>([]);

  const turn = useTurn(chat);

  useEffect(() => {
    if (chat) {
      const shortId = chat.id.slice(0, 8);
      document.title = `cass — ${shortId}`;
    } else {
      document.title = "cass";
    }
  }, [chat]);

  useEffect(() => {
    void (async () => {
      try {
        setModelOptions(await listModels());
      } catch {
        setModelOptions([]);
      }
    })();
  }, [chat?.id]);

  const accessMode = chat?.accessMode ?? "read-only";
  const reasoningEffort = chat?.reasoningEffort ?? "medium";
  const model = chat?.model ?? "";

  const updateSettings = async (settings: {
    accessMode?: AccessMode;
    model?: string;
    reasoningEffort?: ReasoningEffort;
  }) => {
    if (!chat) return null;
    if (turn.state === "running" || turn.state === "approval") {
      turn.setStatusHint("settings can be changed when idle");
      return null;
    }
    try {
      const info = await updateSessionSettings({ chatId: chat.id, ...settings });
      setChat(info);
      return info;
    } catch (e) {
      turn.setStatusHint(String(e));
      return null;
    }
  };

  const cycleAccessMode = () => {
    const idx = ACCESS_MODES.indexOf(accessMode);
    const next = ACCESS_MODES[(idx + 1) % ACCESS_MODES.length];
    void (async () => {
      const info = await updateSettings({ accessMode: next });
      if (info) turn.setStatusHint(`mode: ${info.accessMode}`);
    })();
  };

  const cycleReasoning = () => {
    const idx = REASONING_EFFORTS.indexOf(reasoningEffort);
    const next = REASONING_EFFORTS[(idx + 1) % REASONING_EFFORTS.length];
    void (async () => {
      const info = await updateSettings({ reasoningEffort: next });
      if (info) turn.setStatusHint(`reasoning:${info.reasoningEffort}`);
    })();
  };

  const selectModel = (nextModel: string) => {
    if (!nextModel || nextModel === model) return;
    void (async () => {
      const info = await updateSettings({ model: nextModel });
      if (info) turn.setStatusHint(`model: ${info.model}`);
    })();
  };

  const onSend = (message: string) => {
    void turn.send(message);
  };

  const onCancel = () => {
    void turn.cancel();
  };

  const onResolveApproval = (approved: boolean) => {
    void turn.resolveApproval(approved);
  };

  const body = useMemo(() => {
    if (configError) {
      return <NoConfigCard cwd={cwd} />;
    }
    if (!chat) {
      return (
        <div className="flex flex-1 flex-col items-center justify-center gap-4 p-6">
          <p className="font-mono text-sm text-[var(--color-fg-muted)]">
            no chat open.
          </p>
          <button
            onClick={() => void handleNew()}
            className="font-mono text-sm text-[var(--color-accent)] hover:underline"
          >
            ▸ start a new chat
          </button>
        </div>
      );
    }
    return (
      <>
        <Transcript blocks={turn.blocks} />
        {turn.pendingApproval && (
          <ApprovalDialog
            approval={turn.pendingApproval}
            onResolve={onResolveApproval}
          />
        )}
      </>
    );
  }, [configError, chat, turn.blocks, turn.pendingApproval, cwd, handleNew]);

  return (
    <div className="vignette scanlines grain relative flex h-screen flex-col bg-[var(--color-bg)]">
      <TopBar
        chat={chat}
        onNewChat={() => void handleNew()}
        onOpenChat={() => void handleOpen()}
        chats={chats}
        cwd={cwd}
      />
      {showOpen && (
        <OpenChatList
          chats={chats}
          onPick={(id) => void handlePick(id)}
          onClose={() => setShowOpen(false)}
        />
      )}
      {body}
      {chat && (
        <Composer
          onSend={onSend}
          onCancel={onCancel}
          running={turn.state === "running" || turn.state === "approval"}
          accessMode={accessMode}
          reasoningEffort={reasoningEffort}
          model={model}
          modelOptions={modelOptions}
          onCycleAccessMode={cycleAccessMode}
          onCycleReasoning={cycleReasoning}
          onSelectModel={selectModel}
        />
      )}
      {chat && (
        <StatusFooter
          chatId={chat.id}
          cwd={chat.cwd}
          model={chat.model}
          accessMode={accessMode}
          reasoningEffort={reasoningEffort}
          state={turn.state}
          status={turn.status}
        />
      )}
    </div>
  );
}
