use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::tui::app::{App, Focus, OutputKind};

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // Header
        Constraint::Min(10),   // Output pane
        Constraint::Length(8), // Input box
        Constraint::Length(2), // Status bar
    ])
    .split(frame.area());

    render_header(frame, chunks[0], app);
    render_output(frame, chunks[1], app);
    render_input(frame, chunks[2], app);
    render_status(frame, chunks[3], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let flow_name = app
        .session
        .as_ref()
        .map(|s| s.flow_name.as_str())
        .unwrap_or("...");

    let sources = app
        .session
        .as_ref()
        .map(|s| s.sources_summary.as_str())
        .unwrap_or("");

    let sinks = app
        .session
        .as_ref()
        .map(|s| s.sinks_summary.as_str())
        .unwrap_or("");

    let header = Paragraph::new(Line::from(vec![
        Span::styled("Flow: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            flow_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  │  "),
        Span::styled(sources, Style::default().fg(Color::Blue)),
        Span::raw("  │  "),
        Span::styled(sinks, Style::default().fg(Color::Green)),
    ]))
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(header, area);
}

fn render_output(frame: &mut Frame, area: Rect, app: &App) {
    let output_border_color = if app.focus == Focus::Output {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let running_indicator = if app.claude_running {
        " [Running...] "
    } else {
        ""
    };

    let block = Block::default()
        .title(format!(" Output{running_indicator}"))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(output_border_color));

    let inner_area = block.inner(area);

    let lines: Vec<Line> = app
        .output_lines
        .iter()
        .map(|line| {
            let style = match line.kind {
                OutputKind::System => Style::default().fg(Color::DarkGray),
                OutputKind::Text => Style::default().fg(Color::White),
                OutputKind::ToolUse => Style::default().fg(Color::Yellow),
                OutputKind::ToolResult => Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
                OutputKind::Result => Style::default().fg(Color::Green),
                OutputKind::Error => Style::default().fg(Color::Red),
                OutputKind::Cost => Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            };

            // Prefix with kind indicator
            let prefix = match line.kind {
                OutputKind::System => "  ",
                OutputKind::Text => "  ",
                OutputKind::ToolUse => "⚙ ",
                OutputKind::ToolResult => "  ",
                OutputKind::Result => "✓ ",
                OutputKind::Error => "✗ ",
                OutputKind::Cost => "$ ",
            };

            Line::from(Span::styled(format!("{prefix}{}", line.text), style))
        })
        .collect();

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.output_scroll as u16, 0));

    frame.render_widget(paragraph, area);

    // Scrollbar
    if app.output_lines.len() > inner_area.height as usize {
        let mut scrollbar_state =
            ScrollbarState::new(app.output_lines.len()).position(app.output_scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));
        frame.render_stateful_widget(scrollbar, inner_area, &mut scrollbar_state);
    }
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let input_border_color = if app.focus == Focus::Input {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if app.claude_running {
        " Input (waiting for response...) "
    } else {
        " Input (Enter to send, Esc to go back) "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(input_border_color));

    // Show the input text, or a placeholder
    let display_text = if app.input.is_empty() && !app.claude_running {
        Text::styled(
            "Type your prompt here...",
            Style::default().fg(Color::DarkGray),
        )
    } else {
        // Show the last N lines that fit in the input area
        let inner_height = area.height.saturating_sub(2) as usize;
        let lines: Vec<&str> = app.input.lines().collect();
        let visible_lines = if lines.len() > inner_height && inner_height > 0 {
            &lines[lines.len() - inner_height..]
        } else {
            &lines
        };

        let text_lines: Vec<Line> = visible_lines
            .iter()
            .map(|l| Line::from(Span::styled(*l, Style::default().fg(Color::White))))
            .collect();

        Text::from(text_lines)
    };

    let paragraph = Paragraph::new(display_text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);

    // Show cursor position in the input area
    if app.focus == Focus::Input && !app.claude_running {
        // Calculate cursor position
        let input_inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

        let text_before_cursor = &app.input[..app.input_cursor];
        let cursor_line = text_before_cursor.matches('\n').count();
        let last_newline = text_before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let cursor_col = app.input[last_newline..app.input_cursor].chars().count();

        let inner_height = input_inner.height as usize;
        let total_lines = app.input.lines().count().max(1);
        let scroll_offset = if total_lines > inner_height {
            total_lines - inner_height
        } else {
            0
        };

        let visible_line = cursor_line.saturating_sub(scroll_offset);

        if visible_line < inner_height {
            frame.set_cursor_position((
                input_inner.x + cursor_col as u16,
                input_inner.y + visible_line as u16,
            ));
        }
    }
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let status_items = if app.claude_running {
        vec![
            Span::styled(" ⟳ ", Style::default().fg(Color::Yellow)),
            Span::raw("Claude is working...  "),
            Span::styled(" Shift+↑↓ ", Style::default().fg(Color::Cyan)),
            Span::raw("Scroll  "),
            Span::styled(" Tab ", Style::default().fg(Color::Cyan)),
            Span::raw("Focus  "),
            Span::styled(" Ctrl+Q ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ]
    } else {
        vec![
            Span::styled(" Enter ", Style::default().fg(Color::Cyan)),
            Span::raw("Send  "),
            Span::styled(" Shift+↑↓ ", Style::default().fg(Color::Cyan)),
            Span::raw("Scroll  "),
            Span::styled(" Tab ", Style::default().fg(Color::Cyan)),
            Span::raw("Focus  "),
            Span::styled(" Esc ", Style::default().fg(Color::Cyan)),
            Span::raw("Back  "),
            Span::styled(" Ctrl+C ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ]
    };

    let status = Paragraph::new(Line::from(status_items))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(status, area);
}
