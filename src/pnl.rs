use crate::fees::{CRYPTO_FEE_RATE, calc_fee};
use crate::model::{Resolution, UserTrade};

#[derive(Debug, Default)]
pub struct OutcomePosition {
    pub inventory: f64,
    pub cost_basis: f64,
    pub realized_pnl: f64,
}

impl OutcomePosition {
    pub fn avg_cost(&self) -> f64 {
        if self.inventory > 0.0 {
            self.cost_basis / self.inventory
        } else {
            0.0
        }
    }

    pub fn unrealized_pnl(&self, mid_price: f64) -> f64 {
        if self.inventory > 0.0 {
            self.inventory * (mid_price - self.avg_cost())
        } else {
            0.0
        }
    }
}

/// One completed merge redemption (1 Up + 1 Down → $1.00).
#[derive(Debug, Clone)]
pub struct MergeEvent {
    pub qty: f64,
    pub avg_up: f64,
    pub avg_down: f64,
}

impl MergeEvent {
    pub fn sum(&self) -> f64 { self.avg_up + self.avg_down }
    pub fn gross(&self) -> f64 { self.qty * (1.0 - self.sum()) }
    pub fn entry_fees(&self) -> f64 {
        calc_fee(self.qty, self.avg_up, CRYPTO_FEE_RATE)
            + calc_fee(self.qty, self.avg_down, CRYPTO_FEE_RATE)
    }
    pub fn net(&self) -> f64 { self.gross() - self.entry_fees() }
}

#[derive(Debug, Default)]
pub struct PnlTracker {
    pub up: OutcomePosition,
    pub down: OutcomePosition,
    pub fees_paid: f64,
    pub merges: Vec<MergeEvent>,
    // Holds the Up leg of a pending merge until the Down leg arrives.
    merge_pending_up: Option<(f64, f64)>, // (qty, avg_up)
    pub trades_processed: usize,
}

impl PnlTracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn process_trades(&mut self, trades: &[UserTrade], count: usize) {
        // If cursor moved backward, reset and reprocess from scratch
        if count < self.trades_processed {
            self.reset();
        }
        while self.trades_processed < count {
            let trade = &trades[self.trades_processed];
            let pos = match trade.outcome.as_str() {
                "Up" => &mut self.up,
                "Down" => &mut self.down,
                _ => {
                    self.trades_processed += 1;
                    continue;
                }
            };

            let side = trade.side.to_uppercase();
            let is_merge_redemption = side == "SELL"
                && (trade.price == 1.0 || trade.price == 0.0);

            if side == "BUY" {
                pos.inventory += trade.size;
                pos.cost_basis += trade.price * trade.size;
            } else if side == "SELL" {
                let avg = pos.avg_cost();
                let sell_amount = trade.size.min(pos.inventory);
                pos.realized_pnl += (trade.price - avg) * sell_amount;

                // Record per-merge events. Up leg arrives first; Down leg completes the pair.
                if is_merge_redemption {
                    match trade.outcome.as_str() {
                        "Up" => {
                            self.merge_pending_up = Some((sell_amount, avg));
                        }
                        "Down" => {
                            if let Some((up_qty, avg_up)) = self.merge_pending_up.take() {
                                self.merges.push(MergeEvent { qty: up_qty, avg_up, avg_down: avg });
                            }
                        }
                        _ => {}
                    }
                }

                pos.cost_basis -= avg * sell_amount;
                pos.inventory -= sell_amount;
            }

            // Takers pay fees on every trade (both BUY and SELL).
            // Merge SELLs at price 1.0/0.0 are exempt — they are contract
            // redemptions, not market trades.
            if trade.is_taker && !is_merge_redemption {
                self.fees_paid += calc_fee(trade.size, trade.price, CRYPTO_FEE_RATE);
            }

            self.trades_processed += 1;
        }
    }

    pub fn total_unrealized(&self, up_mid: f64, down_mid: f64) -> f64 {
        self.up.unrealized_pnl(up_mid) + self.down.unrealized_pnl(down_mid)
    }

    pub fn total_realized(&self) -> f64 {
        self.up.realized_pnl + self.down.realized_pnl
    }

    pub fn resolution_pnl(&self, resolution: &Resolution) -> f64 {
        let up_payout = if resolution.winning_outcome == "Up" {
            1.0
        } else {
            0.0
        };
        let down_payout = 1.0 - up_payout;

        let up_pnl = self.up.inventory * up_payout - self.up.cost_basis + self.up.realized_pnl;
        let down_pnl =
            self.down.inventory * down_payout - self.down.cost_basis + self.down.realized_pnl;
        up_pnl + down_pnl
    }
}
