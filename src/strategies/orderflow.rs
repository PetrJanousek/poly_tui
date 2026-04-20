use super::{Strategy, StrategyTrade};
use crate::fees::{CRYPTO_FEE_RATE, calc_fee};
use crate::model::{OrderbookSnapshot, Trade};

/// Order flow imbalance strategy with automatic position balancing.
///
/// Entry: when (up_flow − down_flow) exceeds `flow_threshold`, buys `position_size`
/// of the dominant outcome. Flow resets after each entry.
///
/// Hedge: at every snapshot, if one side is larger than the other AND the current
/// ask on the smaller side is below the break-even max price, buys the exact
/// imbalance quantity to lock in a guaranteed profit regardless of resolution.
///
/// Stops all activity once avg_up_cost + avg_down_cost < `sum_cost_min` — the
/// position is already locked in for profit.
pub struct OrderFlowStrategy {
    pub flow_threshold: f64,
    pub position_size: f64,

    up_flow: f64,
    down_flow: f64,
    up_cost_basis: f64,
    up_shares: f64,
    down_cost_basis: f64,
    down_shares: f64,
}

impl OrderFlowStrategy {
    pub fn new(flow_threshold: f64, position_size: f64) -> Self {
        Self {
            flow_threshold,
            position_size,
            up_flow: 0.0,
            down_flow: 0.0,
            up_cost_basis: 0.0,
            up_shares: 0.0,
            down_cost_basis: 0.0,
            down_shares: 0.0,
        }
    }
}

impl Default for OrderFlowStrategy {
    fn default() -> Self {
        Self::new(1000.0, 5.0)
    }
}

impl Strategy for OrderFlowStrategy {
    fn name(&self) -> &str {
        "orderflow"
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
        let total_cost = self.up_cost_basis + self.down_cost_basis;

        // --- Phase 1: hedge / balance ---
        // If this outcome is the smaller side, check whether buying the imbalance
        // at the current ask still locks in a profit.
        let (my_shares, other_shares) = match outcome {
            "Up" => (self.up_shares, self.down_shares),
            _ => (self.down_shares, self.up_shares),
        };

        if other_shares > my_shares {
            let need_qty = other_shares - my_shares;
            let locked_payout = other_shares; // both sides equal after hedge
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
                    return vec![StrategyTrade {
                        side: "BUY".to_string(),
                        outcome: outcome.to_string(),
                        price: ask,
                        size: need_qty,
                    }];
                }
            }
        }

        // --- Phase 2: flow-based entry ---
        // Stop new entries if both sides are held and the position is already
        // net-profitable after fees — no more risk needed.
        if self.up_shares > 0.0 && self.down_shares > 0.0 {
            let avg_up = self.up_cost_basis / self.up_shares;
            let avg_down = self.down_cost_basis / self.down_shares;
            let gross = 1.0 - avg_up - avg_down;
            let entry_fees = calc_fee(1.0, avg_up, CRYPTO_FEE_RATE)
                + calc_fee(1.0, avg_down, CRYPTO_FEE_RATE);
            if gross > entry_fees {
                return vec![];
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
            return vec![];
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

        // Reset flow — next entry needs fresh imbalance.
        self.up_flow = 0.0;
        self.down_flow = 0.0;

        vec![StrategyTrade {
            side: "BUY".to_string(),
            outcome: outcome.to_string(),
            price,
            size: self.position_size,
        }]
    }
}
