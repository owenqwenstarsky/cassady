# v0.2.1 Collapsed Tool Density Follow-up Plan

## Context

This is a follow-up to `plans/V0_2_1_MESSAGE_RENDERING_POLISH_PLAN.md`.

The first pass improved tool rendering by removing completed processing/invocation blocks and replacing hidden tool output with compact summaries. In practice, repeated successful low-signal tools such as `ls` still clog the transcript because each collapsed result renders as a heading plus a summary body line.

## Goal

Make collapsed tool display much denser while preserving useful auditability.

## Scope

- Collapse non-live successful tool result summaries into the heading line.
- Omit successful `ls` result blocks entirely while tool output is hidden.
- Keep failed tool results visible.
- Keep live/running tool output visible.
- Keep full output behavior unchanged when full tools are enabled.

## Design

When `show_full_tools == false`:

- `ls ✓ (...)` blocks are skipped entirely.
- Other completed successful tool blocks render as one line:

```text
· read ✓ (abc123) · 202 lines · 10.3 KB
```

- Tool body content is omitted.
- Failed tool results continue to show content so errors are visible.
- Live tool blocks with streamed output continue to show body content.

When `show_full_tools == true`:

- Render all tool result blocks normally with full content.

## Implementation Steps

1. Update `src/ui/render.rs` transcript rendering.
2. Add helpers:
   - `is_collapsed_successful_ls_result()`
   - `collapsed_tool_heading_summary()`
3. Change collapsed successful tool behavior:
   - Skip successful `ls` blocks entirely.
   - Add output summary to the heading for other tools.
   - Return empty body content for collapsed successful tools.
4. Leave errors and live output unchanged.
5. Add tests for:
   - successful `ls` hidden while collapsed
   - successful `ls` visible with full tools
   - successful `read` collapsed to one line
   - failed tool output still visible while collapsed

## Acceptance Criteria

- Repeated `ls` calls no longer clutter the default transcript view.
- Other successful tool calls occupy one line when collapsed.
- Expanding tools still shows complete tool outputs.
- Tool failures remain inspectable without expanding tools.
- `cargo fmt` and `cargo test` pass.
