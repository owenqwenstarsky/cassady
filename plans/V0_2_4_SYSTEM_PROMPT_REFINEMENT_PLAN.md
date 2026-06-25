# v0.2.4 System Prompt Refinement Implementation Plan

## Goal

v0.2.4 refines Cassady's generated system prompt so the model receives clearer, more intuitive operating instructions without turning the prompt into a long manual. The effective prompt should explain Cassady's role, terminal transcript behavior, tool use, editing expectations, runtime safety constraints, and response style in a structured way that models can follow reliably.

Success statement:

> A normal effective system prompt, excluding user-provided global instructions, is roughly 900-1,100 tokens and consistently guides the model to inspect before editing, use tools directly when useful, respect access modes, make targeted file changes, and finish each turn with an honest concise response.

## Scope

### In scope

- Rewrite `src/prompt.rs` prompt text into a clearer sectioned structure.
- Preserve the existing prompt-generation model:
  - `build_base_system_prompt(global)` creates reusable conversation-level instructions.
  - `build_effective_system_prompt(...)` appends model/workspace/docs/access/tool runtime constraints.
  - `~/.cass/global.md` text remains embedded as user global instructions when present.
- Add product context that helps the model understand Cassady's terminal chat UX.
- Improve guidance for tool selection, exact-text edits, use of shell, and test/summarization behavior.
- Make access-mode guidance concise and easy to map to the currently allowed tools.
- Add focused tests for prompt sections, ordering, global instructions, access modes, and approximate size.
- Update docs only where they mention global instructions or prompt behavior.
- Keep the prompt provider-agnostic and compatible with all OpenAI-compatible models Cassady supports.

### Out of scope

- Adding prompt templates, profile selection, or user-selectable prompt modes.
- Exposing a CLI command to print or edit the full generated system prompt.
- Changing the `~/.cass/global.md` file format or adding layered project instructions.
- Changing access-policy enforcement, tool schemas, approval UI, or security decisions.
- Adding new tools or changing tool argument formats.
- Implementing automatic prompt compression or conversation summarization.
- Maintaining separate prompts per provider/model family.
- Stuffing large reference documentation, provider catalogs, or CLI help into the system prompt.

## Context and Current State

Relevant files:

- `src/prompt.rs`: builds both the base and effective system prompts. The current prompt is short and functional but sparse.
- `src/app.rs`: reads global instructions and stores the base prompt in new conversations.
- `src/agent.rs`: calls `build_effective_system_prompt(...)` before provider requests.
- `src/conversation.rs`: persists the base system prompt in the conversation record and reuses it when a chat is resumed.
- `src/access.rs`: defines `read-only`, `workspace-edit`, and `full-access` modes.
- `src/security.rs`: central policy for tool availability, read/write boundaries, shell approval, and denials.
- `src/tools/*`: tool implementations and schemas for `ls`, `read`, `grep`, `write`, `edit`, and `shell`.
- `docs/glossary.md`: defines global instructions as optional text in `~/.cass/global.md` included in new chat system prompts.
- `tests/*`: no dedicated prompt tests exist yet; prompt behavior is only indirectly covered through agent/conversation tests.

Current prompt behavior to preserve:

- Cassady identifies itself as Cassady/Cass, a coding agent running in a terminal chat interface.
- User global instructions are included only when non-empty after trimming.
- Global instructions are subordinate to runtime safety constraints.
- The effective prompt includes:
  - model id,
  - active access mode,
  - launch working directory,
  - bundled docs directory,
  - allowed tools,
  - access-mode-specific instructions,
  - final response behavior.
- Tool access is ultimately enforced by runtime policy, not by prompt wording alone.

Current gaps:

- The prompt is organized as numbered sections but does not fully explain Cassady's user-visible transcript model.
- Tool and editing guidance is too compact for models that need stronger direction on when to inspect, search, edit, write, or run shell.
- Access-mode text is accurate but can be made more direct and less repetitive.
- There is no automated check that future prompt edits preserve required sections or stay near the intended size.
- Documentation mentions global instructions, but not how they relate to runtime safety constraints in the refined prompt.

## Design Principles

