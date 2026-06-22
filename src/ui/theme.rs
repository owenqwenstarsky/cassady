use ratatui::style::{Color, Modifier, Style};

pub fn user() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn assistant() -> Style {
    Style::default().fg(Color::White)
}

pub fn reasoning() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn tool() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn error() -> Style {
    Style::default().fg(Color::Red)
}

pub fn input() -> Style {
    Style::default().fg(Color::White)
}

pub fn footer() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn menu() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn selection() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}
