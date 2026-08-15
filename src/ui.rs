use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap,
    },
};

use crate::{
    app::{App, InputMode, Tab},
    model::{ApplicationStatus, Job},
    theme,
};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme::base()), area);

    let shell = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(14),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(frame, app, shell[0]);
    render_content(frame, app, shell[1]);
    render_footer(frame, app, shell[2]);

    if app.input_mode == InputMode::Search {
        render_search(frame, app, centered_rect(62, 7, area));
    }
    if app.show_help {
        render_help(frame, centered_rect(68, 22, area));
    }
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Length(30), Constraint::Min(36)])
        .margin(1)
        .split(area);
    let tracked: usize = ApplicationStatus::ALL
        .iter()
        .map(|status| app.jobs.iter().filter(|job| job.status == *status).count())
        .sum();

    let brand = Paragraph::new(Line::from(vec![
        Span::styled("◆ ", Style::new().fg(theme::ACCENT)),
        Span::styled(
            "OpenJobScout",
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {tracked} tracked"), Style::new().fg(theme::FAINT)),
    ]));
    frame.render_widget(brand, chunks[0]);

    let titles: Vec<Line<'_>> = Tab::ALL
        .iter()
        .map(|tab| {
            let count = app.tab_count(*tab);
            Line::from(format!(" {}  {count} ", tab.label()))
        })
        .collect();
    let selected = Tab::ALL
        .iter()
        .position(|tab| *tab == app.active_tab)
        .unwrap_or_default();
    let tabs = Tabs::new(titles)
        .select(selected)
        .divider(Span::styled("  ", Style::new().fg(theme::FAINT)))
        .style(Style::new().fg(theme::MUTED))
        .highlight_style(
            Style::new()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    frame.render_widget(tabs, chunks[1]);
}

fn render_content(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if area.width >= 96 {
        let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .spacing(1)
            .split(area);
        render_job_list(frame, app, columns[0]);
        render_job_detail(frame, app.selected_job(), columns[1]);
    } else {
        let rows = Layout::vertical([Constraint::Percentage(48), Constraint::Percentage(52)])
            .spacing(1)
            .split(area);
        render_job_list(frame, app, rows[0]);
        render_job_detail(frame, app.selected_job(), rows[1]);
    }
}

fn render_job_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let visible = app.visible_indices();
    let title = if app.search_query.is_empty() {
        format!(" {} ", app.active_tab.label().to_uppercase())
    } else {
        format!(
            " {} · /{} ",
            app.active_tab.label().to_uppercase(),
            app.search_query
        )
    };
    let block = panel(title, true);

    if visible.is_empty() {
        let empty = Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from(Span::styled("No matching jobs", theme::heading())),
            Line::from(""),
            Line::from(Span::styled(
                if app.search_query.is_empty() {
                    "Try another pipeline tab."
                } else {
                    "Press Esc to clear the search."
                },
                theme::muted(),
            )),
        ]))
        .alignment(Alignment::Center)
        .block(block)
        .style(theme::surface());
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem<'_>> = visible
        .iter()
        .map(|index| job_list_item(&app.jobs[*index]))
        .collect();
    let list = List::new(items)
        .block(block)
        .style(theme::surface())
        .highlight_style(theme::selected())
        .highlight_symbol("  ● ")
        .repeat_highlight_symbol(true);
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn job_list_item(job: &Job) -> ListItem<'_> {
    let score_color = match job.score {
        90..=100 => theme::GREEN,
        80..=89 => theme::CYAN,
        70..=79 => theme::YELLOW,
        _ => theme::MUTED,
    };
    let first = Line::from(vec![
        Span::styled(
            format!("{:>3}", job.score),
            Style::new().fg(score_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            job.title.clone(),
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
    ]);
    let second = Line::from(vec![
        Span::raw("     "),
        Span::styled(job.company.clone(), Style::new().fg(theme::MUTED)),
        Span::styled("  ·  ", Style::new().fg(theme::FAINT)),
        Span::styled(job.work_mode.label(), Style::new().fg(theme::MUTED)),
        Span::styled("  ·  ", Style::new().fg(theme::FAINT)),
        Span::styled(job.posted.clone(), Style::new().fg(theme::FAINT)),
    ]);
    ListItem::new(vec![first, second])
}

fn render_job_detail(frame: &mut Frame<'_>, job: Option<&Job>, area: Rect) {
    let Some(job) = job else {
        let empty = Paragraph::new("Select a job to see its details")
            .alignment(Alignment::Center)
            .block(panel(" JOB DETAILS ".into(), false))
            .style(theme::surface());
        frame.render_widget(empty, area);
        return;
    };

    let outer = panel(" JOB DETAILS ".into(), false);
    let inner = outer.inner(area);
    frame.render_widget(outer.style(theme::surface()), area);

    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .margin(1)
    .split(inner);

    let salary = job.salary.as_deref().unwrap_or("Salary not published");
    let header = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            job.title.clone(),
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            job.company.clone(),
            Style::new().fg(theme::ACCENT),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{}  ", job.location), Style::new().fg(theme::MUTED)),
            Span::styled("•  ", Style::new().fg(theme::FAINT)),
            Span::styled(salary, Style::new().fg(theme::GREEN)),
        ]),
    ]));
    frame.render_widget(header, rows[0]);

    let gauge = Gauge::default()
        .block(Block::default().title(Span::styled(" MATCH ", theme::accent())))
        .gauge_style(Style::new().fg(theme::ACCENT).bg(theme::SURFACE_ALT))
        .label(Span::styled(
            format!("{} · excellent fit", job.score),
            theme::heading(),
        ))
        .ratio(f64::from(job.score) / 100.0);
    frame.render_widget(gauge, rows[1]);

    let metadata = Paragraph::new(Line::from(vec![
        status_badge(job.status),
        Span::raw("   "),
        Span::styled(job.verification.clone(), Style::new().fg(theme::GREEN)),
        Span::styled("  via  ", Style::new().fg(theme::FAINT)),
        Span::styled(job.source.clone(), Style::new().fg(theme::MUTED)),
        Span::styled("   ·   ", Style::new().fg(theme::FAINT)),
        Span::styled(job.id.clone(), Style::new().fg(theme::FAINT)),
    ]));
    frame.render_widget(metadata, rows[2]);

    let skills = job
        .skills
        .iter()
        .map(|skill| format!(" {skill} "))
        .collect::<Vec<_>>()
        .join("  ");
    let concerns = if job.concerns.is_empty() {
        "No notable concerns".to_string()
    } else {
        job.concerns
            .iter()
            .map(|concern| format!("⚠ {concern}"))
            .collect::<Vec<_>>()
            .join("   ")
    };
    let body = Paragraph::new(Text::from(vec![
        Line::from(Span::styled("WHY IT MATCHES", theme::accent())),
        Line::from(Span::styled(skills, Style::new().fg(theme::CYAN))),
        Line::from(""),
        Line::from(Span::styled("WATCH", theme::accent())),
        Line::from(Span::styled(concerns, Style::new().fg(theme::RED))),
        Line::from(""),
        Line::from(Span::styled("ABOUT THE ROLE", theme::accent())),
        Line::from(job.description.clone()),
        Line::from(""),
        Line::from(Span::styled("LISTING", theme::accent())),
        Line::from(Span::styled(job.url.clone(), Style::new().fg(theme::MUTED))),
    ]))
    .style(theme::surface())
    .wrap(Wrap { trim: true });
    frame.render_widget(body, rows[4]);

    let actions = Paragraph::new(Line::from(vec![
        keycap("R"),
        Span::styled(" reviewed   ", theme::muted()),
        keycap("A"),
        Span::styled(" applied   ", theme::muted()),
        keycap("I"),
        Span::styled(" interview   ", theme::muted()),
        keycap("X"),
        Span::styled(" reject   ", theme::muted()),
        keycap("O"),
        Span::styled(" offer", theme::muted()),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(actions, rows[5]);
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Min(40), Constraint::Length(34)])
        .margin(1)
        .split(area);
    let shortcuts = Paragraph::new(Line::from(vec![
        keycap("↑↓"),
        Span::styled(" move   ", theme::muted()),
        keycap("←→"),
        Span::styled(" tabs   ", theme::muted()),
        keycap("/"),
        Span::styled(" search   ", theme::muted()),
        keycap("?"),
        Span::styled(" help   ", theme::muted()),
        keycap("Q"),
        Span::styled(" quit", theme::muted()),
    ]));
    frame.render_widget(shortcuts, chunks[0]);

    let message = app
        .notice
        .as_deref()
        .unwrap_or("Local-first · no account required");
    let notice = Paragraph::new(message)
        .style(Style::new().fg(theme::FAINT))
        .alignment(Alignment::Right);
    frame.render_widget(notice, chunks[1]);
}

