use chrono::NaiveDateTime;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph},
};

use crate::model::MarketData;

pub fn render(f: &mut Frame, data: &MarketData, current_ts: NaiveDateTime, area: Rect) {
    let cl = &data.chainlink_prices;
    let bn = &data.binance_prices;

    if cl.is_empty() && bn.is_empty() {
        f.render_widget(
            Paragraph::new("no price data").block(
                Block::default()
                    .title(" price ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    }

    // Fixed x bounds from full dataset so the scale doesn't jump as replay progresses
    let all_x_full = cl.iter().chain(bn.iter()).map(|(ts, _)| ts.and_utc().timestamp() as f64);
    let x_min = all_x_full.clone().fold(f64::INFINITY, f64::min);
    let x_max = all_x_full.fold(f64::NEG_INFINITY, f64::max);

    // Fixed y bounds from full dataset
    let all_prices_full = cl.iter().chain(bn.iter()).map(|(_, p)| *p);
    let y_min = all_prices_full.clone().fold(f64::INFINITY, f64::min);
    let y_max = all_prices_full.fold(f64::NEG_INFINITY, f64::max);
    let margin = ((y_max - y_min) * 0.05).max(0.001);
    let y_lo = (y_min - margin).max(0.0);
    let y_hi = y_max + margin;

    // Only render data points up to current_ts so the line "grows" as replay advances
    let cl_pts: Vec<(f64, f64)> = cl
        .iter()
        .take_while(|(ts, _)| *ts <= current_ts)
        .map(|(ts, p)| (ts.and_utc().timestamp() as f64, *p))
        .collect();
    let bn_pts: Vec<(f64, f64)> = bn
        .iter()
        .take_while(|(ts, _)| *ts <= current_ts)
        .map(|(ts, p)| (ts.and_utc().timestamp() as f64, *p))
        .collect();

    let mut datasets: Vec<Dataset> = Vec::new();
    if !cl_pts.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Cyan))
                .data(&cl_pts),
        );
    }
    if !bn_pts.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Yellow))
                .data(&bn_pts),
        );
    }

    // Current prices in title
    let cl_cur = price_at(cl, current_ts)
        .map(|p| format!(" C:{}", fmt_price(p)))
        .unwrap_or_default();
    let bn_cur = price_at(bn, current_ts)
        .map(|p| format!(" B:{}", fmt_price(p)))
        .unwrap_or_default();
    let crypto = data.market.crypto.to_uppercase();
    let title = format!(" {crypto}{cl_cur}{bn_cur} ");

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .x_axis(Axis::default().bounds([x_min, x_max]))
        .y_axis(
            Axis::default()
                .bounds([y_lo, y_hi])
                .labels(vec![
                    Span::raw(fmt_price(y_lo)),
                    Span::raw(fmt_price(y_hi)),
                ])
                .style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(chart, area);
}

fn price_at(series: &[(NaiveDateTime, f64)], ts: NaiveDateTime) -> Option<f64> {
    let idx = series.partition_point(|(t, _)| *t <= ts);
    idx.checked_sub(1).map(|i| series[i].1)
}

fn fmt_price(p: f64) -> String {
    if p >= 1000.0 {
        format!("{:.0}", p)
    } else if p >= 1.0 {
        format!("{:.2}", p)
    } else {
        format!("{:.4}", p)
    }
}
