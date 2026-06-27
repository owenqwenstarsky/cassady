import { useCallback, useEffect, useRef, useState } from "react";
import {
  type ConversationInfo,
  type ConversationRecord,
  type StreamEvent,
  type TurnHandle,
  approve,
  cancelTurn,
  deny,
  sessionRecords,
  startTurn,
} from "@/lib/tauri";

export type BlockKind =
  | "user"
  | "assistant"
  | "reasoning"
  | "tool"
  | "status";

export interface TranscriptBlock {
  id: string;
  kind: BlockKind;
  text: string;
  toolId?: string;
  toolName?: string;
  toolArguments?: unknown;
  toolOutput?: string;
  toolOk?: boolean;
  toolDone?: boolean;
}

export interface PendingApproval {
  requestId: string;
  toolCallId: string;
  name: string;
  arguments: unknown;
  reason: string;
}

export type TurnState = "idle" | "running" | "approval" | "cancelled" | "error";

let blockSeq = 0;
function nextBlockId(): string {
  blockSeq += 1;
  return `b-${blockSeq}`;
}

function blocksFromRecords(records: ConversationRecord[]): TranscriptBlock[] {
  const blocks: TranscriptBlock[] = [];

  for (const record of records) {
    switch (record.type) {
      case "user": {
        blocks.push({
          id: nextBlockId(),
          kind: "user",
          text: record.content,
        });
        break;
      }
      case "assistant": {
        if (record.reasoning?.trim()) {
          blocks.push({
            id: nextBlockId(),
            kind: "reasoning",
            text: record.reasoning,
          });
        }
        if (record.content.trim()) {
          blocks.push({
            id: nextBlockId(),
            kind: "assistant",
            text: record.content,
          });
        }
        for (const call of record.tool_calls ?? []) {
          blocks.push({
            id: nextBlockId(),
            kind: "tool",
            text: "",
            toolId: call.id,
            toolName: call.name,
            toolArguments: call.arguments,
            toolOutput: "",
            toolDone: false,
          });
        }
        break;
      }
      case "tool": {
        const idx = blocks.findIndex(
          (block) => block.kind === "tool" && block.toolId === record.tool_call_id,
        );
        const toolBlock: TranscriptBlock = {
          id: idx >= 0 ? blocks[idx].id : nextBlockId(),
          kind: "tool",
          text: record.content,
          toolId: record.tool_call_id,
          toolName: record.name,
          toolArguments: idx >= 0 ? blocks[idx].toolArguments : undefined,
          toolOutput: idx >= 0 ? blocks[idx].toolOutput : "",
          toolOk: record.ok,
          toolDone: true,
        };
        if (idx >= 0) {
          blocks[idx] = toolBlock;
        } else {
          blocks.push(toolBlock);
        }
        break;
      }
      case "meta":
      case "system":
        break;
    }
  }

  return blocks;
}

export interface UseTurn {
  blocks: TranscriptBlock[];
  state: TurnState;
  status: string;
  pendingApproval: PendingApproval | null;
  turnId: string | null;
  send: (message: string) => Promise<void>;
  cancel: () => Promise<void>;
  resolveApproval: (approved: boolean) => Promise<void>;
  reset: () => void;
  setStatusHint: (text: string) => void;
}

