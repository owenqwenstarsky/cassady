# v0.2.1 Message Rendering Polish Implementation Plan

## Goal

Improve Cass transcript readability by rendering user/assistant messages as Markdown and making tool calls/results easier to scan. Keep the conversation record and provider payloads unchanged; this release is primarily a presentation-layer improvement.

## Scope

### In scope

- Render `User` and `Assistant` transcript blocks as Markdown in the TUI.
- Preserve existing transcript wrapping, scrolling, and sanitization behavior.
- Improve tool invocation summaries with familiar developer-facing language.
- Improve collapsed tool result display so hidden output still gives useful context.
- Normalize tool titles/statuses for successful, failed, pending, cancelled, and approval blocks.
- Add focused rendering tests.

### Out of scope

- Changing stored conversation format.
- Changing provider message format.
- Rich Markdown support for tables, images, raw HTML, or nested block-level edge cases.
- Interactive expand/collapse per individual tool call.
- Syntax highlighting for code fences.

## Current State

Rendering is centralized in `src/ui/render.rs`:

- `transcript_lines_from()` converts `TranscriptBlock`s into ratatui `Line`s.
- User and assistant content is displayed as sanitized plain text.
- Tool output is hidden when `show_full_tools == false`, except for live streamed tool output.
- `max_transcript_scroll()` depends on `transcript_lines_from()` output and wrapped row counting.

Conversation-to-transcript conversion happens in `src/app.rs`:

- `blocks_from_conversation()` turns records into `TranscriptBlock`s.
- Assistant tool calls currently show pretty-printed JSON arguments.
- Tool result blocks show full raw content when expanded.

## Design Principles

1. **Presentation-only where possible**
   - Keep `TranscriptBlock` and conversation records stable unless a small field addition is clearly worth it.
   - Prefer helper functions in rendering/conversion code over protocol/model changes.

2. **Developer-native wording**
   - Use common dev terms: `file`, `lines`, `command`, `query`, `matches`, `edits`, `diff`, `exit`, `duration`.
   - Avoid implementation-specific or awkward terms like `replacements`, `operations`, `mutations`, or `modifications`.

3. **Readable collapsed state**
   - Collapsed tools should not vanish entirely.
   - Show compact metadata such as line count and byte size.

4. **Markdown subset first**
   - Support common chat Markdown well.
   - Do not attempt complete Markdown terminal fidelity in v0.2.1.

## Implementation Steps

## 1. Add Markdown parser dependency

Update `Cargo.toml`:

```toml
pulldown-cmark = "0.12"
```

Rationale: `pulldown-cmark` is mature, lightweight, and suitable for converting Markdown events into ratatui lines.

## 2. Add Markdown rendering helpers

File: `src/ui/render.rs`

Add helper functions:

```rust
fn render_markdown_content(content: &str, base_style: Style) -> Vec<Line<'static>>
fn render_plain_content(content: &str) -> Vec<Line<'static>>
fn indent_rendered_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>>
```

Use Markdown rendering for:

- `TranscriptKind::User`
- `TranscriptKind::Assistant`

Use plain rendering for:

- `TranscriptKind::Tool`
- `TranscriptKind::Reasoning`
- `TranscriptKind::Status`
- `TranscriptKind::Error`

### Markdown subset

Support these elements:

- Paragraphs
- Soft/hard breaks
- Headings
- Bullet lists
- Ordered lists
- Fenced/indented code blocks
- Inline code
- Emphasis
- Strong text
- Blockquotes
- Links as visible text, optionally followed by dim URL if useful

Suggested visual treatment:

- Heading: bold, maybe same role color
- Bullet: `• `
- Ordered item: `1. `, `2. `
- Code block: preserve text lines, dim or gray style, with indentation
- Inline code: contrasting style, e.g. yellow or gray
- Blockquote: prefix `│ `, dim style

Important: continue sanitizing rendered text with `sanitize_line()` or equivalent character filtering.

## 3. Integrate Markdown renderer into transcript rendering

Current pattern in `transcript_lines_from()`:

```rust
for line in content.lines() {
    lines.push(Line::raw(format!("  {}", sanitize_line(line))));
}
```

Replace with logic like:

```rust
let rendered = match block.kind {
    TranscriptKind::User | TranscriptKind::Assistant => {
        render_markdown_content(&content, style_for(&block.kind))
    }
    _ => render_plain_content(&content),
};
lines.extend(indent_rendered_lines(rendered));
```

