# v0.3.3 Tool Output Context Reliability Implementation Plan

## Goal

v0.3.3 makes Cassady more reliable after broad tool output has been truncated, compacted, or superseded in the model context. The assistant should be able to tell when details are missing, understand which file range or command produced them, and quickly recover by using narrower reads or searches instead of stalling or making unsafe edits from incomplete context.

Success statement:

> After a large read or command output is compacted out of the request context, the assistant receives concise recovery guidance with enough provenance to inspect the exact missing area again before editing.

## Scope

### In scope

- Improve model-facing compaction notes for large tool outputs.
- Preserve useful provenance for compacted `read`, `grep`, and `shell` outputs.
- Add focused guidance that nudges the assistant toward smaller line ranges and search-first workflows.
- Keep existing provider message structure valid when tool outputs are compacted or earlier records are omitted.
- Align UI summaries, stored records, and model-facing transformed output so truncation/compaction is understandable without changing the full conversation history.
- Add regression tests for broad-output recovery and context-budget trimming.
- Update README and bundled docs where they describe context management, tool output limits, and recommended inspection workflows.

### Out of scope

- Implementing semantic summarization with an additional model call.
- Replacing Cassady's approximate token estimator with provider-specific tokenizers.
- Adding a full retrieval index over prior tool outputs or repository contents.
- Changing the JSONL conversation storage format in a way that makes existing chats unreadable.
- Changing UI collapsed-tool behavior except where labels or summaries need to expose truncation/compaction status.
- Automatically editing files based on compacted output without reinspection.

## Context or Current State

Relevant current behavior:

- `src/agent.rs` converts conversation records into provider messages, supersedes older repeated read outputs, compacts older tool outputs with `compact_tool_outputs`, and trims records with `trim_to_context_budget` and `trim_to_message_limit`.
- `compact_text` currently emits a generic head/tail note: `Cass compacted this tool output from ... chars to fit the model context`.
- `superseded_read_note` already preserves file path and line range when a later read covers an earlier read range.
- `src/tools/read.rs` returns headers like `--- path lines start-end ---`, followed by numbered lines. This is good provenance, but compaction can obscure the most useful middle section.
- `src/tools/grep.rs` already recommends narrowing when `max_matches` is reached.
- `src/tools/shell.rs` returns complete stdout/stderr/exit-code text to the agent loop; if the output is large, current compaction does not know the original command or suggest a narrower command.
- `src/ui/render.rs` has separate collapsed/full tool-output presentation. That display choice must remain UI-only and must not affect the stored record or model-facing context.

The main reliability gap is that once a broad output has been compacted, the assistant may see only a generic excerpt and lose the clue needed to make the next targeted tool call.

## Design Principles

1. **Never hide incompleteness.** If Cassady compacts or truncates output before sending it to the model, the transformed content must say so plainly.
2. **Recovery beats summarization.** Prefer actionable provenance and follow-up instructions over trying to summarize omitted content heuristically.
3. **Keep guidance compact.** The fix must not consume enough context to make context pressure worse.
4. **Preserve valid provider conversations.** Tool result messages must still match their assistant tool calls after compaction and trimming.
5. **Do not mutate history for UI convenience.** JSONL records should retain original tool outputs unless a future storage migration explicitly changes that contract.

## Design

### Model-facing compaction notes

Replace the generic `compact_text(content, target_chars)` path with a metadata-aware formatter, for example:

```rust
struct ToolOutputCompactionHint {
    tool_name: Option<String>,
    original_chars: usize,
    retained_head_chars: usize,
    retained_tail_chars: usize,
    provenance: ToolOutputProvenance,
}

enum ToolOutputProvenance {
    Read { sections: Vec<ReadOutputSection> },
    Grep { stopped_after: Option<usize> },
    Shell { command: Option<String> },
    Unknown,
}
```

The first implementation can infer provenance from the tool result text and nearby conversation/tool-call data rather than changing stored record schemas.

Example compacted read result:

```text
[Cass compacted this read output from 48,212 chars to fit the model context. The omitted content came from src/app.rs lines 1-1820. Use read with a narrower line range, or grep for a symbol before reading, before relying on omitted details.]
--- retained head excerpt ---
...
--- omitted middle ---
--- retained tail excerpt ---
...
```

Example compacted shell result:

```text
[Cass compacted this shell output from 81,004 chars to fit the model context. Rerun a narrower command, pipe through grep/head/tail, or inspect the specific files named in the excerpt before making edits based on omitted lines.]
```