export function useTurn(chat: ConversationInfo | null): UseTurn {
  const [blocks, setBlocks] = useState<TranscriptBlock[]>([]);
  const [state, setState] = useState<TurnState>("idle");
  const [status, setStatus] = useState<string>("idle");
  const [pendingApproval, setPendingApproval] = useState<PendingApproval | null>(
    null,
  );
  const [turnId, setTurnId] = useState<string | null>(null);

  const turnIdRef = useRef<string | null>(null);
  const loadSeqRef = useRef(0);
  const streamSeqRef = useRef(0);
  const chatRef = useRef<ConversationInfo | null>(chat);
  chatRef.current = chat;

  const appendBlock = useCallback((block: TranscriptBlock) => {
    setBlocks((prev) => [...prev, block]);
  }, []);

  const updateBlock = useCallback(
    (id: string, updater: (b: TranscriptBlock) => TranscriptBlock) => {
      setBlocks((prev) => prev.map((b) => (b.id === id ? updater(b) : b)));
    },
    [],
  );

  const handleEvent = useCallback(
    (event: StreamEvent) => {
      switch (event.kind) {
        case "assistantChunk": {
          setBlocks((prev) => {
            const last = prev[prev.length - 1];
            if (last && last.kind === "assistant" && !last.toolDone) {
              return [
                ...prev.slice(0, -1),
                { ...last, text: last.text + event.text },
              ];
            }
            return [
              ...prev,
              {
                id: nextBlockId(),
                kind: "assistant",
                text: event.text,
              },
            ];
          });
          break;
        }
        case "reasoningChunk": {
          setBlocks((prev) => {
            const last = prev[prev.length - 1];
            if (last && last.kind === "reasoning") {
              return [
                ...prev.slice(0, -1),
                { ...last, text: last.text + event.text },
              ];
            }
            return [
              ...prev,
              {
                id: nextBlockId(),
                kind: "reasoning",
                text: event.text,
              },
            ];
          });
          break;
        }
        case "toolCallStarted": {
          appendBlock({
            id: nextBlockId(),
            kind: "tool",
            text: "",
            toolId: event.id,
            toolName: event.name,
            toolArguments: event.arguments,
            toolOutput: "",
            toolDone: false,
          });
          break;
        }
        case "toolOutputChunk": {
          setBlocks((prev) =>
            prev.map((b) =>
              b.toolId === event.id
                ? {
                    ...b,
                    toolOutput: (b.toolOutput ?? "") + event.content,
                  }
                : b,
            ),
          );
          break;
        }
        case "toolResult": {
          setBlocks((prev) =>
            prev.map((b) =>
              b.toolId === event.id
                ? {
                    ...b,
                    toolOk: event.ok,
                    toolDone: true,
                    text: event.content,
                  }
                : b,
            ),
          );
          break;
        }
        case "approvalRequested": {
          setPendingApproval({
            requestId: event.requestId,
            toolCallId: event.toolCallId,
            name: event.name,
            arguments: event.arguments,
            reason: event.reason,
          });
          setState("approval");
          break;
        }
        case "approvalResolved": {
          setPendingApproval(null);
          setState("running");
          break;
        }
        case "status": {
          setStatus(event.text);
          break;
        }
        case "finished": {
          setState("idle");
          setStatus("idle");
          turnIdRef.current = null;
          setTurnId(null);
          break;
        }
        case "error": {
          setState("error");
          setStatus(event.message);
          break;
        }
      }
    },
    [appendBlock, updateBlock],
  );

  const send = useCallback(
    async (message: string) => {
      const activeChat = chatRef.current;
      if (!activeChat || turnIdRef.current) return;
      const chatId = activeChat.id;
      const streamSeq = streamSeqRef.current;
      loadSeqRef.current += 1;
      appendBlock({
        id: nextBlockId(),
        kind: "user",
        text: message,
      });
      setState("running");
      setStatus("running");
      try {
        const handle: TurnHandle = await startTurn(chatId, message, (event) => {
          if (
            streamSeqRef.current === streamSeq &&
            chatRef.current?.id === chatId
          ) {
            handleEvent(event);
          }
        });
        if (streamSeqRef.current === streamSeq && chatRef.current?.id === chatId) {
          turnIdRef.current = handle.turnId;
          setTurnId(handle.turnId);
        }
      } catch (e) {
        if (streamSeqRef.current === streamSeq && chatRef.current?.id === chatId) {
          setState("error");
          setStatus(String(e));
        }
      }
    },
    [appendBlock, handleEvent],
  );

  const cancel = useCallback(async () => {
    const id = turnIdRef.current;
    if (!id) return;
    try {
      await cancelTurn(id);
    } catch (e) {
      setStatus(String(e));
    }
    turnIdRef.current = null;
    setTurnId(null);
    setState("cancelled");
    setStatus("cancelled");
  }, []);

  const resolveApproval = useCallback(
    async (approved: boolean) => {
      const id = turnIdRef.current;
      const req = pendingApproval;
      if (!id || !req) return;
      setPendingApproval(null);
      setState("running");
      try {
        if (approved) {
          await approve(id, req.requestId);
        } else {
          await deny(id, req.requestId);
        }
      } catch (e) {
        setState("error");
        setStatus(String(e));
      }
    },
    [pendingApproval],
  );

  const reset = useCallback(() => {
    setBlocks([]);
    setState("idle");
    setStatus("idle");
    setPendingApproval(null);
    turnIdRef.current = null;
    setTurnId(null);
  }, []);

  const setStatusHint = useCallback((text: string) => {
    setStatus(text);
  }, []);

  useEffect(() => {
    const chatId = chat?.id;
    const loadSeq = loadSeqRef.current + 1;
    loadSeqRef.current = loadSeq;
    streamSeqRef.current += 1;
    turnIdRef.current = null;
    setTurnId(null);
    setPendingApproval(null);
    setState("idle");
    setStatus("idle");
    setBlocks([]);

    if (!chatId) return;

    void (async () => {
      try {
        const records = await sessionRecords(chatId);
        if (loadSeqRef.current === loadSeq && chatRef.current?.id === chatId) {
          setBlocks(blocksFromRecords(records));
        }
      } catch (e) {
        if (loadSeqRef.current === loadSeq && chatRef.current?.id === chatId) {
          setState("error");
          setStatus(String(e));
        }
      }
    })();
  }, [chat?.id]);

  return {
    blocks,
    state,
    status,
    pendingApproval,
    turnId,
    send,
    cancel,
    resolveApproval,
    reset,
    setStatusHint,
  };
}
