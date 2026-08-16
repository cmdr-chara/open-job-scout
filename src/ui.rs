use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap,
    },
};

use crate::{
    app::{App, InputMode, Tab},
    model::{ApplicationStatus, Job, JobEvent},
    safety::terminal_text,
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

    match app.input_mode {
        InputMode::Search => render_text_input(
            frame,
            centered_rect(64, 7, area),
            " SEARCH JOBS ",
            "/ ",
            &app.search_query,
            "Type a title, company, location, note…",
        ),
        InputMode::Note => render_text_input(
            frame,
            centered_rect(72, 8, area),
            " ADD NOTE ",
            "N ",
            &app.note_buffer,
            "Add context for this application…",
        ),
        InputMode::Browse => {}
    }
    if app.show_help {
        render_help(frame, centered_rect(72, 25, area));
    }
    if app.show_history {
        render_history(frame, app, centered_rect(82, 26, area));
    }
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Length(31), Constraint::Min(38)])
        .margin(1)
        .split(area);
    let brand = Paragraph::new(Line::from(vec![
        Span::styled("◆ ", Style::new().fg(theme::ACCENT)),
        Span::styled(
            "OpenJobScout",
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {} tracked", app.jobs.len()),
            Style::new().fg(theme::FAINT),
        ),
    ]));
    frame.render_widget(brand, chunks[0]);

    let titles = Tab::ALL
        .iter()
        .map(|tab| Line::from(format!(" {}  {} ", tab.label(), app.tab_count(*tab))))
        .collect::<Vec<_>>();
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
    if area.width >= 98 {
        let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .spacing(1)
            .split(area);
        render_job_list(frame, app, columns[0]);
        render_job_detail(frame, app.selected_job(), columns[1]);
    } else {
        let rows = Layout::vertical([Constraint::Percentage(47), Constraint::Percentage(53)])
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
            terminal_text(&app.search_query)
        )
    };
    let block = panel(title, true);
    if visible.is_empty() {
        let message = if app.jobs.is_empty() {
            "No tracked jobs yet\n\nThe Rust UI is connected to your real tracker database.\nDiscovery will be ported next."
        } else if app.search_query.is_empty() {
            "Nothing in this pipeline view."
        } else {
            "No jobs match this search.\n\nPress Esc to clear it."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .block(block)
                .style(theme::surface())
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let items = visible
        .iter()
        .map(|index| job_list_item(&app.jobs[*index]))
        .collect::<Vec<_>>();
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
        score if score >= 90.0 => theme::GREEN,
        score if score >= 80.0 => theme::CYAN,
        score if score >= 70.0 => theme::YELLOW,
        _ => theme::MUTED,
    };
    let first = Line::from(vec![
        Span::styled(
            format!("{:>3.0}", job.score),
            Style::new().fg(score_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            terminal_text(&job.title),
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
    ]);
    let second = Line::from(vec![
        Span::raw("     "),
        Span::styled(terminal_text(&job.company), Style::new().fg(theme::MUTED)),
        Span::styled("  ·  ", Style::new().fg(theme::FAINT)),
        Span::styled(job.work_mode.label(), Style::new().fg(theme::MUTED)),
        Span::styled("  ·  ", Style::new().fg(theme::FAINT)),
        status_badge(job.status),
    ]);
    ListItem::new(vec![first, second])
}

fn render_job_detail(frame: &mut Frame<'_>, job: Option<&Job>, area: Rect) {
    let Some(job) = job else {
        frame.render_widget(
            Paragraph::new("Select a job to see its details")
                .alignment(Alignment::Center)
                .block(panel(" JOB DETAILS ".into(), false))
                .style(theme::surface()),
            area,
        );
        return;
    };

    let outer = panel(" JOB DETAILS ".into(), false);
    let inner = outer.inner(area);
    frame.render_widget(outer.style(theme::surface()), area);
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .margin(1)
    .split(inner);

    let salary = terminal_text(&job.salary_label());
    let header = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            terminal_text(&job.title),
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            terminal_text(&job.company),
            Style::new().fg(theme::ACCENT),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(terminal_text(&job.location), Style::new().fg(theme::MUTED)),
            Span::styled("  •  ", Style::new().fg(theme::FAINT)),
            Span::styled(salary, Style::new().fg(theme::GREEN)),
        ]),
    ]));
    frame.render_widget(header, rows[0]);

    let gauge = Gauge::default()
        .block(Block::default().title(Span::styled(" MATCH ", theme::accent())))
        .gauge_style(Style::new().fg(theme::ACCENT).bg(theme::SURFACE_ALT))
        .label(Span::styled(
            format!("{:.0} / 100", job.score),
            theme::heading(),
        ))
        .ratio((job.score / 100.0).clamp(0.0, 1.0));
    frame.render_widget(gauge, rows[1]);

    let metadata = Paragraph::new(Line::from(vec![
        status_badge(job.status),
        Span::raw("   "),
        Span::styled(
            terminal_text(&job.verification),
            verification_style(&job.verification),
        ),
        Span::styled(" via ", Style::new().fg(theme::FAINT)),
        Span::styled(terminal_text(&job.source), Style::new().fg(theme::MUTED)),
        Span::styled("   ·   ", Style::new().fg(theme::FAINT)),
        Span::styled(job.short_id().to_string(), Style::new().fg(theme::FAINT)),
    ]));
    frame.render_widget(metadata, rows[2]);

    let reasons: String = if job.reasons.is_empty() {
        "No ranking reasons recorded".into()
    } else {
        job.reasons
            .iter()
            .map(|reason| format!("+ {}", terminal_text(reason)))
            .collect::<Vec<_>>()
            .join("   ")
    };
    let concerns: String = if job.concerns.is_empty() {
        "No notable concerns".into()
    } else {
        job.concerns
            .iter()
            .map(|concern| format!("⚠ {}", terminal_text(concern)))
            .collect::<Vec<_>>()
            .join("   ")
    };
    let mut lines = vec![
        Line::from(Span::styled("WHY IT RANKED", theme::accent())),
        Line::from(Span::styled(reasons, Style::new().fg(theme::CYAN))),
        Line::from(""),
        Line::from(Span::styled("WATCH", theme::accent())),
        Line::from(Span::styled(concerns, Style::new().fg(theme::RED))),
        Line::from(""),
        Line::from(Span::styled("ABOUT THE ROLE", theme::accent())),
        Line::from(terminal_text(&job.description)),
    ];
    if !job.notes.trim().is_empty() {
        lines.extend([
            Line::from(""),
            Line::from(Span::styled("NOTES", theme::accent())),
            Line::from(terminal_text(&job.notes)),
        ]);
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled("LISTING", theme::accent())),
        Line::from(Span::styled(
            terminal_text(job.preferred_url()),
            Style::new().fg(theme::MUTED),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(theme::surface())
            .wrap(Wrap { trim: true }),
        rows[3],
    );

    let actions = Paragraph::new(Line::from(vec![
        keycap("Enter"),
        Span::styled(" open   ", theme::muted()),
        keycap("N"),
        Span::styled(" note   ", theme::muted()),
        keycap("E"),
        Span::styled(" history   ", theme::muted()),
        keycap("A"),
        Span::styled(" applied   ", theme::muted()),
        keycap("I"),
        Span::styled(" interview", theme::muted()),
    ]));
    frame.render_widget(actions, rows[4]);
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Min(48), Constraint::Length(42)])
        .margin(1)
        .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            keycap("↑↓"),
            Span::styled(" move   ", theme::muted()),
            keycap("←→"),
            Span::styled(" tabs   ", theme::muted()),
            keycap("/"),
            Span::styled(" search   ", theme::muted()),
            keycap("U"),
            Span::styled(" reload   ", theme::muted()),
            keycap("?"),
            Span::styled(" help", theme::muted()),
        ])),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(terminal_text(
            app.notice
                .as_deref()
                .unwrap_or("Local-first · SQLite-backed · no account required"),
        ))
        .style(Style::new().fg(theme::FAINT))
        .alignment(Alignment::Right),
        chunks[1],
    );
}

