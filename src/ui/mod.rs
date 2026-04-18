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
    let up_snap = app.current_up_snapshot();
    let down_snap = app.current_down_snapshot();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(40),
            Constraint::Length(4),
        ])
        .split(f.area());

    // Top row: market info | up depth | up orderbook | down depth | down orderbook
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(21),
            Constraint::Percentage(22),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
        ])
        .split(main_chunks[0]);

    // Market info panel
    let market_info = app
        .market_data
        .as_ref()
        .map(|d| {
            let title = format!(
                " {} - {} ",
                d.market.crypto.to_uppercase(),
                d.market.question,
            );
            let play_status = if app.replay.playing { "Playing" } else { "Paused" };
            Paragraph::new(format!(
                "\n  {}\n  {}\n\n  Up:   {}\n  Down: {}\n  Trades: {}\n\n  {} | {}",
                d.market.question,
                d.market.crypto.to_uppercase(),
                d.up_snapshots.len(),
                d.down_snapshots.len(),
                d.user_trades.len(),
                play_status,
                app.replay.speed.label(),
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
    depth::render(f, up_snap, "Up", top_chunks[1]);
    orderbook::render(f, up_snap, "Up", top_chunks[2]);
    depth::render(f, down_snap, "Down", top_chunks[3]);
    orderbook::render(f, down_snap, "Down", top_chunks[4]);

    // Bottom row: trades | pnl
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[1]);

    let current_ts = up_snap.map(|s| s.timestamp).unwrap_or(chrono::NaiveDateTime::MIN);

    if let Some(data) = &app.market_data {
        let all_visible = data
            .all_trades
            .iter()
            .take_while(|t| t.timestamp <= current_ts)
            .count();
        let user_visible = app
            .replay
            .visible_trade_count(&data.up_snapshots, &data.user_trades);
        let visible_count = if app.show_all_trades {
            all_visible
        } else {
            user_visible
        };

        trades::render(
            f,
            &data.all_trades,
            &data.user_trades,
            app.show_all_trades,
            visible_count,
            bottom_chunks[0],
        );
        pnl_panel::render(
            f,
            &app.pnl,
            up_snap,
            down_snap,
            data.resolution.as_ref(),
            bottom_chunks[1],
        );
    }

    // Timeline driven by up_snapshots
    if let Some(data) = &app.market_data {
        timeline::render(
            f,
            &app.replay,
            &data.up_snapshots,
            &data.user_trades,
            main_chunks[2],
        );
    }
}
