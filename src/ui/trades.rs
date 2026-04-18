use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Row, Table},
};

use crate::model::{Trade, UserTrade};

pub fn render(
    f: &mut Frame,
    all_trades: &[Trade],
    user_trades: &[UserTrade],
    show_all: bool,
    visible_count: usize,
    area: Rect,
) {
    let title = if show_all {
        " All Trades [t] ".to_string()
    } else {
        format!(" My Trades [t] ({visible_count}) ")
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    if show_all {
        let header = Row::new(vec!["Time", "Side", "Out", "*", "R", "Price", "Size"])
            .style(Style::default().fg(Color::DarkGray));

        let visible = &all_trades[..visible_count.min(all_trades.len())];
        let rows: Vec<Row> = visible
            .iter()
            .rev()
            .map(|t| {
                let color = if t.is_user {
                    if t.side == "BUY" && t.outcome == "Up" {
                        Color::Green
                    } else if t.side == "BUY" {
                        Color::Red
                    } else if t.outcome == "Up" {
                        Color::LightGreen
                    } else {
                        Color::LightRed
                    }
                } else {
                    Color::DarkGray
                };
                let marker = if t.is_user { "*" } else { " " };
                let role = t
                    .is_taker
                    .map(|taker| if taker { "T" } else { "M" })
                    .unwrap_or("-");
                Row::new(vec![
                    t.timestamp.format("%H:%M:%S%.3f").to_string(),
                    t.side.clone(),
                    t.outcome.clone(),
                    marker.to_string(),
                    role.to_string(),
                    format!("{:.4}", t.price),
                    format!("{:.2}", t.size),
                ])
                .style(Style::default().fg(color))
            })
            .collect();

        let widths = [
            Constraint::Length(12), // time with ms
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(2), // *
            Constraint::Length(2), // R
            Constraint::Length(8),
            Constraint::Length(10),
        ];

        let table = Table::new(rows, widths).header(header).block(block);
        f.render_widget(table, area);
    } else {
        let header = Row::new(vec!["Time", "Side", "Outcome", "Role", "Price", "Size"])
            .style(Style::default().fg(Color::DarkGray));

        let rows: Vec<Row> = user_trades
            .iter()
            .take(visible_count)
            .rev()
            .map(|t| {
                let is_buy = t.side.eq_ignore_ascii_case("buy");
                let is_down = t.outcome.eq_ignore_ascii_case("down");
                let color = if is_buy && !is_down {
                    Color::Green
                } else {
                    Color::Red
                };
                let role = if t.is_taker { "T" } else { "M" };
                Row::new(vec![
                    t.timestamp.format("%H:%M:%S%.3f").to_string(),
                    t.side.clone(),
                    t.outcome.clone(),
                    role.to_string(),
                    format!("{:.4}", t.price),
                    format!("{:.2}", t.size),
                ])
                .style(Style::default().fg(color))
            })
            .collect();

        let widths = [
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(10),
        ];

        let table = Table::new(rows, widths).header(header).block(block);
        f.render_widget(table, area);
    }
}