Ensure empty content still produces no body lines.

## 4. Improve collapsed tool result summaries

File: `src/ui/render.rs`

Change `display_content()` so completed tool blocks do not disappear when tools are collapsed.

Add:

```rust
fn collapsed_tool_summary(content: &str) -> String
```

Suggested output:

```text
42 lines · 3.1 KB · tool output hidden
```

Rules:

- Count lines with `content.lines().count()`.
- Count bytes with `content.len()`.
- Use human-readable byte formatting.
- If content is empty, show `no output`.
- Preserve current live streamed tool behavior: live output should remain visible even when full tools are hidden.

Example behavior:

Collapsed:

```text
· read ✓ (abc123)
  120 lines · 8.4 KB · tool output hidden
```

Expanded:

```text
· read ✓ (abc123)
  <full content>
```

## 5. Add tool argument summarization

File: `src/app.rs`

Add:

```rust
fn summarize_tool_arguments(name: &str, args: &serde_json::Value) -> String
```

Use this in `blocks_from_conversation()` for assistant tool-call blocks instead of always pretty-printing raw JSON.

Fallback to pretty JSON if a tool is unknown or arguments do not match expected shape.

### Recommended summaries

#### `read`

```text
file: src/ui/render.rs
lines: 1–120
```

If no range:

```text
file: src/ui/render.rs
```

#### `write`

```text
file: src/lib.rs
bytes: 1.8 KB
```

If content length is unavailable, omit bytes.

#### `edit`

Use `edits`, not `replacements`.

```text
file: src/ui/render.rs
edits: 2
```

If only one edit:

```text
file: src/ui/render.rs
edits: 1
```

#### `shell`

```text
command: cargo test
```

#### `grep`

```text
query: transcript
path: src
```

If include/exclude globs exist, include them only if concise.

#### `ls`

```text
path: src/ui
```

#### Unknown tool fallback

Pretty JSON:

```rust
serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string())
```

## 6. Normalize tool titles

File: `src/app.rs`

Current loaded transcript titles are close to good:

- Invocation: `{tool} … ({id})`
- Success: `{tool} ✓ ({id})`
- Failure: `{tool} ✗ ({id})`

Keep these, but audit live event handling to ensure live-created blocks match loaded blocks after reload.

Preferred title forms:

```text
· read … (abc123)       # pending/running invocation
· read ✓ (abc123)       # successful result
! read ✗ (abc123)       # failed result, via TranscriptKind::Error
· shell cancelled (abc123)
· approval required (abc123)
```

Do not include verbose implementation terms in titles.

## 7. Tests

Add or update tests in `src/ui/render.rs`:

1. Assistant Markdown heading/list/code renders into multiple lines.
2. User Markdown uses Markdown rendering.
3. Tool output collapsed summary appears when `show_full_tools == false`.
4. Tool output is fully visible when `show_full_tools == true`.
5. Live streamed tool output remains visible even when tools are collapsed.
6. Scroll calculation still counts rendered Markdown lines.

Add tests in `src/app.rs` for `summarize_tool_arguments()`:

1. `edit` summary uses `edits`, not `replacements`.
2. `shell` summary uses `command`.
3. `read` summary uses `file` and `lines`.
4. Unknown tool falls back to pretty JSON.

## 8. Manual QA checklist

Run:

```bash
cargo fmt
cargo test
```

Then manually verify in the TUI:

- User Markdown renders cleanly.
- Assistant Markdown renders cleanly.
- Bullets and code blocks look acceptable in narrow terminals.
- Tool calls are scannable without expanding full output.
- `/tools` or equivalent full-tool toggle still shows complete output.
- Reloaded conversations and live conversations show consistent tool formatting.
- Error tool results are visually distinct.

## Suggested Commit Breakdown

1. Add Markdown rendering dependency and helpers.
2. Switch user/assistant transcript blocks to Markdown rendering.
3. Add collapsed tool result summaries.
4. Add tool argument summaries with developer-native wording.
5. Normalize live/reloaded tool titles.
6. Add tests and polish.

## Acceptance Criteria

- User and assistant messages render common Markdown elements in the transcript.
- Tool invocations no longer default to noisy JSON for known built-in tools.
- `edit` tool summaries say `edits`, not `replacements`.
- Collapsed tool outputs show useful metadata instead of disappearing.
- Existing tool expansion behavior remains available.
- `cargo fmt` and `cargo test` pass.
