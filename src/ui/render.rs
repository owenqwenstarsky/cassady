use crate::access::AccessMode;
use crate::config::ReasoningEffort;
use crate::ui::autofill::AutoFillMenu;
use crate::ui::theme;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};
use std::path::Path;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone)]
pub enum TranscriptKind {
    User,
    Assistant,
    Reasoning,
    Tool,
    Status,
    Error,
}

#[derive(Debug, Clone)]
pub struct TranscriptBlock {
    pub kind: TranscriptKind,
    pub title: String,
    pub content: String,
}

#[derive(Debug)]
pub struct RenderState<'a> {
    pub app_name: &'a str,
    pub chat_id: &'a str,
    pub model: &'a str,
    pub mode: AccessMode,
    pub cwd: &'a Path,
    pub transcript: &'a [TranscriptBlock],
    pub input: &'a str,
    pub status: &'a str,
    pub busy: bool,
    pub show_full_tools: bool,
    pub show_reasoning: bool,
    pub reasoning_effort: ReasoningEffort,
    pub scroll: u16,
    pub autofill: Option<&'a AutoFillMenu>,
}

pub fn render(f: &mut Frame<'_>, state: &RenderState<'_>) {
    let menu_height = autofill_height(state.autofill);
    let chunks = main_layout(f.area(), state.input, menu_height);

    let lines = transcript_lines_from(
        state.transcript,
        state.show_full_tools,
        state.show_reasoning,
    );
    let effective_scroll = state.scroll.min(max_transcript_scroll(
        state.transcript,
        state.show_full_tools,
        state.show_reasoning,
        chunks[0],
    ));
    let transcript = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0));
    f.render_widget(transcript, chunks[0]);

    if let Some(menu) = state.autofill {
        render_autofill_menu(f, chunks[1], menu);
    }

    let input =
        Paragraph::new(Text::from(input_lines(state.input, state.busy))).wrap(Wrap { trim: false });
    f.render_widget(input, chunks[2]);

    let footer = truncate_end(&footer_text(state), chunks[3].width as usize);
    f.render_widget(Paragraph::new(footer).style(theme::footer()), chunks[3]);
}

pub fn transcript_area(area: Rect, input: &str) -> Rect {
    main_layout(area, input, 0)[0]
}

pub fn max_transcript_scroll(
    transcript: &[TranscriptBlock],
    show_full_tools: bool,
    show_reasoning: bool,
    area: Rect,
) -> u16 {
    // Paragraph::scroll is measured in rendered rows, not logical lines. Long
    // tool output and assistant messages wrap, so count rows with the same
    // word-wrapping behavior ratatui uses for Paragraph::wrap(trim: false).
    let content_width = area.width.max(1) as usize;
    let row_count = transcript_lines_from(transcript, show_full_tools, show_reasoning)
        .iter()
        .map(|line| ratatui_wrapped_row_count(&line.to_string(), content_width))
        .sum::<usize>();
    let viewport_rows = area.height as usize;
    row_count
        .saturating_sub(viewport_rows)
        .min(u16::MAX as usize) as u16
}

fn main_layout(area: Rect, input: &str, menu_height: u16) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(menu_height),
            Constraint::Length(input_height(input, area.width)),
            Constraint::Length(1),
        ])
        .split(area)
}

fn autofill_height(menu: Option<&AutoFillMenu>) -> u16 {
    menu.map(|menu| menu.items.len().min(6) as u16).unwrap_or(0)
}