fn render_search(frame: &mut Frame<'_>, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .style(Style::new().bg(theme::SURFACE))
        .title(Span::styled(" SEARCH JOBS ", theme::accent()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = if app.search_query.is_empty() {
        Line::from(Span::styled(
            "Type a title, company, skill…",
            theme::muted(),
        ))
    } else {
        Line::from(vec![
            Span::styled("/ ", Style::new().fg(theme::ACCENT)),
            Span::styled(
                app.search_query.clone(),
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▌", Style::new().fg(theme::ACCENT)),
        ])
    };
    let search = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().fg(theme::BORDER)),
        )
        .style(theme::surface());
    frame.render_widget(search, inner);
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .style(Style::new().bg(theme::SURFACE))
        .title(Span::styled(" SHORTCUTS ", theme::accent()));
    let help = Paragraph::new(Text::from(vec![
        Line::from(vec![
            keycap("↑ / K"),
            Span::styled("  Previous job", theme::muted()),
        ]),
        Line::from(vec![
            keycap("↓ / J"),
            Span::styled("  Next job", theme::muted()),
        ]),
        Line::from(vec![
            keycap("← / H"),
            Span::styled("  Previous tab", theme::muted()),
        ]),
        Line::from(vec![
            keycap("→ / L"),
            Span::styled("  Next tab", theme::muted()),
        ]),
        Line::from(""),
        Line::from(vec![
            keycap("/"),
            Span::styled("      Search instantly", theme::muted()),
        ]),
        Line::from(vec![
            keycap("Esc"),
            Span::styled("    Clear search", theme::muted()),
        ]),
        Line::from(""),
        Line::from(vec![
            keycap("R"),
            Span::styled("      Mark reviewed", theme::muted()),
        ]),
        Line::from(vec![
            keycap("A"),
            Span::styled("      Mark applied", theme::muted()),
        ]),
        Line::from(vec![
            keycap("I"),
            Span::styled("      Mark interview", theme::muted()),
        ]),
        Line::from(vec![
            keycap("X"),
            Span::styled("      Mark rejected", theme::muted()),
        ]),
        Line::from(vec![
            keycap("O"),
            Span::styled("      Mark offer", theme::muted()),
        ]),
        Line::from(vec![
            keycap("C"),
            Span::styled("      Mark closed", theme::muted()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press ? or Esc to close",
            Style::new().fg(theme::FAINT),
        )),
    ]))
    .block(block)
    .style(theme::surface())
    .wrap(Wrap { trim: true });
    frame.render_widget(help, area);
}

fn panel(title: String, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if active {
            theme::BORDER_ACTIVE
        } else {
            theme::BORDER
        }))
        .style(theme::surface())
        .title(Span::styled(
            title,
            if active {
                theme::accent()
            } else {
                theme::muted()
            },
        ))
}

fn status_badge(status: ApplicationStatus) -> Span<'static> {
    Span::styled(
        format!(" {} ", status.label()),
        Style::new()
            .fg(theme::BACKGROUND)
            .bg(status.color())
            .add_modifier(Modifier::BOLD),
    )
}

fn keycap(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::new()
            .fg(theme::TEXT)
            .bg(theme::SURFACE_ALT)
            .add_modifier(Modifier::BOLD),
    )
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(height) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(area.width.saturating_sub(width) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn full_layout_renders_without_error() {
        let app = App::default();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("OpenJobScout"));
        assert!(text.contains("Northstar Labs"));
        assert!(text.contains("WHY IT MATCHES"));
    }

    #[test]
    fn compact_layout_renders_without_error() {
        let app = App::default();
        let backend = TestBackend::new(78, 34);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn search_overlay_renders_query() {
        let app = App {
            input_mode: InputMode::Search,
            search_query: "python".into(),
            ..Default::default()
        };
        let backend = TestBackend::new(110, 38);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("SEARCH JOBS"));
        assert!(text.contains("python"));
    }
}
