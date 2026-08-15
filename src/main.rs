mod app;
mod model;
mod theme;
mod ui;

use std::{io, time::Duration};

use anyhow::Result;
use app::App;
use clap::{Parser, Subcommand};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};

#[derive(Debug, Parser)]
#[command(
    name = "jobscout",
    version,
    about = "Fast, local-first job discovery and application tracking",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open the interactive terminal application.
    Ui,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Ui) {
        Command::Ui => run_ui(),
    }
}

fn run_ui() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_event_loop(&mut terminal);
    let cleanup = restore_terminal(&mut terminal);

    result?;
    cleanup?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::default();

    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                app.handle_key(key);
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => app.handle_key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::NONE,
                )),
                MouseEventKind::ScrollUp => app.handle_key(KeyEvent::new(
                    KeyCode::Up,
                    KeyModifiers::NONE,
                )),
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cli_command_is_ui() {
        let cli = Cli::try_parse_from(["jobscout"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn explicit_ui_command_parses() {
        let cli = Cli::try_parse_from(["jobscout", "ui"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Ui)));
    }
}
