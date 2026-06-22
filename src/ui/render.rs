use crate::access::AccessMode;
use crate::ui::autofill::AutoFillMenu;
use crate::ui::theme;
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
            Constraint::Length(input_height(input)),
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
        lines.push(Line::styled(heading_for(block), style));

        let content = display_content(block, show_full_tools, show_reasoning);
        if !content.trim().is_empty() {
            for line in content.lines() {
                lines.push(Line::raw(format!("  {}", sanitize_line(line))));
            }
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
    if matches!(block.kind, TranscriptKind::Tool) && !show_full_tools {
        String::new()
    } else if matches!(block.kind, TranscriptKind::Reasoning) && !show_reasoning {
        String::new()
    } else {
        block.content.clone()
    }
}

fn footer_text(state: &RenderState<'_>) -> String {
    let busy = if state.busy { "running" } else { "idle" };
    let mode = match state.mode {
        AccessMode::ReadOnly => "read-only",
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
    if state.show_reasoning {
        parts.push("reasoning:on".into());
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

fn input_height(input: &str) -> u16 {
    let mut lines = input.lines().count().max(1) as u16;
    if input.ends_with('\n') {
        lines = lines.saturating_add(1);
    }
    lines.clamp(1, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

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