fn render_autofill_menu(f: &mut Frame<'_>, area: Rect, menu: &AutoFillMenu) {
    if area.height == 0 || menu.items.is_empty() {
        return;
    }

    let visible_items = menu.items.len().min(area.height as usize).min(6);
    let selected = menu.selected_index().unwrap_or(0);
    let start = if selected >= visible_items {
        selected + 1 - visible_items
    } else {
        0
    };
    let end = (start + visible_items).min(menu.items.len());
    let width = area.width as usize;
    let mut lines = Vec::new();

    for idx in start..end {
        let item = &menu.items[idx];
        let marker = if idx == selected { "›" } else { " " };
        let mut text = format!("{marker} {}", item.label);
        if let Some(detail) = &item.detail {
            text.push_str("  ");
            text.push_str(detail);
        }
        let style = if idx == selected {
            theme::selection()
        } else {
            theme::menu()
        };
        lines.push(Line::styled(truncate_end(&text, width), style));
    }

    f.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn transcript_lines_from(
    transcript: &[TranscriptBlock],
    show_full_tools: bool,
    show_reasoning: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for block in transcript {
        if matches!(block.kind, TranscriptKind::Assistant) && block.content.trim().is_empty() {
            continue;
        }
        if matches!(block.kind, TranscriptKind::Reasoning) && !show_reasoning {
            continue;
        }

        let style = style_for(&block.kind);
        lines.push(Line::styled(display_heading(block, show_full_tools), style));

        let content = display_content(block, show_full_tools, show_reasoning);
        if !content.trim().is_empty() {
            let rendered = match block.kind {
                TranscriptKind::User | TranscriptKind::Assistant => {
                    render_markdown_content(&content, style)
                }
                _ => render_plain_content(&content),
            };
            lines.extend(indent_rendered_lines(rendered));
        }
        lines.push(Line::raw(""));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "Ask a question or request a code change.",
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines
}

fn render_plain_content(content: &str) -> Vec<Line<'static>> {
    content
        .lines()
        .map(|line| Line::raw(sanitize_line(line)))
        .collect()
}

fn render_markdown_content(content: &str, base_style: Style) -> Vec<Line<'static>> {
    let mut renderer = MarkdownRenderer::new(base_style);
    for event in Parser::new(content) {
        renderer.event(event);
    }
    renderer.finish()
}

struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    base_style: Style,
    pending_prefix: Option<String>,
    list_stack: Vec<ListState>,
    in_code_block: bool,
    in_heading: bool,
    in_blockquote: bool,
}

#[derive(Debug, Clone, Copy)]
struct ListState {
    next: Option<u64>,
}

