use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::App;

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let filter_text = app
        .crypto_filter
        .as_deref()
        .unwrap_or("all")
        .to_uppercase();

    let title = format!(
        " Markets [{filter_text}] {} ",
        app.date_from.format("%Y-%m-%d")
    );

    let items: Vec<ListItem> = app
        .markets
        .iter()
        .map(|m| {
            let crypto = m.crypto.to_uppercase();
            let time = m
                .end_date
                .map(|d| d.format("%H:%M").to_string())
                .unwrap_or_default();
            let line = Line::from(vec![
                Span::styled(
                    format!("{crypto:>4} "),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(time, Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(&m.question, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, area, &mut app.market_list_state);
}
