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
        .title(" Order Book ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let Some(snap) = snapshot else {
        let p = Paragraph::new("No data").block(block);
        f.render_widget(p, area);
        return;
    };

    // Find max size for bar scaling
    let max_size = snap
        .bid_sizes
        .iter()
        .chain(snap.ask_sizes.iter())
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1.0);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Available width for bars (leave room for price + size text)
    let bar_width = (inner.width as usize).saturating_sub(22);

    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled("  PRICE", Style::default().fg(Color::DarkGray)),
        Span::raw("    "),
        Span::styled("SIZE", Style::default().fg(Color::DarkGray)),
        Span::raw("    "),
        Span::styled("DEPTH", Style::default().fg(Color::DarkGray)),
    ]));
    lines.push(Line::raw(""));

    // Asks (reversed so best ask is closest to center)
    for i in (0..snap.ask_prices.len()).rev() {
        let price = snap.ask_prices[i];
        let size = snap.ask_sizes[i];
        let bar_len = ((size / max_size) * bar_width as f64) as usize;
        let bar = "\u{2588}".repeat(bar_len);

        lines.push(Line::from(vec![
            Span::styled(format!("{price:>8.4}"), Style::default().fg(Color::Red)),
            Span::raw("  "),
            Span::styled(format!("{size:>8.2}"), Style::default().fg(Color::Red)),
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(Color::Red)),
        ]));
    }

    // Spread line
    if let (Some(&best_bid), Some(&best_ask)) =
        (snap.bid_prices.first(), snap.ask_prices.first())
    {
        let spread = best_ask - best_bid;
        lines.push(Line::from(Span::styled(
            format!("  spread: {spread:.4}"),
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Bids
    for i in 0..snap.bid_prices.len() {
        let price = snap.bid_prices[i];
        let size = snap.bid_sizes[i];
        let bar_len = ((size / max_size) * bar_width as f64) as usize;
        let bar = "\u{2588}".repeat(bar_len);

        lines.push(Line::from(vec![
            Span::styled(format!("{price:>8.4}"), Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(format!("{size:>8.2}"), Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(Color::Green)),
        ]));
    }

    let p = Paragraph::new(lines);
    f.render_widget(p, inner);
}