impl MarkdownRenderer {
    fn new(base_style: Style) -> Self {
        Self {
            lines: Vec::new(),
            spans: Vec::new(),
            style_stack: vec![base_style],
            base_style,
            pending_prefix: None,
            list_stack: Vec::new(),
            in_code_block: false,
            in_heading: false,
            in_blockquote: false,
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_text(&text),
            Event::Code(code) => self.push_styled(&code, self.current_style().fg(Color::Yellow)),
            Event::SoftBreak | Event::HardBreak => self.flush_line(),
            Event::Rule => {
                self.flush_line();
                self.lines.push(Line::styled(
                    "────────",
                    self.base_style.fg(Color::DarkGray),
                ));
            }
            Event::Html(html) | Event::InlineHtml(html) => self.push_text(&html),
            Event::FootnoteReference(reference) => self.push_text(&format!("[{reference}]")),
            Event::TaskListMarker(checked) => self.push_text(if checked { "[x] " } else { "[ ] " }),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_line();
                self.in_heading = true;
                let marker = match level {
                    HeadingLevel::H1 => "# ",
                    HeadingLevel::H2 => "## ",
                    HeadingLevel::H3 => "### ",
                    _ => "#### ",
                };
                self.pending_prefix = Some(marker.into());
                self.push_style(self.current_style().add_modifier(Modifier::BOLD));
            }
            Tag::BlockQuote(_) => {
                self.flush_line();
                self.in_blockquote = true;
                self.pending_prefix = Some("│ ".into());
                self.push_style(self.current_style().fg(Color::DarkGray));
            }
            Tag::CodeBlock(kind) => {
                self.flush_line();
                self.in_code_block = true;
                let label = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(format!("```{lang}")),
                    _ => Some("```".to_string()),
                };
                if let Some(label) = label {
                    self.lines
                        .push(Line::styled(label, self.base_style.fg(Color::DarkGray)));
                }
                self.push_style(self.base_style.fg(Color::Gray));
            }
            Tag::List(start) => {
                self.flush_line();
                self.list_stack.push(ListState { next: start });
            }
            Tag::Item => {
                self.flush_line();
                let prefix = if let Some(list) = self.list_stack.last_mut() {
                    if let Some(n) = list.next {
                        list.next = Some(n + 1);
                        format!("{n}. ")
                    } else {
                        "• ".to_string()
                    }
                } else {
                    "• ".to_string()
                };
                self.pending_prefix = Some(prefix);
            }
            Tag::Emphasis => self.push_style(self.current_style().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(self.current_style().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => {
                self.push_style(self.current_style().add_modifier(Modifier::CROSSED_OUT))
            }
            Tag::Link { .. } => self.push_style(self.current_style().underlined()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_line(),
            TagEnd::Heading(_) => {
                self.flush_line();
                self.in_heading = false;
                self.pop_style();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_line();
                self.in_blockquote = false;
                self.pop_style();
            }
            TagEnd::CodeBlock => {
                self.flush_line();
                self.lines
                    .push(Line::styled("```", self.base_style.fg(Color::DarkGray)));
                self.in_code_block = false;
                self.pop_style();
            }
            TagEnd::List(_) => {
                self.flush_line();
                self.list_stack.pop();
            }
            TagEnd::Item => self.flush_line(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.pop_style();
            }
            _ => self.flush_line(),
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        if self.lines.is_empty() {
            render_plain_content("")
        } else {
            self.lines
        }
    }

    fn push_style(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn current_style(&self) -> Style {
        *self.style_stack.last().unwrap_or(&self.base_style)
    }

    fn push_text(&mut self, text: &str) {
        self.push_styled(text, self.current_style());
    }

    fn push_styled(&mut self, text: &str, style: Style) {
        for (idx, part) in text.split('\n').enumerate() {
            if idx > 0 {
                self.flush_line();
            }
            if part.is_empty() {
                continue;
            }
            self.ensure_prefix();
            self.spans.push(Span::styled(sanitize_line(part), style));
        }
    }

    fn ensure_prefix(&mut self) {
        if let Some(prefix) = self.pending_prefix.take() {
            self.spans.push(Span::styled(prefix, self.current_style()));
        }
    }

    fn flush_line(&mut self) {
        if self.spans.is_empty() {
            if let Some(prefix) = self.pending_prefix.take() {
                self.lines.push(Line::styled(prefix, self.current_style()));
            }
            return;
        }
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        if self.in_blockquote {
            self.pending_prefix = Some("│ ".into());
        }
    }
}

fn indent_rendered_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

fn input_lines(input: &str, busy: bool) -> Vec<Line<'static>> {
    let prefix = if busy { "… " } else { "› " };
    if input.is_empty() {
        return vec![Line::styled(prefix.to_string(), theme::input())];
    }

    let mut lines = Vec::new();
    for (idx, line) in input.lines().enumerate() {
        let marker = if idx == 0 { prefix } else { "  " };
        lines.push(Line::styled(
            format!("{marker}{}", sanitize_line(line)),
            theme::input(),
        ));
    }
    if input.ends_with('\n') {
        lines.push(Line::styled("  ", theme::input()));
    }
    lines
}

fn ratatui_wrapped_row_count(line: &str, content_width: usize) -> usize {
    // This mirrors ratatui's WordWrapper::process_input for Wrap { trim: false }
    // closely enough for scroll bounds. Details like whitespace-only wrapped
    // lines matter: if we undercount rows, the scroll bottom stops before the
    // final assistant message even though it is present in the transcript.
    let max_width = content_width.max(1);
    let mut rows = 0usize;
    let mut pending_line_has_symbols = false;
    let mut line_width = 0usize;
    let mut word_width = 0usize;
    let mut word_symbols = 0usize;
    let mut whitespace_width = 0usize;
    let mut whitespace_symbols = 0usize;
    let mut pending_whitespace = std::collections::VecDeque::new();
    let mut non_whitespace_previous = false;

    for ch in line.chars() {
        let symbol_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if symbol_width > max_width {
            continue;
        }
        let is_whitespace = ch.is_whitespace();
        let word_found = non_whitespace_previous && is_whitespace;
        let untrimmed_overflow = !pending_line_has_symbols
            && word_width
                .saturating_add(whitespace_width)
                .saturating_add(symbol_width)
                > max_width;

        if word_found || untrimmed_overflow {
            if whitespace_symbols > 0 {
                pending_line_has_symbols = true;
                line_width = line_width.saturating_add(whitespace_width);
            }
            if word_symbols > 0 {
                pending_line_has_symbols = true;
                line_width = line_width.saturating_add(word_width);
            }
            pending_whitespace.clear();
            whitespace_width = 0;
            whitespace_symbols = 0;
            word_width = 0;
            word_symbols = 0;
        }

        let line_full = line_width >= max_width;
        let pending_word_overflow = symbol_width > 0
            && line_width
                .saturating_add(whitespace_width)
                .saturating_add(word_width)
                >= max_width;

        if line_full || pending_word_overflow {
            rows = rows.saturating_add(1);
            let mut remaining_width = max_width.saturating_sub(line_width);
            line_width = 0;
            pending_line_has_symbols = false;

            while let Some(width) = pending_whitespace.front().copied() {
                if width > remaining_width {
                    break;
                }
                whitespace_width = whitespace_width.saturating_sub(width);
                whitespace_symbols = whitespace_symbols.saturating_sub(1);
                remaining_width = remaining_width.saturating_sub(width);
                pending_whitespace.pop_front();
            }

            if is_whitespace && pending_whitespace.is_empty() {
                continue;
            }
        }

        if is_whitespace {
            whitespace_width = whitespace_width.saturating_add(symbol_width);
            whitespace_symbols = whitespace_symbols.saturating_add(1);
            pending_whitespace.push_back(symbol_width);
        } else {
            word_width = word_width.saturating_add(symbol_width);
            word_symbols = word_symbols.saturating_add(1);
        }

        non_whitespace_previous = !is_whitespace;
    }

    if !pending_line_has_symbols && word_symbols == 0 && whitespace_symbols > 0 {
        rows = rows.saturating_add(1);
    }
    if whitespace_symbols > 0 || word_symbols > 0 {
        pending_line_has_symbols = true;
    }
    if pending_line_has_symbols {
        rows = rows.saturating_add(1);
    }
    rows.max(1)
}

fn style_for(kind: &TranscriptKind) -> Style {
    match kind {
        TranscriptKind::User => theme::user(),
        TranscriptKind::Assistant => theme::assistant(),
        TranscriptKind::Reasoning => theme::reasoning(),
        TranscriptKind::Tool => theme::tool(),
        TranscriptKind::Status => Style::default().fg(Color::DarkGray),
        TranscriptKind::Error => theme::error(),
    }
}

fn display_heading(block: &TranscriptBlock, show_full_tools: bool) -> String {
    let heading = heading_for(block);
    if is_collapsed_successful_tool_result(block, show_full_tools) {
        format!("{heading} · {}", collapsed_tool_summary(&block.content))
    } else {
        heading
    }
}

fn heading_for(block: &TranscriptBlock) -> String {
    match block.kind {
        TranscriptKind::User => "› you".into(),
        TranscriptKind::Assistant => "cass".into(),
        TranscriptKind::Reasoning => "· reasoning".into(),
        TranscriptKind::Tool => format!("· {}", block.title),
        TranscriptKind::Status => {
            if block.title.trim().is_empty() || block.title == "status" {
                "· status".into()
            } else {
                format!("· {}", block.title)
            }
        }
        TranscriptKind::Error => format!("! {}", block.title),
    }
}

fn display_content(block: &TranscriptBlock, show_full_tools: bool, show_reasoning: bool) -> String {
    if is_collapsed_successful_tool_result(block, show_full_tools) {
        String::new()
    } else if matches!(block.kind, TranscriptKind::Tool)
        && !show_full_tools
        && !is_live_tool_output(block)
    {
        collapsed_tool_summary(&block.content)
    } else if matches!(block.kind, TranscriptKind::Reasoning) && !show_reasoning {
        String::new()
    } else {
        block.content.clone()
    }
}

fn is_collapsed_successful_tool_result(block: &TranscriptBlock, show_full_tools: bool) -> bool {
    !show_full_tools
        && matches!(block.kind, TranscriptKind::Tool)
        && !is_live_tool_output(block)
        && block.title.contains('✓')
}

fn collapsed_tool_summary(content: &str) -> String {
    if content.is_empty() {
        return "no output".into();
    }
    let lines = content.lines().count();
    format!(
        "{} · {} · tool output hidden",
        pluralize(lines, "line"),
        human_bytes(content.len())
    )
}

fn pluralize(count: usize, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

fn human_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let bytes_f = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes_f < MB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{:.1} MB", bytes_f / MB)
    }
}

fn is_live_tool_output(block: &TranscriptBlock) -> bool {
    block.title.contains('…') && block.content.contains("streamed output:\n")
}

fn footer_text(state: &RenderState<'_>) -> String {
    let busy = if state.busy { "running" } else { "idle" };
    let mode = match state.mode {
        AccessMode::ReadOnly => "read-only",
        AccessMode::WorkspaceEdit => "workspace-edit",
        AccessMode::FullAccess => "full-access",
    };
    let mut parts = vec![
        state.app_name.to_ascii_lowercase(),
        mode.to_string(),
        busy.to_string(),
        short_model(state.model),
        short_path(state.cwd),
        short_chat_id(state.chat_id),
    ];
    if state.show_full_tools {
        parts.push("tools:full".into());
    }
    parts.push(format!("reasoning:{}", state.reasoning_effort));
    if state.show_reasoning {
        parts.push("reasoning:visible".into());
    }
    if !state.status.trim().is_empty() {
        parts.push(state.status.trim().to_string());
    }
    parts.join(" · ")
}

fn short_model(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).to_string()
}

