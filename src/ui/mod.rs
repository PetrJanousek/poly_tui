pub mod depth;
pub mod market_list;
pub mod orderbook;
pub mod pnl_panel;
pub mod price_chart;
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
    let current_ts = up_snap.map(|s| s.timestamp).unwrap_or(chrono::NaiveDateTime::MIN);

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(40),
            Constraint::Length(4),
        ])
        .split(f.area());

    // Top row: [market info | up orderbook | up depth / down depth | down orderbook]
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(13), // market info
            Constraint::Fill(1),        // up orderbook
            Constraint::Percentage(14), // depth stack
            Constraint::Fill(1),        // down orderbook
        ])
        .split(main_chunks[0]);

    // Depth column: up depth on top, down depth below
    let depth_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(top_chunks[2]);

    // Market info panel
    if let Some(data) = &app.market_data {
        let play_status = if app.replay.playing { "▶" } else { "⏸" };
        let speed = app.replay.speed.label();
        let question = &data.market.question;
        let q_short = if question.len() > 16 {
            format!("{}…", &question[..16])
        } else {
            question.clone()
        };
        let info_text = format!(
            "{} {} {}\n{}\nUp:{} Dn:{}\nTrades:{}",
            data.market.crypto.to_uppercase(),
            play_status,
            speed,
            q_short,
            data.up_snapshots.len(),
            data.down_snapshots.len(),
            data.user_trades.len(),
        );
        f.render_widget(
            Paragraph::new(info_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            top_chunks[0],
        );
    }

    orderbook::render(f, up_snap, "Up", top_chunks[1]);
    depth::render(f, up_snap, "Up", depth_chunks[0]);
    depth::render(f, down_snap, "Down", depth_chunks[1]);
    orderbook::render(f, down_snap, "Down", top_chunks[3]);

    // Bottom row: [price chart | trades | pnl]
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // price chart
            Constraint::Fill(1),        // trades
            Constraint::Percentage(22), // pnl
        ])
        .split(main_chunks[1]);

    if let Some(data) = &app.market_data {
        price_chart::render(f, data, current_ts, bottom_chunks[0]);

        let all_visible = data
            .all_trades
            .iter()
            .take_while(|t| t.timestamp <= current_ts)
            .count();
        let user_visible = app
            .replay
            .visible_trade_count(&data.up_snapshots, &data.user_trades);
        let visible_count = if app.show_all_trades { all_visible } else { user_visible };

        trades::render(
            f,
            &data.all_trades,
            &data.user_trades,
            app.show_all_trades,
            visible_count,
            bottom_chunks[1],
        );
        pnl_panel::render(
            f,
            &app.pnl,
            up_snap,
            down_snap,
            data.resolution.as_ref(),
            bottom_chunks[2],
        );
    }

    // Timeline
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
