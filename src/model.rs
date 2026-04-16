use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct Market {
    pub condition_id: String,
    pub crypto: String,
    pub question: String,
    pub slug: String,
    pub end_date: Option<NaiveDateTime>,
}

#[derive(Debug, Clone)]
pub struct OrderbookSnapshot {
    pub timestamp: NaiveDateTime,
    pub outcome: String,
    pub bid_prices: Vec<f64>,
    pub bid_sizes: Vec<f64>,
    pub ask_prices: Vec<f64>,
    pub ask_sizes: Vec<f64>,
}

impl OrderbookSnapshot {
    pub fn mid_price(&self) -> Option<f64> {
        let best_bid = self.bid_prices.first()?;
        let best_ask = self.ask_prices.first()?;
        Some((best_bid + best_ask) / 2.0)
    }
}

#[derive(Debug, Clone)]
pub struct UserTrade {
    pub timestamp: NaiveDateTime,
    pub side: String,
    pub outcome: String,
    pub price: f64,
    pub size: f64,
    pub transaction_hash: String,
    pub is_taker: bool,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub winning_outcome: String,
    pub yes_price: f64,
    pub no_price: f64,
}

pub struct MarketData {
    pub market: Market,
    pub snapshots: Vec<OrderbookSnapshot>,
    pub user_trades: Vec<UserTrade>,
    pub resolution: Option<Resolution>,
}