1. **High signal, low bulk.** The prompt should contain the instructions most likely to improve model behavior, not a copy of the README.
2. **Runtime policy remains authoritative.** Prompt text should guide the model, while `src/security.rs` and tool availability continue to enforce real permissions.
3. **Structure beats length.** Use clear headings and dense paragraphs/bullets so models can find instructions during long sessions.
4. **Tell the model what the user can see.** Explain streamed output, visible tool calls/results, approvals, and edit diffs so the assistant does not narrate inaccurately.
5. **Prefer action over ceremony.** Encourage the model to use tools directly, inspect before changing files, and ask questions only when missing information materially blocks progress.
6. **Make editing rules concrete.** Exact-text edits are a core reliability constraint and should be stated plainly.
7. **Avoid provider-specific assumptions.** The prompt should work for small and large OpenAI-compatible models without relying on special model behavior.
8. **Keep user instructions safe.** Global instructions are important, but they must never override runtime access modes, tool denials, or user requests in the active chat.

## Prompt Architecture

Keep the two-stage prompt generation, but make the internal structure more intentional.

### Base prompt

`build_base_system_prompt(global)` should contain stable instructions that are true for every session:

1. Identity and role.
2. Operating principles.
3. User global instructions, when present.
4. Tool-use behavior.
5. Editing behavior.
6. Response behavior.

The base prompt is stored in the conversation when a chat is created. Because resumed chats reuse the stored base prompt, changing the base prompt affects new chats but not necessarily existing conversations. That behavior is acceptable and should be documented only if user-facing docs mention prompt changes.

### Effective prompt

`build_effective_system_prompt(...)` should append runtime-specific information near the end:

1. Current runtime context:
   - model,
   - access mode,
   - launch working directory,
   - bundled docs directory,
   - allowed tools.
2. Access-mode rules for the active mode.
3. Final reminder that runtime policy and tool results are authoritative.

Runtime constraints should remain near the end so they are fresh in the model's context and can override earlier general instructions.

### Numbering and headings

Use stable Markdown-like headings rather than fragile sentence-only text. For example:

```text
# Cassady operating instructions

## Role
...

## Working style
...
```

Numbered headings are acceptable if tests are written against section names rather than exact numbers. Avoid deeply nested outlines.

## Target Prompt Content

The final wording can change during implementation, but it should cover the following content.

### 1. Role

Required ideas:

- You are Cassady, also called Cass.
- You are a coding assistant inside an interactive terminal chat.
- You help with real project work: reading code, explaining behavior, editing files, and running relevant commands when allowed.
- Work carefully and honestly; do not pretend to have inspected or changed files unless tool results confirm it.

Avoid:

- Overly broad claims such as being a general-purpose autonomous system.
- Long branding language.
- Any implication that prompt instructions can bypass runtime access policy.

### 2. Working style

Required ideas:

- Prefer concrete progress over long speculative plans.
- Inspect relevant files before making claims or edits.
- Ask a focused follow-up question only when the task is ambiguous or blocked.
- Keep explanations concise but include enough context for the user to review the work.
- If the user's request is impossible in the current mode, explain the limitation and the next viable step.

Suggested wording style:

```text
Make the smallest useful plan, then act. Do not over-plan routine code tasks. When information is missing, gather it with tools if possible; ask the user only when a choice or secret is genuinely required.
```

### 3. Transcript and UI awareness

Required ideas:

- Assistant text is streamed to the user.
- Tool calls and tool results are visible in the transcript.
- Edit diffs and approval prompts may be shown by Cassady's UI.
- The model should request tools directly rather than asking for chat permission before every tool call.
- Cassady handles access denials and approval UI separately.

This section should reduce behaviors such as:

- Saying "I will run X" and then not calling the tool.
- Asking "May I read the file?" when the tool is available.
- Claiming a shell command ran before its result arrives.
- Repeating huge summaries of tool output that the user can already see.

### 4. Tool use

Required guidance by tool area:

- `ls`: use for directory orientation.
- `grep`: use before reading large or unknown files, or to locate definitions/usages.
- `read`: use targeted reads for files or ranges that matter.
- `edit`: use for focused changes to existing files.
- `write`: use for new files or intentional full rewrites.
- `shell`: use for tests, builds, formatting, diagnostics, or project commands when allowed and useful.

