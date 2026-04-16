use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Row, Table},
};

use crate::model::UserTrade;

pub fn render(f: &mut Frame, trades: &[UserTrade], visible_count: usize, area: Rect) {
    let block = Block::default()
        .title(format!(" Trades ({visible_count}) "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let header = Row::new(vec!["Time", "Side", "Outcome", "Price", "Size"])
        .style(Style::default().fg(Color::DarkGray));

    let rows: Vec<Row> = trades
        .iter()
        .take(visible_count)
        .rev() // newest first
        .map(|t| {
            let is_buy = t.side.eq_ignore_ascii_case("buy");
            let is_down = t.outcome.eq_ignore_ascii_case("down");
            let color = if is_buy && !is_down {
                Color::Green
            } else {
                Color::Red
            };
            Row::new(vec![
                t.timestamp.format("%H:%M:%S").to_string(),
                t.side.clone(),
                t.outcome.clone(),
                format!("{:.4}", t.price),
                format!("{:.2}", t.size),
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(6),
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(block);

    f.render_widget(table, area);
}
