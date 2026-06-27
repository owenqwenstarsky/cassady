import type { AccessMode, ReasoningEffort } from "@/lib/tauri";

export const ACCESS_MODES: AccessMode[] = [
  "read-only",
  "workspace-edit",
  "full-access",
];

export const REASONING_EFFORTS: ReasoningEffort[] = [
  "off",
  "low",
  "medium",
  "high",
];
