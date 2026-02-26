pub mod app;
pub mod claude_stream;
pub mod views;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use std::io;
use std::time::Duration;

use self::app::{App, Screen};

/// Main entry point for the TUI.
pub async fn run(server_url: String, flow_id: Option<String>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(server_url);

    // Load initial data
    app.load_flows().await;

    // If a flow ID was provided, jump straight to session
    if let Some(id) = flow_id {
        app.select_flow_by_id(&id).await;
    }

    // Main event loop
    let result = run_loop(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        // Draw
        terminal.draw(|frame| {
            match &app.screen {
                Screen::FlowList => views::flow_list::render(frame, app),
                Screen::Session => views::session::render(frame, app),
            }
        })?;

        // Poll for events with a timeout so we can also check for async updates
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                // Global quit: Ctrl+C
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
                {
                    return Ok(());
                }

                match &app.screen {
                    Screen::FlowList => {
                        handle_flow_list_keys(app, key).await;
                    }
                    Screen::Session => {
                        if handle_session_keys(app, key).await {
                            // true means quit was requested
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Check for Claude stream updates
        app.poll_claude_events().await;

        if app.should_quit {
            return Ok(());
        }
    }
}

async fn handle_flow_list_keys(app: &mut App, key: event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.flow_list_up(),
        KeyCode::Down | KeyCode::Char('j') => app.flow_list_down(),
        KeyCode::Enter => app.enter_session().await,
        KeyCode::Char('r') => app.load_flows().await,
        _ => {}
    }
}

async fn handle_session_keys(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.leave_session();
            false
        }
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            // Alt+Enter to send prompt
            app.send_prompt().await;
            false
        }
        KeyCode::Enter if !app.claude_running => {
            // Enter sends when Claude is not running
            // But if Shift is held or input is multi-line-focused, add newline
            if app.input_cursor_line() > 0 || key.modifiers.contains(KeyModifiers::SHIFT) {
                app.input_newline();
            } else {
                app.send_prompt().await;
            }
            false
        }
        KeyCode::Enter => {
            app.input_newline();
            false
        }
        KeyCode::Char(c) => {
            app.input_char(c);
            false
        }
        KeyCode::Backspace => {
            app.input_backspace();
            false
        }
        KeyCode::Left => {
            app.input_left();
            false
        }
        KeyCode::Right => {
            app.input_right();
            false
        }
        KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.scroll_output_up();
            false
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.scroll_output_down();
            false
        }
        KeyCode::PageUp => {
            app.scroll_output_up();
            false
        }
        KeyCode::PageDown => {
            app.scroll_output_down();
            false
        }
        KeyCode::Tab => {
            // Toggle focus between input and output pane
            app.toggle_focus();
            false
        }
        _ => false,
    }
}