General instructions:

- Use tools when current filesystem state matters.
- Prefer targeted inspection over guessing.
- Batch related reads when possible, but avoid reading unrelated files.
- Do not use shell for file inspection when `ls`/`grep`/`read` is safer and sufficient.
- If a tool is denied, adapt to the denial instead of repeating the same call.

### 5. Editing

Required ideas:

- Prefer `edit` for small and medium modifications to existing files.
- `edit` replacements must use exact old text that appears uniquely in the original file.
- Keep edits minimal, unique, and non-overlapping.
- Combine related replacements for the same file in one `edit` call when practical.
- Use `write` only for new files or full rewrites where that is safer and intentional.
- After meaningful code changes, run relevant tests/formatters when allowed, or tell the user what should be run.
- Mention changed files and verification in the final response.

### 6. Safety and access modes

The base prompt should state the general principle:

- Follow runtime constraints and active access mode.
- Do not try to bypass workspace boundaries, docs read-only rules, approvals, or tool denials.

The effective prompt should include active-mode-specific guidance:

#### read-only

- Allowed tools should normally be `ls`, `read`, and `grep`.
- Inspect only inside the launch workspace and bundled docs directory.
- Do not request `write`, `edit`, or `shell`.
- If changes or commands are needed, explain that a more permissive access mode is required.

#### workspace-edit

- Read/list/search inside the launch workspace and bundled docs directory.
- Write/edit only inside the launch workspace.
- Bundled docs are read-only.
- Shell may be requested when useful, but Cassady will show the approval UI; do not ask for shell permission in chat first.
- If a path escapes the workspace, choose an in-workspace alternative or explain the limitation.

#### full-access

- `ls`, `read`, `grep`, `write`, `edit`, and `shell` may be requested when needed.
- Shell runs from the launch working directory.
- Normal OS permissions still apply.
- Bundled docs remain read-only for write/edit.
- Even in full-access, keep changes targeted and avoid destructive commands unless the user explicitly requested them and the action is necessary.

### 7. Final response behavior

Required ideas:

- Always end the turn with a concise user-facing response after tool work.
- Do not finish with only tool calls.
- Summarize what changed, where, and how it was verified.
- If no changes were made, summarize findings or blockers.
- Be honest about failures, denials, skipped tests, or assumptions.

## Approximate Prompt Budget

Target size: roughly 900-1,100 tokens for the normal effective system prompt, excluding user global instructions.

Because Cassady does not currently include a tokenizer, implement a simple approximate check rather than adding a heavy tokenizer dependency unless the implementer strongly prefers otherwise.

Recommended helper for tests:

```rust
fn approximate_token_count(s: &str) -> usize {
    s.split_whitespace().count() * 4 / 3
}
```

This heuristic is intentionally rough. The test should prevent accidental prompt bloat, not enforce an exact model-token count. Suggested limits:

- Base prompt without global instructions: approximately 650-850 heuristic tokens.
- Effective prompt in each access mode without global instructions: approximately 900-1,150 heuristic tokens.

If the final prompt is slightly outside the target but demonstrably better, prefer readability over gaming the heuristic. The acceptance target should remain "around 1,000 tokens," not an exact failure-prone threshold.

## Proposed Prompt Skeleton

This skeleton is illustrative, not a required exact implementation.

```text
# Cassady operating instructions

## Role
You are Cassady, also called Cass, a coding assistant running in an interactive terminal chat. Help with real project work: inspect files, explain code, make targeted edits, and run useful commands when allowed. Work carefully and do not claim that files were read, commands ran, or edits succeeded until tool results confirm it.

## Working style
Make the smallest useful plan, then act. Prefer current project evidence over guesses. Use tools to gather missing filesystem context. Ask a focused follow-up question only when a user choice, secret, or missing requirement blocks progress. Keep user-facing explanations concise and practical.

## Transcript and tools
Assistant text is streamed. Tool calls, tool results, approvals, and edit diffs are visible in the transcript. Request tools directly when they are the right next step; Cassady enforces access policy and shows approval prompts separately. If a tool is denied or fails, adapt and explain the limitation.

## Tool use
Use ls for directory orientation, grep to locate text or inspect large/unknown areas, read for relevant files or ranges, edit for targeted changes, write for new files or intentional full rewrites, and shell for tests/builds/diagnostics when allowed. Prefer targeted reads and related batched reads over broad exploration.

## Editing
Inspect before editing. For edit, each old text must match exactly and uniquely in the original file; keep replacements minimal and non-overlapping. Do not use write for small changes to existing files. After meaningful code changes, run relevant verification when allowed or state what should be run.

## User global instructions
...

## Runtime context
Model: ...
Access mode: ...
Launch working directory: ...
Bundled Cass docs directory: ...
Allowed tools this turn: ...

## Access rules for this session
...

## Final response
End every turn with a concise response. Summarize changed files and verification, or summarize findings/blockers if no change was made. Do not end with only tool calls.
```

