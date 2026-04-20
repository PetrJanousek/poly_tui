use super::{Strategy, StrategyTrade};
use crate::model::OrderbookSnapshot;

/// Threshold strategy: buy when best ask is cheap, sell when best bid is rich.
/// Manages a single position per outcome independently.
pub struct ThresholdStrategy {
    pub buy_threshold: f64,  // buy when best_ask < this
    pub sell_threshold: f64, // sell when best_bid > this
    pub position_size: f64,  // fixed size per trade

    up_inventory: f64,
    down_inventory: f64,
}

impl ThresholdStrategy {
    pub fn new(buy_threshold: f64, sell_threshold: f64, position_size: f64) -> Self {
        Self {
            buy_threshold,
            sell_threshold,
            position_size,
            up_inventory: 0.0,
            down_inventory: 0.0,
        }
    }
}

impl Default for ThresholdStrategy {
    fn default() -> Self {
        Self::new(0.35, 0.65, 10.0)
    }
}

impl Strategy for ThresholdStrategy {
    fn name(&self) -> &str {
        "threshold"
    }

    fn on_snapshot(&mut self, snap: &OrderbookSnapshot, outcome: &str) -> Vec<StrategyTrade> {
        let inventory = match outcome {
            "Up" => &mut self.up_inventory,
            _ => &mut self.down_inventory,
        };

        let best_ask = snap.ask_prices.first().copied();
        let best_bid = snap.bid_prices.first().copied();
        let mut signals = Vec::new();

        if let Some(ask) = best_ask {
            if ask < self.buy_threshold && *inventory == 0.0 {
                *inventory += self.position_size;
                signals.push(StrategyTrade {
                    side: "BUY".to_string(),
                    outcome: outcome.to_string(),
                    price: ask,
                    size: self.position_size,
                });
            }
        }

        if let Some(bid) = best_bid {
            if bid > self.sell_threshold && *inventory > 0.0 {
                let size = *inventory;
                *inventory = 0.0;
                signals.push(StrategyTrade {
                    side: "SELL".to_string(),
                    outcome: outcome.to_string(),
                    price: bid,
                    size,
                });
            }
        }

        signals
    }
}