fn short_chat_id(id: &str) -> String {
    if id.len() <= 18 {
        id.to_string()
    } else {
        format!("{}…{}", &id[..10], &id[id.len() - 4..])
    }
}

fn short_path(path: &Path) -> String {
    let display = if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", rest.display())
            }
        } else {
            path.display().to_string()
        }
    } else {
        path.display().to_string()
    };
    truncate_middle(&display, 40)
}

fn sanitize_line(line: &str) -> String {
    line.replace('\t', "    ")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn truncate_end(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    let mut out = s.chars().take(max - 1).collect::<String>();
    out.push('…');
    out
}

fn truncate_middle(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    let head = (max - 1) / 2;
    let tail = max - 1 - head;
    let start = s.chars().take(head).collect::<String>();
    let end = s
        .chars()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{start}…{end}")
}

fn input_height(input: &str, available_width: u16) -> u16 {
    // The first line has a 2-char prefix ("› " or "… ") and continuation
    // lines have a 2-char indent ("  "), so the content width is always
    // available_width - 2.  Count word-wrapped rows so that a single long
    // line grows the input area instead of overflowing horizontally.
    let prefix_width = 2usize;
    let content_width = available_width.saturating_sub(prefix_width as u16) as usize;
    let content_width = content_width.max(1);

    let mut rows = 0u16;
    let lines: Vec<&str> = if input.is_empty() {
        vec![""]
    } else {
        input.lines().collect()
    };
    for line in &lines {
        rows = rows.saturating_add(ratatui_wrapped_row_count(line, content_width) as u16);
    }
    if input.ends_with('\n') {
        rows = rows.saturating_add(1);
    }
    rows.max(1).clamp(1, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn assistant_markdown_renders_common_blocks() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Assistant,
            title: "response".into(),
            content: "## Plan\n\n- Read files\n- Apply `edits`\n\n```rust\nlet ok = true;\n```"
                .into(),
        }];

        let text = rendered_text(&transcript_lines_from(&transcript, false, false));

        assert!(text.contains("## Plan"));
        assert!(text.contains("• Read files"));
        assert!(text.contains("Apply edits"));
        assert!(text.contains("```rust"));
        assert!(text.contains("let ok = true;"));
    }

    #[test]
    fn user_markdown_is_rendered() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::User,
            title: "message".into(),
            content: "Please do **this**:\n1. Test".into(),
        }];

        let text = rendered_text(&transcript_lines_from(&transcript, false, false));

        assert!(text.contains("Please do this:"));
        assert!(text.contains("1. Test"));
    }

    #[test]
    fn collapsed_tool_output_shows_one_line_summary() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Tool,
            title: "read ✓ (call_1)".into(),
            content: "one\ntwo\nthree".into(),
        }];

        let rendered = transcript_lines_from(&transcript, false, false);
        let text = rendered_text(&rendered);

        assert_eq!(rendered.len(), 2); // heading plus spacer
        assert!(text.contains("read ✓ (call_1) · 3 lines"));
        assert!(!text.contains("one\ntwo\nthree"));
    }

    #[test]
    fn successful_ls_shows_summary_when_tools_are_collapsed() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Tool,
            title: "ls ✓ (call_1)".into(),
            content: "file1\nfile2".into(),
        }];

        let rendered = transcript_lines_from(&transcript, false, false);
        let text = rendered_text(&rendered);

        assert_eq!(rendered.len(), 2); // heading plus spacer
        assert!(text.contains("ls ✓ (call_1) · 2 lines"));
        assert!(!text.contains("file1\nfile2"));
    }

    #[test]
    fn successful_ls_is_visible_when_tools_are_expanded() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Tool,
            title: "ls ✓ (call_1)".into(),
            content: "file1\nfile2".into(),
        }];

        let text = rendered_text(&transcript_lines_from(&transcript, true, false));

        assert!(text.contains("ls ✓ (call_1)"));
        assert!(text.contains("file1"));
    }

    #[test]
    fn failed_tool_output_stays_visible_when_collapsed() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Error,
            title: "ls ✗ (call_1)".into(),
            content: "permission denied".into(),
        }];

        let text = rendered_text(&transcript_lines_from(&transcript, false, false));

        assert!(text.contains("ls ✗ (call_1)"));
        assert!(text.contains("permission denied"));
    }

    #[test]
    fn expanded_tool_output_shows_full_content() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Tool,
            title: "read ✓ (call_1)".into(),
            content: "one\ntwo".into(),
        }];

        let text = rendered_text(&transcript_lines_from(&transcript, true, false));

        assert!(text.contains("one\n  two"));
    }

    #[test]
    fn live_tool_output_stays_visible_when_collapsed() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Tool,
            title: "shell … (call_1)".into(),
            content: "streamed output:\nhello".into(),
        }];

        let text = rendered_text(&transcript_lines_from(&transcript, false, false));

        assert!(text.contains("streamed output"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn max_scroll_counts_wrapped_transcript_rows() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Assistant,
            title: "response".into(),
            content: "12345 12345 12345".into(),
        }];
        let area = Rect::new(0, 0, 12, 3);

        assert!(max_transcript_scroll(&transcript, false, false, area) > 0);
    }

    #[test]
    fn max_scroll_stays_zero_for_short_transcript() {
        let transcript = vec![TranscriptBlock {
            kind: TranscriptKind::Assistant,
            title: "response".into(),
            content: "Done.".into(),
        }];
        let area = Rect::new(0, 0, 80, 10);

        assert_eq!(max_transcript_scroll(&transcript, false, false, area), 0);
    }

    #[test]
    fn input_height_wraps_long_line() {
        // A single long line with no newlines should occupy more than 1 row
        // when the terminal is narrow.
        let long = "word ".repeat(40);
        assert!(input_height(&long, 20) > 1);
    }

    #[test]
    fn input_height_short_line_is_one_row() {
        assert_eq!(input_height("hello", 80), 1);
    }

    #[test]
    fn input_height_counts_explicit_newlines() {
        assert_eq!(input_height("line1\nline2\nline3", 80), 3);
    }

    #[test]
    fn input_height_clamps_at_six() {
        let long = "word ".repeat(400);
        assert_eq!(input_height(&long, 20), 6);
    }

    #[test]
    fn max_scroll_counts_whitespace_only_render_rows() {
        let content = (0..60)
            .map(|i| if i % 2 == 0 { "x" } else { "" })
            .collect::<Vec<_>>()
            .join("\n");
        let transcript = vec![
            TranscriptBlock {
                kind: TranscriptKind::Tool,
                title: "read ✓ (call_1)".into(),
                content,
            },
            TranscriptBlock {
                kind: TranscriptKind::Assistant,
                title: "response".into(),
                content: "Done.".into(),
            },
        ];
        let area = Rect::new(0, 0, 80, 5);
        let max = max_transcript_scroll(&transcript, true, false, area);

        assert!(max > 50, "max scroll was {max}");
    }
}
