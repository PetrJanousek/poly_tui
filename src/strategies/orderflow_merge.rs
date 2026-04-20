use super::{Strategy, StrategyTrade};
use crate::model::{OrderbookSnapshot, Trade};

/// Order flow imbalance strategy with continuous merging.
///
/// Identical entry/hedge logic to `OrderFlowStrategy`, but after every snapshot
/// automatically merges any balanced pairs whose avg_sum < 1.0:
///   SELL n Up  @ 1.0  (Up side captures the full $1 payout)
///   SELL n Down @ 0.0  (Down side closes at zero — net = $1 per pair)
///
/// This models Polymarket's merge redemption: 1 Up + 1 Down → $1.00 USDC,
/// instantly freeing capital for the next entry.
pub struct OrderFlowMergeStrategy {
    pub flow_threshold: f64,
    pub position_size: f64,
    pub sum_cost_min: f64,

    up_flow: f64,
    down_flow: f64,
    up_cost_basis: f64,
    up_shares: f64,
    down_cost_basis: f64,
    down_shares: f64,
}

impl OrderFlowMergeStrategy {
    pub fn new(flow_threshold: f64, position_size: f64, sum_cost_min: f64) -> Self {
        Self {
            flow_threshold,
            position_size,
            sum_cost_min,
            up_flow: 0.0,
            down_flow: 0.0,
            up_cost_basis: 0.0,
            up_shares: 0.0,
            down_cost_basis: 0.0,
            down_shares: 0.0,
        }
    }

    fn avg_up(&self) -> f64 {
        if self.up_shares > 0.0 { self.up_cost_basis / self.up_shares } else { 0.0 }
    }

    fn avg_down(&self) -> f64 {
        if self.down_shares > 0.0 { self.down_cost_basis / self.down_shares } else { 0.0 }
    }
}

impl Default for OrderFlowMergeStrategy {
    fn default() -> Self {
        Self::new(1000.0, 5.0, 0.98)
    }
}

impl Strategy for OrderFlowMergeStrategy {
    fn name(&self) -> &str {
        "orderflow_merge"
    }

    fn on_market_start(&mut self) {
        self.up_flow = 0.0;
        self.down_flow = 0.0;
        self.up_cost_basis = 0.0;
        self.up_shares = 0.0;
        self.down_cost_basis = 0.0;
        self.down_shares = 0.0;
    }

    fn on_trade(&mut self, trade: &Trade) {
        match trade.outcome.as_str() {
            "Up" => self.up_flow += trade.size,
            _ => self.down_flow += trade.size,
        }
    }

    fn on_snapshot(&mut self, snap: &OrderbookSnapshot, outcome: &str) -> Vec<StrategyTrade> {
        let mut signals = Vec::new();

        // --- Phase 0: merge any profitable balanced pairs ---
        // min(up, down) pairs can always be redeemed for $1.00 each from the contract.
        // Model as SELL Up @ 1.0 + SELL Down @ 0.0 so PnlTracker books the correct
        // realized profit: (1.0 - avg_up) * n + (0.0 - avg_down) * n = (1 - sum) * n
        let merge_qty = self.up_shares.min(self.down_shares);
        if merge_qty > 0.0 {
            let avg_up = self.avg_up();
            let avg_down = self.avg_down();
            if avg_up + avg_down < 1.0 {
                signals.push(StrategyTrade {
                    side: "SELL".to_string(),
                    outcome: "Up".to_string(),
                    price: 1.0,
                    size: merge_qty,
                });
                signals.push(StrategyTrade {
                    side: "SELL".to_string(),
                    outcome: "Down".to_string(),
                    price: 0.0,
                    size: merge_qty,
                });
                self.up_cost_basis -= avg_up * merge_qty;
                self.up_shares -= merge_qty;
                self.down_cost_basis -= avg_down * merge_qty;
                self.down_shares -= merge_qty;
            }
        }

        // --- Phase 1: hedge / balance ---
        let total_cost = self.up_cost_basis + self.down_cost_basis;
        let (my_shares, other_shares) = match outcome {
            "Up" => (self.up_shares, self.down_shares),
            _ => (self.down_shares, self.up_shares),
        };

        if other_shares > my_shares {
            let need_qty = other_shares - my_shares;
            let locked_payout = other_shares;
            let max_price = (locked_payout - total_cost) / need_qty;

            if let Some(&ask) = snap.ask_prices.first() {
                if ask > 0.0 && ask <= max_price {
                    match outcome {
                        "Up" => {
                            self.up_cost_basis += ask * need_qty;
                            self.up_shares += need_qty;
                        }
                        _ => {
                            self.down_cost_basis += ask * need_qty;
                            self.down_shares += need_qty;
                        }
                    }
                    signals.push(StrategyTrade {
                        side: "BUY".to_string(),
                        outcome: outcome.to_string(),
                        price: ask,
                        size: need_qty,
                    });
                    return signals;
                }
            }
        }

        // --- Phase 2: flow-based entry ---
        if self.up_shares > 0.0 && self.down_shares > 0.0 {
            let avg_up = self.avg_up();
            let avg_down = self.avg_down();
            if avg_up + avg_down < self.sum_cost_min {
                return signals;
            }
        }

        let imbalance = self.up_flow - self.down_flow;
        let signal_outcome = if imbalance > self.flow_threshold {
            Some("Up")
        } else if imbalance < -self.flow_threshold {
            Some("Down")
        } else {
            None
        };

        if signal_outcome != Some(outcome) {
            return signals;
        }

        let price = snap.ask_prices.first().copied().unwrap_or(0.5);

        match outcome {
            "Up" => {
                self.up_cost_basis += price * self.position_size;
                self.up_shares += self.position_size;
            }
            _ => {
                self.down_cost_basis += price * self.position_size;
                self.down_shares += self.position_size;
            }
        }

        self.up_flow = 0.0;
        self.down_flow = 0.0;

        signals.push(StrategyTrade {
            side: "BUY".to_string(),
            outcome: outcome.to_string(),
            price,
            size: self.position_size,
        });
        signals
    }
}