fn render_text_input(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &'static str,
    prefix: &'static str,
    value: &str,
    placeholder: &'static str,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .style(Style::new().bg(theme::SURFACE))
        .title(Span::styled(title, theme::accent()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let line = if value.is_empty() {
        Line::from(Span::styled(placeholder, theme::muted()))
    } else {
        Line::from(vec![
            Span::styled(prefix, Style::new().fg(theme::ACCENT)),
            Span::styled(
                terminal_text(value),
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▌", Style::new().fg(theme::ACCENT)),
        ])
    };
    frame.render_widget(Paragraph::new(line).style(theme::surface()), inner);
}

fn render_history(frame: &mut Frame<'_>, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let block = modal_block(" HISTORY ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![
        Line::from(Span::styled(
            app.selected_job()
                .map(|job| {
                    format!(
                        "{} · {}",
                        terminal_text(&job.title),
                        terminal_text(&job.company)
                    )
                })
                .unwrap_or_else(|| "Job history".into()),
            theme::heading(),
        )),
        Line::from(""),
    ];
    if app.history.is_empty() {
        lines.push(Line::from(Span::styled(
            "No history events recorded yet.",
            theme::muted(),
        )));
    } else {
        for event in &app.history {
            lines.extend(history_lines(event));
        }
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled("E / Esc  close", theme::muted())),
    ]);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true }),
        inner,
    );
}

