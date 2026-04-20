pub mod orderflow;
pub mod orderflow_merge;
pub mod threshold;

use crate::model::{OrderbookSnapshot, Trade, UserTrade};

/// A single trade signal emitted by a strategy.
pub struct StrategyTrade {
    pub side: String,    // "BUY" or "SELL"
    pub outcome: String, // "Up" or "Down"
    pub price: f64,
    pub size: f64,
}

/// Implement this trait to define a strategy.
///
/// `on_market_start` — called once before any events (reset state here).
/// `on_trade`        — called for every market trade in chronological order (default: no-op).
/// `on_snapshot`     — called at each orderbook snapshot; return trades to execute.
pub trait Strategy {
    fn name(&self) -> &str;
    fn on_market_start(&mut self) {}
    fn on_trade(&mut self, _trade: &Trade) {}
    fn on_snapshot(&mut self, snap: &OrderbookSnapshot, outcome: &str) -> Vec<StrategyTrade>;
}

/// Run a strategy over both outcome snapshot series, interleaving market trades
/// chronologically so `on_trade` fires before the next snapshot in time.
pub fn run_strategy_both_outcomes(
    strategy: &mut dyn Strategy,
    up_snapshots: &[OrderbookSnapshot],
    down_snapshots: &[OrderbookSnapshot],
    all_trades: &[Trade],
) -> Vec<UserTrade> {
    strategy.on_market_start();

    // Merge Up and Down snapshots into a single chronological sequence.
    let mut snap_events: Vec<(&OrderbookSnapshot, &str)> = up_snapshots
        .iter()
        .map(|s| (s, "Up"))
        .chain(down_snapshots.iter().map(|s| (s, "Down")))
        .collect();
    snap_events.sort_by_key(|(s, _)| s.timestamp);

    let mut trade_idx = 0;
    let mut result = Vec::new();

    for (snap, outcome) in &snap_events {
        // Feed all market trades whose timestamp precedes this snapshot.
        while trade_idx < all_trades.len()
            && all_trades[trade_idx].timestamp <= snap.timestamp
        {
            strategy.on_trade(&all_trades[trade_idx]);
            trade_idx += 1;
        }

        // Ask strategy for execution signals at this snapshot.
        for signal in strategy.on_snapshot(snap, outcome) {
            let hash = format!(
                "bt_{}_{}_{}", strategy.name(),
                snap.timestamp.and_utc().timestamp_millis(),
                signal.outcome,
            );
            result.push(UserTrade {
                timestamp: snap.timestamp,
                side: signal.side,
                outcome: signal.outcome,
                price: signal.price,
                size: signal.size,
                transaction_hash: hash,
                is_taker: true,
            });
        }
    }

    result.sort_by_key(|t| t.timestamp);
    result
}
