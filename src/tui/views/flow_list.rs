use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // Title bar
        Constraint::Min(10),   // Flow list
        Constraint::Length(3), // Help bar
    ])
    .split(frame.area());

    render_title(frame, chunks[0]);
    render_flows(frame, chunks[1], app);
    render_help(frame, chunks[2]);
}

fn render_title(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "Cthulu",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("TUI", Style::default().fg(Color::DarkGray)),
        Span::raw(" — Select a flow to start a session"),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(title, area);
}

fn render_flows(frame: &mut Frame, area: Rect, app: &App) {
    if app.flow_list_loading {
        let loading = Paragraph::new("Loading flows...")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Flows ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(loading, area);
        return;
    }

    if let Some(err) = &app.flow_list_error {
        let error = Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Flows ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            );
        frame.render_widget(error, area);
        return;
    }

    if app.flows.is_empty() {
        let empty = Paragraph::new("No flows found. Create flows via the API or Studio.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Flows ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .flows
        .iter()
        .enumerate()
        .map(|(i, flow)| {
            let status = if flow.enabled {
                Span::styled("●", Style::default().fg(Color::Green))
            } else {
                Span::styled("○", Style::default().fg(Color::DarkGray))
            };

            let name_style = if i == app.flow_list_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let line = Line::from(vec![
                Span::raw("  "),
                status,
                Span::raw(" "),
                Span::styled(&flow.name, name_style),
                Span::raw("  "),
                Span::styled(
                    format!("({} nodes)", flow.node_count),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(&flow.description, Style::default().fg(Color::DarkGray)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" Flows ({}) ", app.flows.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    // We need a ListState for highlighting
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.flow_list_index));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ ", Style::default().fg(Color::Cyan)),
        Span::raw("Navigate  "),
        Span::styled(" Enter ", Style::default().fg(Color::Cyan)),
        Span::raw("Select  "),
        Span::styled(" r ", Style::default().fg(Color::Cyan)),
        Span::raw("Refresh  "),
        Span::styled(" q ", Style::default().fg(Color::Cyan)),
        Span::raw("Quit"),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(help, area);
}
