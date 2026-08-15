use ratatui::style::{Color, Modifier, Style};

pub const BACKGROUND: Color = Color::Rgb(13, 13, 18);
pub const SURFACE: Color = Color::Rgb(22, 22, 30);
pub const SURFACE_ALT: Color = Color::Rgb(29, 29, 39);
pub const BORDER: Color = Color::Rgb(55, 55, 72);
pub const BORDER_ACTIVE: Color = Color::Rgb(129, 96, 255);
pub const TEXT: Color = Color::Rgb(239, 239, 245);
pub const MUTED: Color = Color::Rgb(143, 143, 160);
pub const FAINT: Color = Color::Rgb(96, 96, 112);
pub const ACCENT: Color = Color::Rgb(143, 112, 255);
pub const ACCENT_SOFT: Color = Color::Rgb(58, 44, 99);
pub const CYAN: Color = Color::Rgb(103, 215, 255);
pub const GREEN: Color = Color::Rgb(104, 211, 145);
pub const YELLOW: Color = Color::Rgb(246, 199, 96);
pub const RED: Color = Color::Rgb(241, 113, 126);

pub const fn base() -> Style {
    Style::new().fg(TEXT).bg(BACKGROUND)
}

pub const fn surface() -> Style {
    Style::new().fg(TEXT).bg(SURFACE)
}

pub const fn muted() -> Style {
    Style::new().fg(MUTED).bg(SURFACE)
}

pub const fn accent() -> Style {
    Style::new()
        .fg(ACCENT)
        .bg(SURFACE)
        .add_modifier(Modifier::BOLD)
}

pub const fn heading() -> Style {
    Style::new()
        .fg(TEXT)
        .bg(SURFACE)
        .add_modifier(Modifier::BOLD)
}

pub const fn selected() -> Style {
    Style::new()
        .fg(TEXT)
        .bg(ACCENT_SOFT)
        .add_modifier(Modifier::BOLD)
}
