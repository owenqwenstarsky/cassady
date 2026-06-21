use ratatui::style::{Color, Modifier, Style};

pub fn header() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}
pub fn user() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}
pub fn assistant() -> Style {
    Style::default().fg(Color::White)
}
pub fn tool() -> Style {
    Style::default().fg(Color::Yellow)
}
pub fn error() -> Style {
    Style::default().fg(Color::Red)
}
