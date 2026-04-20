use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::model::{OrderbookSnapshot, Resolution};
use crate::pnl::PnlTracker;

pub fn render(
    f: &mut Frame,
    pnl: &PnlTracker,
    up_snapshot: Option<&OrderbookSnapshot>,
    down_snapshot: Option<&OrderbookSnapshot>,
    resolution: Option<&Resolution>,
    area: Rect,
) {
    let block = Block::default()
        .title(" PnL ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let up_mid = up_snapshot.and_then(|s| s.mid_price()).unwrap_or(0.0);
    let down_mid = down_snapshot.and_then(|s| s.mid_price()).unwrap_or(0.0);

    let mut lines = Vec::new();

    // Inventory
    lines.push(Line::from(vec![
        Span::styled("Up:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.2} shares", pnl.up.inventory),
            Style::default().fg(Color::White),
        ),
        Span::raw("  avg "),
        Span::styled(
            format!("{:.4}", pnl.up.avg_cost()),
            Style::default().fg(Color::Yellow),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Down: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.2} shares", pnl.down.inventory),
            Style::default().fg(Color::White),
        ),
        Span::raw("  avg "),
        Span::styled(
            format!("{:.4}", pnl.down.avg_cost()),
            Style::default().fg(Color::Yellow),
        ),
    ]));

    let up_avg = pnl.up.avg_cost();
    let down_avg = pnl.down.avg_cost();
    if up_avg > 0.0 && down_avg > 0.0 {
        let sum = up_avg + down_avg;
        let sum_color = if sum < 1.0 { Color::Green } else { Color::Red };
        lines.push(Line::from(vec![
            Span::styled("Sum:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{sum:.4}"), Style::default().fg(sum_color)),
            Span::styled(
                format!(" ({:+.4})", sum - 1.0),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    lines.push(Line::raw(""));

    // Realized PnL
    let realized = pnl.total_realized();
    let realized_color = if realized >= 0.0 { Color::Green } else { Color::Red };
    lines.push(Line::from(vec![
        Span::styled("Realized:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{realized:+.4}"), Style::default().fg(realized_color)),
    ]));

    // Unrealized PnL
    let unrealized = pnl.total_unrealized(up_mid, down_mid);
    let unreal_color = if unrealized >= 0.0 { Color::Green } else { Color::Red };
    lines.push(Line::from(vec![
        Span::styled("Unrealized: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{unrealized:+.4}"), Style::default().fg(unreal_color)),
    ]));

    // Total
    let total = realized + unrealized;
    let total_color = if total >= 0.0 { Color::Green } else { Color::Red };
    lines.push(Line::from(vec![
        Span::styled("Total:      ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{total:+.4}"), Style::default().fg(total_color)),
    ]));

    // Hedge calculation
    let up_inv = pnl.up.inventory;
    let down_inv = pnl.down.inventory;
    if up_inv > 0.0 || down_inv > 0.0 {
        lines.push(Line::raw(""));

        let total_cost = pnl.up.cost_basis + pnl.down.cost_basis;
        let realized_adj = pnl.up.realized_pnl + pnl.down.realized_pnl;

        if (up_inv - down_inv).abs() < 0.01 {
            // Already balanced — show locked profit
            let locked = up_inv - total_cost + realized_adj;
            let locked_color = if locked >= 0.0 { Color::Green } else { Color::Red };
            lines.push(Line::from(vec![
                Span::styled("Hedge: ", Style::default().fg(Color::DarkGray)),
                Span::styled("balanced  ", Style::default().fg(Color::Green)),
                Span::styled(format!("{locked:+.2}"), Style::default().fg(locked_color)),
            ]));
        } else {
            let (need_outcome, need_qty, current_ask) = if up_inv > down_inv {
                let ask = down_snapshot.and_then(|s| s.ask_prices.first().copied());
                ("Down", up_inv - down_inv, ask)
            } else {
                let ask = up_snapshot.and_then(|s| s.ask_prices.first().copied());
                ("Up", down_inv - up_inv, ask)
            };

            let locked_payout = up_inv.max(down_inv);
            let max_price = if need_qty > 0.0 {
                (locked_payout - total_cost + realized_adj) / need_qty
            } else {
                0.0
            };

            let max_color = if max_price > 0.0 { Color::Green } else { Color::Red };
            lines.push(Line::from(vec![
                Span::styled("Hedge: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{need_qty:.0} {need_outcome}"),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  max "),
                Span::styled(format!("{max_price:.4}"), Style::default().fg(max_color)),
            ]));

            if let Some(ask) = current_ask {
                let locked = locked_payout - total_cost + realized_adj - need_qty * ask;
                let ask_color = if ask <= max_price { Color::Green } else { Color::Red };
                let locked_color = if locked >= 0.0 { Color::Green } else { Color::Red };
                lines.push(Line::from(vec![
                    Span::styled("       ask ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{ask:.4}"), Style::default().fg(ask_color)),
                    Span::raw("  → "),
                    Span::styled(format!("{locked:+.2}"), Style::default().fg(locked_color)),
                ]));
            }
        }
    }

    // Resolution
    if let Some(res) = resolution {
        lines.push(Line::raw(""));
        let final_pnl = pnl.resolution_pnl(res);
        let res_color = if final_pnl >= 0.0 { Color::Green } else { Color::Red };
        lines.push(Line::from(vec![
            Span::styled("Resolved: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&res.winning_outcome, Style::default().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Final PnL:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{final_pnl:+.4}"), Style::default().fg(res_color)),
        ]));
    }

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}