## Implementation Steps

### 1. Inventory exact current behavior

- Review `src/prompt.rs`, `src/agent.rs`, `src/app.rs`, `src/conversation.rs`, `src/access.rs`, `src/security.rs`, and `src/tools/schema.rs`.
- Confirm current tool names and per-mode availability from `SecurityPolicy::tool_availability`.
- Confirm docs directory behavior and blocked write roots from app/tool context construction.
- Confirm how global instructions are loaded and trimmed.
- Confirm how resumed conversations reuse the stored base prompt.

### 2. Rewrite `build_base_system_prompt`

- Replace the current compact numbered prompt with structured, high-signal sections.
- Include identity, working style, transcript/tool visibility, general tool guidance, editing guidance, global instructions, and response behavior.
- Preserve trimming behavior for `global`.
- Keep global instructions clearly labelled and explicitly subordinate to runtime safety constraints.
- Avoid including runtime-only values in the base prompt.

### 3. Rewrite `build_effective_system_prompt`

- Keep appending to `base.trim_end()`.
- Add a clear `Runtime context` section with model, access mode, cwd, docs dir, and allowed tools.
- Add one active-mode-specific `Access rules for this session` paragraph/bullet set.
- Keep runtime constraints after global/base text.
- Keep the final response reminder either at the end of the base prompt or at the end of the effective prompt. If it remains in the base prompt, add a short final runtime-policy reminder after access rules.

### 4. Add prompt tests

Create `tests/prompt_tests.rs` or add focused unit tests in `src/prompt.rs`. Prefer integration tests in `tests/prompt_tests.rs` so prompt behavior is covered through the public crate API if exports allow it.

Recommended tests:

1. **Base prompt has required sections.**
   - Build with `None`.
   - Assert it contains headings/phrases for role, working style, tools, editing, and final response behavior.
   - Assert it does not contain runtime-only paths or model labels.

2. **Global instructions are included and trimmed.**
   - Build with whitespace-wrapped global text.
   - Assert the exact trimmed content appears.
   - Assert the prompt says global instructions cannot conflict with runtime safety constraints.

3. **Empty global instructions are omitted.**
   - Build with `Some("   \n")`.
   - Assert the global-instructions heading is absent.

4. **Effective prompt includes runtime context.**
   - Use temporary or fixed paths for cwd/docs.
   - Assert model, mode, cwd, docs dir, and allowed tools are present.

5. **Each access mode gets correct instructions.**
   - `read-only`: contains no write/edit/shell request guidance and says more permissive mode is needed for modifications.
   - `workspace-edit`: says write/edit only inside workspace and shell approval is handled by Cassady UI.
   - `full-access`: says all tools may be requested, shell runs from cwd, docs remain read-only.

6. **Runtime constraints stay after global instructions.**
   - Build base with global text, then effective prompt.
   - Assert global text index is before runtime context index.
   - Assert access rules appear after runtime context.

7. **Prompt size remains intentional.**
   - Build effective prompts for all modes without global instructions.
   - Use the approximate token helper.
   - Assert each is within the chosen guardrail, for example `800..=1250` approximate tokens.

8. **Allowed tools list reflects caller input.**
   - Pass a small custom tool list.
   - Assert the rendered list matches it.

Test guidance:

- Avoid asserting the entire prompt as one giant snapshot unless the project already uses snapshot testing.
- Prefer stable phrases and section headings so minor copy edits do not make tests brittle.
- If a snapshot is added, keep it intentionally small or use one golden prompt plus semantic tests.

