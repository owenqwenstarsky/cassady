use anyhow::{bail, Result};
use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};
use std::collections::BTreeSet;
use std::io::{self, Write};

const MAX_VISIBLE_ITEMS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub detail: Option<String>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
        }
    }

    pub fn with_detail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: Some(detail.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Menu {
    title: String,
    items: Vec<MenuItem>,
    visible_items: usize,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
            visible_items: MAX_VISIBLE_ITEMS,
        }
    }

    pub fn with_visible_items(mut self, visible_items: usize) -> Self {
        self.visible_items = visible_items.max(1);
        self
    }

    pub fn select_one(&self, initial: usize) -> Result<usize> {
        if self.items.is_empty() {
            bail!("menu has no items");
        }
        let mut session = MenuSession::enter()?;
        let mut highlighted = initial.min(self.items.len() - 1);
        let mut top = 0usize;
        let mut footer = String::new();
        adjust_view(&mut top, highlighted, self.visible_items, self.items.len());

        loop {
            session.render(self, highlighted, top, None, &footer)?;
            footer.clear();
            match event::read()? {
                Event::Key(key)
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c') =>
                {
                    bail!("menu cancelled");
                }
                Event::Key(key) => match key.code {
                    KeyCode::Esc => bail!("menu cancelled"),
                    KeyCode::Up | KeyCode::Char('k') => {
                        highlighted = highlighted.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        highlighted = (highlighted + 1).min(self.items.len() - 1);
                    }
                    KeyCode::Home => highlighted = 0,
                    KeyCode::End => highlighted = self.items.len() - 1,
                    KeyCode::PageUp => {
                        highlighted = highlighted.saturating_sub(self.visible_items);
                    }
                    KeyCode::PageDown => {
                        highlighted = (highlighted + self.visible_items).min(self.items.len() - 1);
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => return Ok(highlighted),
                    _ => {}
                },
                _ => {}
            }
            adjust_view(&mut top, highlighted, self.visible_items, self.items.len());
        }
    }

    pub fn select_many(
        &self,
        initially_selected: &BTreeSet<usize>,
        require_one: bool,
    ) -> Result<Vec<usize>> {
        if self.items.is_empty() {
            bail!("menu has no items");
        }
        let mut session = MenuSession::enter()?;
        let mut highlighted = 0usize;
        let mut top = 0usize;
        let mut selected: BTreeSet<usize> = initially_selected
            .iter()
            .copied()
            .filter(|idx| *idx < self.items.len())
            .collect();
        let mut footer = String::new();

        loop {
            session.render(self, highlighted, top, Some(&selected), &footer)?;
            footer.clear();
            match event::read()? {
                Event::Key(key)
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c') =>
                {
                    bail!("menu cancelled");
                }
                Event::Key(key) => match key.code {
                    KeyCode::Esc => bail!("menu cancelled"),
                    KeyCode::Up | KeyCode::Char('k') => {
                        highlighted = highlighted.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        highlighted = (highlighted + 1).min(self.items.len() - 1);
                    }
                    KeyCode::Home => highlighted = 0,
                    KeyCode::End => highlighted = self.items.len() - 1,
                    KeyCode::PageUp => {
                        highlighted = highlighted.saturating_sub(self.visible_items);
                    }
                    KeyCode::PageDown => {
                        highlighted = (highlighted + self.visible_items).min(self.items.len() - 1);
                    }
                    KeyCode::Char(' ') => {
                        if !selected.insert(highlighted) {
                            selected.remove(&highlighted);
                        }
                    }
                    KeyCode::Enter => {
                        if require_one && selected.is_empty() {
                            footer =
                                "Select at least one item with Space, then press Enter.".into();
                        } else {
                            return Ok(selected.into_iter().collect());
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
            adjust_view(&mut top, highlighted, self.visible_items, self.items.len());
        }
    }
}

fn adjust_view(top: &mut usize, highlighted: usize, visible_items: usize, len: usize) {
    let visible_items = visible_items.max(1).min(len.max(1));
    if highlighted < *top {
        *top = highlighted;
    } else if highlighted >= *top + visible_items {
        *top = highlighted + 1 - visible_items;
    }
}

#[derive(Debug, Clone)]
pub struct TextPrompt {
    title: String,
    default: Option<String>,
    required: bool,
}

impl TextPrompt {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            default: None,
            required: false,
        }
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn prompt(&self) -> Result<String> {
        let mut session = TextSession::enter()?;
        let mut value = self.default.clone().unwrap_or_default();
        let mut footer = String::new();

        loop {
            session.render(self, &value, &footer)?;
            footer.clear();
            match event::read()? {
                Event::Key(key)
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c') =>
                {
                    bail!("prompt cancelled");
                }
                Event::Key(key)
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('u') =>
                {
                    value.clear();
                }
                Event::Key(key) => match key.code {
                    KeyCode::Esc => bail!("prompt cancelled"),
                    KeyCode::Enter => {
                        let trimmed = value.trim();
                        if !trimmed.is_empty() {
                            session.finish()?;
                            return Ok(trimmed.to_string());
                        }
                        if let Some(default) = &self.default {
                            session.finish()?;
                            return Ok(default.clone());
                        }
                        if self.required {
                            footer = "Value is required.".into();
                        } else {
                            session.finish()?;
                            return Ok(String::new());
                        }
                    }
                    KeyCode::Backspace => {
                        value.pop();
                    }
                    KeyCode::Char(ch) => {
                        if !key.modifiers.contains(KeyModifiers::CONTROL) {
                            value.push(ch);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

struct TextSession {
    rendered_lines: u16,
    finished: bool,
}

impl TextSession {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), Show)?;
        Ok(Self {
            rendered_lines: 0,
            finished: false,
        })
    }

    fn render(&mut self, prompt: &TextPrompt, value: &str, footer: &str) -> Result<()> {
        let mut out = io::stdout();
        if self.rendered_lines > 0 {
            let lines_up = self.rendered_lines.saturating_sub(1);
            if lines_up > 0 {
                queue!(out, MoveUp(lines_up))?;
            }
            queue!(out, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
        } else {
            queue!(out, MoveToColumn(0))?;
        }

        queue!(
            out,
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            Print("? "),
            ResetColor,
            SetAttribute(Attribute::Bold),
            Print(&prompt.title),
            SetAttribute(Attribute::Reset),
            Print("\r\n")
        )?;

        let help = if !footer.is_empty() {
            format!("  ! {footer}")
        } else if prompt.default.is_some() {
            "  default prefilled · Enter submit · Ctrl-U clear · Esc cancel".to_string()
        } else {
            "  Enter submit · Esc cancel".to_string()
        };
        let help_color = if footer.is_empty() {
            Color::DarkGrey
        } else {
            Color::Yellow
        };
        queue!(
            out,
            SetForegroundColor(help_color),
            Print(help),
            ResetColor,
            Print("\r\n"),
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            Print("  › "),
            ResetColor,
            SetAttribute(Attribute::Reset),
            Print(value)
        )?;

        out.flush()?;
        self.rendered_lines = 3;
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        if !self.finished {
            let mut out = io::stdout();
            queue!(out, Print("\r\n"))?;
            out.flush()?;
            self.finished = true;
        }
        Ok(())
    }
}

impl Drop for TextSession {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

struct MenuSession {
    rendered_lines: u16,
}

impl MenuSession {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), Hide)?;
        Ok(Self { rendered_lines: 0 })
    }

    fn render(
        &mut self,
        menu: &Menu,
        highlighted: usize,
        top: usize,
        selected: Option<&BTreeSet<usize>>,
        footer: &str,
    ) -> Result<()> {
        let mut out = io::stdout();
        if self.rendered_lines > 0 {
            queue!(
                out,
                MoveUp(self.rendered_lines),
                MoveToColumn(0),
                Clear(ClearType::FromCursorDown)
            )?;
        } else {
            queue!(out, MoveToColumn(0))?;
        }

        queue!(
            out,
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            Print("? "),
            ResetColor,
            SetAttribute(Attribute::Bold),
            Print(&menu.title),
            SetAttribute(Attribute::Reset),
            Print("\r\n")
        )?;
        let help = if selected.is_some() {
            "  ↑/↓ move · Space toggle · Enter submit · Esc cancel"
        } else {
            "  ↑/↓ move · Enter submit · Esc cancel"
        };
        queue!(
            out,
            SetForegroundColor(Color::DarkGrey),
            Print(help),
            ResetColor,
            Print("\r\n")
        )?;

        let len = menu.items.len();
        let visible = menu.visible_items.max(1).min(len);
        let end = (top + visible).min(len);
        let mut lines = 2u16;

        if top > 0 {
            queue!(
                out,
                SetForegroundColor(Color::DarkGrey),
                Print(format!("  ↑ {} more\r\n", top)),
                ResetColor
            )?;
            lines += 1;
        }

        for idx in top..end {
            let item = &menu.items[idx];
            let is_highlighted = idx == highlighted;
            if is_highlighted {
                queue!(
                    out,
                    SetForegroundColor(Color::Cyan),
                    SetAttribute(Attribute::Bold),
                    Print("  › ")
                )?;
            } else {
                queue!(out, Print("    "))?;
            }

            if let Some(selected) = selected {
                let mark = if selected.contains(&idx) {
                    "[x] "
                } else {
                    "[ ] "
                };
                queue!(out, Print(mark))?;
            }

            queue!(out, Print(&item.label))?;
            if let Some(detail) = &item.detail {
                queue!(
                    out,
                    SetForegroundColor(Color::DarkGrey),
                    Print("  — "),
                    Print(detail),
                    ResetColor
                )?;
                if is_highlighted {
                    queue!(
                        out,
                        SetForegroundColor(Color::Cyan),
                        SetAttribute(Attribute::Bold)
                    )?;
                }
            }
            queue!(
                out,
                SetAttribute(Attribute::Reset),
                ResetColor,
                Print("\r\n")
            )?;
            lines += 1;
        }

        if end < len {
            queue!(
                out,
                SetForegroundColor(Color::DarkGrey),
                Print(format!("  ↓ {} more\r\n", len - end)),
                ResetColor
            )?;
            lines += 1;
        }

        if !footer.is_empty() {
            queue!(
                out,
                SetForegroundColor(Color::Yellow),
                Print("  ! "),
                Print(footer),
                ResetColor,
                Print("\r\n")
            )?;
            lines += 1;
        }

        out.flush()?;
        self.rendered_lines = lines;
        Ok(())
    }
}

impl Drop for MenuSession {
    fn drop(&mut self) {
        let mut out = io::stdout();
        if self.rendered_lines > 0 {
            let _ = queue!(
                out,
                MoveUp(self.rendered_lines),
                MoveToColumn(0),
                Clear(ClearType::FromCursorDown)
            );
        }
        let _ = execute!(out, Show);
        let _ = out.flush();
        let _ = terminal::disable_raw_mode();
    }
}