Keep notes deterministic and short; avoid per-line summaries of omitted content.

### Read-output provenance

Reuse and extend the existing `ReadOutputSection` parsing in `src/agent.rs`:

- Detect every `--- path lines start-end ---` section before compaction.
- Preserve observed line coverage from numbered lines when available.
- Include one compact range summary in compaction notes:
  - Single section: `path lines 35-220`.
  - Multiple sections: `3 read sections including path_a lines 1-120 and path_b lines 40-90`.
- When a later read supersedes an earlier range, continue using the current superseded-read note and ensure tests cover interaction with compaction.

### Grep and shell guidance

For `grep` output:

- Preserve existing `… stopped after N matches` text.
- If compacted, add a note suggesting a narrower query, smaller path scope, lower `max_matches`, or a focused `read` around matching lines.

For `shell` output:

- If the tool-call arguments are available in the message conversion path, include a sanitized command preview in the note when reasonably short.
- Suggest command narrowing patterns without prescribing platform-specific syntax unless the command itself is already shell-specific, e.g. `grep`, `head`, `tail`, or a more targeted subcommand.
- Do not rerun shell commands automatically.

### Prompt and tool descriptions

Update the base prompt and tool descriptions only enough to reinforce reliable behavior:

- Prefer `grep` before broad `read` when the target location is unknown.
- Read smaller line ranges when files are large or when previous output says it was compacted.
- Treat compacted/truncated output as incomplete evidence; reinspect before editing.

Avoid bloating `src/prompt.rs`; keep additions short and test expected key phrases rather than full prompt text.

### UI and storage alignment

- Stored JSONL should keep the original tool result content.
- Model-facing transformed messages may contain compacted/superseded notes.
- UI collapsed mode should keep using summaries, but summaries should not imply the model saw the full output when it did not.
- If practical, make collapsed tool summaries include a compact `compacted` or `truncated` marker only when the stored/result text itself says that Cassady truncated or stopped output.

## Implementation Steps

1. Inspect provider-message conversion in `src/agent.rs` and identify where tool-call names/arguments are still available when compacting tool results.
2. Refactor `compact_text` into metadata-aware helpers that can produce deterministic compaction notes for read, grep, shell, and unknown outputs.
3. Reuse existing read-section parsing to build concise read range summaries for compacted read outputs.
4. Add shell and grep-specific recovery guidance based on tool name and output markers.
5. Preserve the newest tool result behavior unless tests show that the newest result can still exceed practical context limits; if changed, document the tradeoff explicitly.
6. Update `src/tools/read.rs`, `src/tools/grep.rs`, and `src/prompt.rs` descriptions with concise search-first and narrow-range guidance.
7. Add or update UI summary helpers in `src/ui/render.rs` only if needed to expose stored truncation/compaction markers consistently.
8. Update README and bundled docs for context reliability, broad-output recovery, and recommended inspection workflow.
9. Run `cargo fmt` and `cargo test --locked --all-targets`.

## Tests

- Large read output compacts to a note that includes original size, path, line range, and a narrower-read/search suggestion.
- Multi-file read output compacts to a concise multi-section provenance summary.
- Superseded read output still produces the superseded note and does not lose provider-message validity after context trimming.
- Large grep output compaction preserves or adds narrowing guidance.
- Large shell output compaction suggests rerunning a narrower command and does not include unsafe automatic actions.
- Context-budget trimming does not leave orphaned tool results or assistant tool calls.
- Stored conversation records retain original tool output while model-facing messages can be compacted.
- Prompt/tool spec tests verify the presence of concise search-first and reinspection guidance.

## Documentation

- Update `README.md` where tool output/context behavior is described.
- Update `docs/workflows.md` with recommended search-first and narrow-read workflows.
- Update `docs/troubleshooting.md` with recovery steps for compacted or truncated output.
- Update `docs/glossary.md` if terms such as compacted output, superseded read, or model-facing context need clarification.

## Acceptance Criteria

- Compacted tool outputs include actionable provenance and recovery guidance.
- Broad read and command-output workflows have regression coverage demonstrating safe reinspection before edits.
- Existing conversations remain loadable and resumable.
- Provider message conversion remains valid for tool-call/tool-result pairs after compaction and trimming.
- `cargo fmt` and `cargo test --locked --all-targets` pass.