### 5. Update docs references

Update only docs that need to mention global instructions or prompt behavior.

Likely files:

- `docs/glossary.md`: expand `Global instructions` to say they are included in new chat system prompts and followed unless they conflict with runtime safety constraints.
- `docs/configuration.md` or `README.md` only if they already mention `~/.cass/global.md` and need clarification.

Do not publish the full internal system prompt in docs. It is implementation detail and will evolve.

### 6. Run verification

Required commands:

```sh
cargo fmt --check
cargo test --locked --all-targets
```

If prompt tests use temp paths or platform-dependent path display, run on the current platform and avoid hardcoding separators where possible.

## Tests

Automated tests to add:

- New prompt tests covering base prompt structure, global instruction behavior, effective runtime context, access-mode-specific wording, ordering, and approximate size.
- Existing agent/conversation/tool tests should continue to pass unchanged.

Manual checks:

- Read one generated prompt for each access mode and verify it is understandable as prose.
- Check that the prompt does not repeat the same instruction in several sections.
- Check that docs/global-instruction wording matches the generated prompt.
- Confirm a normal prompt is around 1,000 tokens by the chosen heuristic or an external tokenizer if one is available.

## Documentation

Required documentation updates are intentionally small:

- `docs/glossary.md`: update the `Global instructions` definition.
- Any existing README/configuration references to `~/.cass/global.md`: clarify that these instructions are included in new chat system prompts and cannot override safety constraints.

No new user guide is required for this release unless implementation adds user-visible commands or configuration, which is out of scope.

## Compatibility and Migration Notes

- Existing conversations keep the base system prompt stored when they were created. The refined base prompt will apply to new conversations.
- Runtime constraints are still generated at request time, so active access mode, cwd, docs dir, and allowed tools remain current for resumed chats.
- `~/.cass/global.md` remains plain text and does not require migration.
- No config schema changes are expected.
- No provider/model configuration changes are expected.

## Risks and Mitigations

### Risk: prompt grows too large

Mitigation:

- Add a size guardrail test.
- Keep docs/provider details out of the prompt.
- Prefer compact instructions over long examples.

### Risk: tests become brittle

Mitigation:

- Test for section presence and key behavior, not every exact sentence.
- Keep exact string assertions limited to stable safety-critical phrases.

### Risk: prompt implies permissions the runtime denies

Mitigation:

- Derive access-mode wording from `SecurityPolicy::tool_availability` and current policy behavior.
- Include allowed tools in the runtime context.
- Phrase guidance as "may request when allowed" rather than unconditional permission, except in mode-specific sections verified against code.

### Risk: global instructions appear stronger than safety rules

Mitigation:

- Place global instructions in a clearly labelled section.
- State that they are followed only when consistent with user requests and runtime safety constraints.
- Add a test for this wording.

### Risk: models ignore concise instructions

Mitigation:

- Use direct imperative wording.
- Put runtime constraints near the end.
- Avoid burying editing and safety rules in long paragraphs.

## Acceptance Criteria

- `src/prompt.rs` produces a structured, readable prompt with clear sections for role, working style, transcript/tool behavior, tool use, editing, runtime context, access rules, and final responses.
- The effective prompt for each access mode is roughly 900-1,100 tokens excluding user global instructions, with an automated guardrail preventing major accidental bloat.
- User global instructions are included only when non-empty, trimmed, clearly labelled, and subordinate to runtime safety constraints.
- Runtime context includes model, access mode, launch cwd, bundled docs directory, and allowed tools.
- Access-mode guidance matches current policy for `read-only`, `workspace-edit`, and `full-access`.
- Editing instructions explicitly cover exact unique old text, minimal non-overlapping replacements, and using `write` only for new files or intentional rewrites.
- Tool-use instructions explain when to use `ls`, `grep`, `read`, `edit`, `write`, and `shell` without over-constraining the model.
- Final response guidance requires a concise user-facing response after tool work and honest reporting of verification or blockers.
- Documentation references to global instructions are accurate and do not expose the full internal prompt.
- `cargo fmt --check` and `cargo test --locked --all-targets` pass.
