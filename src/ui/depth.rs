use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::model::OrderbookSnapshot;

pub fn render(f: &mut Frame, snapshot: Option<&OrderbookSnapshot>, area: Rect) {
    let block = Block::default()
        .title(" Depth ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let Some(snap) = snapshot else {
        let p = Paragraph::new("No data").block(block);
        f.render_widget(p, area);
        return;
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Compute cumulative sizes
    let mut bid_cumul = Vec::new();
    let mut sum = 0.0;
    for &s in &snap.bid_sizes {
        sum += s;
        bid_cumul.push(sum);
    }

    let mut ask_cumul = Vec::new();
    sum = 0.0;
    for &s in &snap.ask_sizes {
        sum += s;
        ask_cumul.push(sum);
    }

    let max_cumul = bid_cumul
        .iter()
        .chain(ask_cumul.iter())
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1.0);

    let half_width = inner.width as usize / 2;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled(
            format!("{:>w$}", "BIDS", w = half_width),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            format!("{:<w$}", "ASKS", w = half_width),
            Style::default().fg(Color::Red),
        ),
    ]));

    let max_levels = bid_cumul.len().max(ask_cumul.len());

    for i in 0..max_levels {
        let bid_bar_len = bid_cumul
            .get(i)
            .map(|&c| ((c / max_cumul) * half_width as f64) as usize)
            .unwrap_or(0);
        let ask_bar_len = ask_cumul
            .get(i)
            .map(|&c| ((c / max_cumul) * half_width as f64) as usize)
            .unwrap_or(0);

        // Bids grow from right to left
        let bid_padding = half_width.saturating_sub(bid_bar_len);
        let bid_bar = format!(
            "{}{}",
            " ".repeat(bid_padding),
            "\u{2588}".repeat(bid_bar_len)
        );

        // Asks grow from left to right
        let ask_bar = format!(
            "{}{}",
            "\u{2588}".repeat(ask_bar_len),
            " ".repeat(half_width.saturating_sub(ask_bar_len))
        );

        lines.push(Line::from(vec![
            Span::styled(bid_bar, Style::default().fg(Color::Green)),
            Span::styled(ask_bar, Style::default().fg(Color::Red)),
        ]));
    }

    let p = Paragraph::new(lines);
    f.render_widget(p, inner);
}
