use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::model::OrderbookSnapshot;
use crate::replay::ReplayState;

pub fn render(
    f: &mut Frame,
    replay: &ReplayState,
    snapshots: &[&OrderbookSnapshot],
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if snapshots.is_empty() {
        return;
    }

    let total = snapshots.len();
    let cursor = replay.cursor.min(total.saturating_sub(1));

    // Time display
    let current_time = snapshots[cursor].timestamp.format("%H:%M:%S").to_string();
    let start_time = snapshots[0].timestamp.format("%H:%M:%S").to_string();
    let end_time = snapshots[total - 1]
        .timestamp
        .format("%H:%M:%S")
        .to_string();

    // Progress bar
    let bar_width = (inner.width as usize).saturating_sub(40);
    let progress = if total > 1 {
        cursor as f64 / (total - 1) as f64
    } else {
        0.0
    };
    let filled = (progress * bar_width as f64) as usize;
    let empty = bar_width.saturating_sub(filled);
    let bar = format!("[{}{}]", "=".repeat(filled), " ".repeat(empty));

    let play_icon = if replay.playing { "\u{25b6}" } else { "\u{23f8}" };

    let line = Line::from(vec![
        Span::styled(
            format!(" {play_icon} "),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(&start_time, Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        Span::styled(bar, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled(&end_time, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(
            current_time,
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Speed: {}", replay.speed.label()),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{}/{}", cursor + 1, total),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let p = Paragraph::new(line);
    f.render_widget(p, inner);
}
