pub mod depth;
pub mod market_list;
pub mod orderbook;
pub mod pnl_panel;
pub mod timeline;
pub mod trades;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, AppMode};

pub fn render(f: &mut Frame, app: &mut App) {
    match app.mode {
        AppMode::MarketBrowser => render_browser(f, app),
        AppMode::Replay => render_replay(f, app),
    }
}

fn render_browser(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    market_list::render(f, app, chunks[0]);

    // Status bar
    let help = Line::from(vec![
        Span::styled(" j/k", Style::default().fg(Color::Cyan)),
        Span::raw(":nav "),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(":select "),
        Span::styled("1-7", Style::default().fg(Color::Cyan)),
        Span::raw(":crypto "),
        Span::styled("a", Style::default().fg(Color::Cyan)),
        Span::raw(":all "),
        Span::styled("[/]", Style::default().fg(Color::Cyan)),
        Span::raw(":date "),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(":quit"),
        if !app.status_message.is_empty() {
            Span::styled(
                format!("  {}", app.status_message),
                Style::default().fg(Color::Yellow),
            )
        } else {
            Span::raw("")
        },
    ]);

    let status = Paragraph::new(help).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(status, chunks[1]);
}

fn render_replay(f: &mut Frame, app: &mut App) {
    let snapshots = app.current_snapshots();
    let cursor = app.replay.cursor.min(snapshots.len().saturating_sub(1));
    let current_snapshot = snapshots.get(cursor).copied();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(40),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Top row: market list | depth | orderbook
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ])
        .split(main_chunks[0]);

    // Show market info in left panel during replay
    let market_info = app
        .market_data
        .as_ref()
        .map(|d| {
            let title = format!(
                " {} - {} [{}] ",
                d.market.crypto.to_uppercase(),
                d.market.question,
                app.show_outcome
            );
            Paragraph::new(format!(
                "\n  Market: {}\n  Crypto: {}\n  Outcome: {}\n  Snapshots: {}",
                d.market.question,
                d.market.crypto.to_uppercase(),
                app.show_outcome,
                snapshots.len()
            ))
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
        })
        .unwrap_or_else(|| Paragraph::new("No market"));

    f.render_widget(market_info, top_chunks[0]);
    depth::render(f, current_snapshot, top_chunks[1]);
    orderbook::render(f, current_snapshot, top_chunks[2]);

    // Bottom row: trades | pnl
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[1]);

    let visible_trades = app.market_data.as_ref().map_or(0, |d| {
        app.replay.visible_trade_count(
            &snapshots.iter().copied().cloned().collect::<Vec<_>>(),
            &d.user_trades,
        )
    });

    if let Some(data) = &app.market_data {
        trades::render(f, &data.user_trades, visible_trades, bottom_chunks[0]);
        pnl_panel::render(
            f,
            &app.pnl,
            current_snapshot,
            data.resolution.as_ref(),
            bottom_chunks[1],
        );
    }

    // Timeline
    timeline::render(f, &app.replay, &snapshots, main_chunks[2]);
}