fn history_lines(event: &JobEvent) -> Vec<Line<'static>> {
    let transition = match (&event.old_value, &event.new_value) {
        (Some(old), Some(new)) => {
            format!("{} → {}", terminal_text(old), terminal_text(new))
        }
        (_, Some(new)) => terminal_text(new),
        _ => String::new(),
    };
    let mut line = vec![
        Span::styled(
            terminal_text(&event.created_at),
            Style::new().fg(theme::FAINT),
        ),
        Span::raw("  "),
        Span::styled(
            terminal_text(&event.event_type.to_uppercase()),
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ),
    ];
    if !transition.is_empty() {
        line.push(Span::raw("  "));
        line.push(Span::styled(transition, Style::new().fg(theme::TEXT)));
    }
    let mut lines = vec![Line::from(line)];
    if let Some(note) = event.note.as_deref().filter(|note| !note.is_empty()) {
        lines.push(Line::from(Span::styled(
            format!("    {}", terminal_text(note)),
            theme::muted(),
        )));
    }
    lines.push(Line::from(""));
    lines
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(Clear, area);
    let block = modal_block(" SHORTCUTS ");
    let help = Paragraph::new(Text::from(vec![
        help_line("↑ / K", "Previous job"),
        help_line("↓ / J", "Next job"),
        help_line("← / H", "Previous tab"),
        help_line("→ / L", "Next tab"),
        Line::from(""),
        help_line("/", "Search instantly"),
        help_line("Enter / O", "Open employer listing"),
        help_line("N", "Add a note"),
        help_line("E", "View durable job history"),
        help_line("U", "Reload tracker from SQLite"),
        Line::from(""),
        help_line("R", "Mark reviewed"),
        help_line("A", "Mark applied"),
        help_line("I", "Mark interview"),
        help_line("X", "Mark rejected"),
        help_line("Shift+O", "Mark offer"),
        help_line("C", "Mark closed"),
        Line::from(""),
        help_line("Q", "Quit"),
        help_line("? / Esc", "Close this overlay"),
    ]))
    .block(block)
    .style(theme::surface())
    .wrap(Wrap { trim: true });
    frame.render_widget(help, area);
}

fn help_line(key: &'static str, label: &'static str) -> Line<'static> {
    Line::from(vec![
        keycap(key),
        Span::styled(format!("  {label}"), theme::muted()),
    ])
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

fn modal_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .style(Style::new().bg(theme::SURFACE))
        .title(Span::styled(title, theme::accent()))
}

fn status_badge(status: ApplicationStatus) -> Span<'static> {
    Span::styled(
        format!(" {} ", status.label()),
        Style::new()
            .fg(status.color())
            .bg(theme::SURFACE_ALT)
            .add_modifier(Modifier::BOLD),
    )
}

fn verification_style(value: &str) -> Style {
    match value.to_ascii_lowercase().as_str() {
        "verified" | "reachable" => Style::new().fg(theme::GREEN),
        "closed" | "failed" => Style::new().fg(theme::RED),
        _ => Style::new().fg(theme::YELLOW),
    }
}

fn keycap(value: &str) -> Span<'static> {
    Span::styled(
        format!(" {value} "),
        Style::new()
            .fg(theme::TEXT)
            .bg(theme::SURFACE_ALT)
            .add_modifier(Modifier::BOLD),
    )
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let width = area
        .width
        .saturating_mul(percent_x)
        .saturating_div(100)
        .max(24)
        .min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn full_layout_renders_without_error() {
        let backend = TestBackend::new(140, 44);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn compact_layout_renders_without_error() {
        let backend = TestBackend::new(78, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn note_overlay_renders_without_error() {
        let backend = TestBackend::new(110, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            input_mode: InputMode::Note,
            note_buffer: "Strong hiring manager call".into(),
            ..Default::default()
        };
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }
}
