import { useEffect, useMemo, useState } from "react";
import { TopBar, OpenChatList, NoConfigCard, useSessionManager } from "@/components/TopBar";
import { Transcript } from "@/components/Transcript";
import { Composer } from "@/components/Composer";
import { BranchModal } from "@/components/BranchModal";
import { LogoutModal } from "@/components/LogoutModal";
import { LoginModal } from "@/components/LoginModal";
import { ACCESS_MODES, REASONING_EFFORTS } from "@/lib/sessionSettings";
import { StatusFooter } from "@/components/StatusFooter";
import { ApprovalDialog } from "@/components/ApprovalDialog";
import { useTurn } from "@/hooks/useTurn";
import type {
  AccessMode,
  BranchFamily,
  Checkpoint,
  CommandOutcome,
  LogoutResult,
  ProviderApplyResult,
  ProviderLogoutCandidate,
  ReasoningEffort,
} from "@/lib/tauri";
import {
  createBranchFromCheckpoint,
  listModels,
  reloadSessionConfig,
  runSlashCommand,
  updateSessionSettings,
  type ModelOption,
} from "@/lib/tauri";

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
  const [branchFamily, setBranchFamily] = useState<BranchFamily | null>(null);
  const [logoutCandidates, setLogoutCandidates] = useState<
    ProviderLogoutCandidate[]
  >([]);
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [loginOpen, setLoginOpen] = useState(false);

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

  const refreshModelOptions = async () => {
    try {
      setModelOptions(await listModels());
    } catch {
      setModelOptions([]);
    }
  };

  const routeSlashOutcome = async (outcome: CommandOutcome) => {
    switch (outcome.kind) {
      case "status": {
        turn.pushNotice(outcome.content, "status");
        // The CLI sets the status line to the content for /fast and /model,
        // and to "status shown" for /status.
        turn.setStatusHint(
          outcome.title === "status" ? "status shown" : outcome.content,
        );
        break;
      }
      case "newChat": {
        turn.queueAfterLoad({ status: outcome.status });
        setChat(outcome.info);
        break;
      }
      case "resumedChat": {
        turn.queueAfterLoad({
          status: outcome.status,
          notice: outcome.warning
            ? { text: outcome.warning, kind: "error" }
            : undefined,
        });
        setChat(outcome.info);
        break;
      }
      case "openBranchPicker": {
        setBranchFamily(outcome.family);
        turn.setStatusHint("branch/restore menu");
        break;
      }
      case "openLoginWizard": {
        setLoginOpen(true);
        turn.setStatusHint("login");
        break;
      }
      case "openLogoutPicker": {
        setLogoutCandidates(outcome.candidates);
        setLogoutOpen(true);
        turn.setStatusHint("logout");
        break;
      }
      case "busy":
        turn.setStatusHint(outcome.message);
        break;
      case "parseError":
        turn.setStatusHint(outcome.message);
        break;
      case "error":
        turn.pushNotice(outcome.message, "error");
        turn.setStatusHint(outcome.message);
        break;
    }
  };

  const onSlashCommand = (input: string) => {
    if (!chat) return;
    void (async () => {
      try {
        const outcome = await runSlashCommand(chat.id, input);
        await routeSlashOutcome(outcome);
      } catch (e) {
        turn.setStatusHint(String(e));
      }
    })();
  };

  const onBranchSwitch = (branchId: string) => {
    setBranchFamily(null);
    onSlashCommand(`/resume ${branchId}`);
  };

  const onBranchFromCheckpoint = async (
    checkpoint: Checkpoint,
    restoreFiles: boolean,
  ) => {
    if (!chat) return;
    try {
      const result = await createBranchFromCheckpoint(
        chat.id,
        checkpoint,
        restoreFiles,
      );
      const notice = result.restore
        ? {
            text: `${result.restore.summary}\n\nApplied: ${result.restore.applied}\nSkipped: ${result.restore.skipped}\nConflicts: ${result.restore.conflicts}`,
            kind: (result.restore.conflicts > 0 ? "error" : "status") as
              | "status"
              | "error",
          }
        : undefined;
      turn.queueAfterLoad({ status: result.status, notice });
      setChat(result.info);
      setBranchFamily(null);
    } catch (e) {
      turn.setStatusHint(String(e));
    }
  };

  const onLogoutResolved = async (result: LogoutResult) => {
    setLogoutOpen(false);
    setLogoutCandidates([]);
    await refreshModelOptions();
    if (result.removedProviderIds.length === 0) {
      turn.pushNotice("logout cancelled", "status");
      turn.setStatusHint("logout cancelled");
    } else if (result.remainingProviderCount === 0) {
      turn.pushNotice(
        `removed providers: ${result.removedProviderIds.join(", ")}\nremoved model entries: ${result.removedModelCount}\nno providers remain; run /login before sending another message`,
        "status",
      );
      turn.setStatusHint("no provider configured");
    } else {
      // Reload the open session's config so the active provider/model reflect
      // the post-logout defaults (repair_active_defaults updated config.json).
      if (chat) {
        try {
          const info = await reloadSessionConfig(chat.id);
          setChat(info);
        } catch (e) {
          turn.setStatusHint(String(e));
        }
      }
      turn.pushNotice(
        `removed providers: ${result.removedProviderIds.join(", ")}\nremoved model entries: ${result.removedModelCount}\nactive provider: ${result.activeProvider ?? "?"}\nactive model: ${result.activeModel ?? "?"}`,
        "status",
      );
      turn.setStatusHint("logout updated");
    }
  };

  const onLoginResolved = async (result: ProviderApplyResult) => {
    setLoginOpen(false);
    await refreshModelOptions();
    // Reload the open session's config so it picks up the new active
    // provider/model written by apply_provider_login.
    if (chat) {
      try {
        const info = await reloadSessionConfig(chat.id);
        setChat(info);
      } catch (e) {
        turn.setStatusHint(String(e));
      }
    }
    turn.pushNotice(
      `active provider: ${result.activeProvider}\nactive model: ${result.activeModel}`,
      "status",
    );
    turn.setStatusHint("login updated");
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
    <div className="vignette scanlines grain relative flex h-screen w-screen min-w-0 flex-col overflow-hidden bg-[var(--color-bg)]">
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
          onSlashCommand={onSlashCommand}
          cwd={chat.cwd}
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
      {chat && branchFamily && (
        <BranchModal
          family={branchFamily}
          onSwitch={onBranchSwitch}
          onBranch={onBranchFromCheckpoint}
          onClose={() => {
            setBranchFamily(null);
            turn.setStatusHint("branch menu cancelled");
          }}
        />
      )}
      {chat && logoutOpen && (
        <LogoutModal
          candidates={logoutCandidates}
          onResolved={onLogoutResolved}
          onClose={() => {
            setLogoutOpen(false);
            turn.setStatusHint("logout cancelled");
          }}
        />
      )}
      {chat && loginOpen && (
        <LoginModal
          onResolved={onLoginResolved}
          onClose={() => {
            setLoginOpen(false);
            turn.setStatusHint("login cancelled");
          }}
        />
      )}
    </div>
  );
}
