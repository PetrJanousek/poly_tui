use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::model::{OrderbookSnapshot, UserTrade};
use crate::replay::ReplayState;

pub fn render(
    f: &mut Frame,
    replay: &ReplayState,
    snapshots: &[&OrderbookSnapshot],
    trades: &[UserTrade],
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

    let current_time = snapshots[cursor].timestamp.format("%H:%M:%S").to_string();
    let start_time = snapshots[0].timestamp.format("%H:%M:%S").to_string();
    let end_time = snapshots[total - 1]
        .timestamp
        .format("%H:%M:%S")
        .to_string();

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

    // Trade markers row (rendered above the progress bar).
    // Position each trade on the same column scale as the progress bar: the
    // prefix " ▶ HH:MM:SS [" is 13 chars wide, then bar_width columns of bar
    // content, then "]".
    let marker_line = if bar_width > 0 {
        let start_ts = snapshots[0].timestamp.and_utc().timestamp();
        let end_ts = snapshots[total - 1].timestamp.and_utc().timestamp();
        let span_ts = (end_ts - start_ts).max(1) as f64;

        let mut cells: Vec<Option<&str>> = vec![None; bar_width];
        for t in trades {
            let ts = t.timestamp.and_utc().timestamp();
            if ts < start_ts || ts > end_ts {
                continue;
            }
            let pos =
                ((ts - start_ts) as f64 / span_ts * (bar_width as f64 - 1.0)).round() as usize;
            if pos >= bar_width {
                continue;
            }
            let kind = t.outcome.as_str();
            cells[pos] = match cells[pos] {
                None => Some(kind),
                Some(existing) if existing != kind => Some("Both"),
                other => other,
            };
        }

        let mut spans = Vec::with_capacity(bar_width + 2);
        spans.push(Span::raw(" ".repeat(13))); // align with first bar cell
        for cell in &cells {
            match cell {
                None => spans.push(Span::raw(" ")),
                Some("Up") => spans.push(Span::styled(
                    "|",
                    Style::default().fg(Color::Green),
                )),
                Some("Down") => spans.push(Span::styled(
                    "|",
                    Style::default().fg(Color::Red),
                )),
                Some(_) => spans.push(Span::styled(
                    "|",
                    Style::default().fg(Color::Yellow),
                )),
            }
        }
        Line::from(spans)
    } else {
        Line::raw("")
    };

    let progress_line = Line::from(vec![
        Span::styled(format!(" {play_icon} "), Style::default().fg(Color::Cyan)),
        Span::styled(&start_time, Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        Span::styled(bar, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled(&end_time, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(current_time, Style::default().fg(Color::White)),
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

    let p = Paragraph::new(vec![marker_line, progress_line]);
    f.render_widget(p, inner);
}
