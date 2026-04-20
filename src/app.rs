use chrono::NaiveDate;
use ratatui::widgets::ListState;

use crate::model::{Market, MarketData};
use crate::pnl::PnlTracker;
use crate::replay::ReplayState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    MarketBrowser,
    Replay,
}

/// Which source to use for "user" trades when loading a market.
#[derive(Debug, Clone, PartialEq)]
pub enum TradeSource {
    /// Fetch real trades from the Polymarket API.
    User,
    /// Run a built-in strategy over the loaded snapshots.
    Backtest(String),
}

impl TradeSource {
    pub fn label(&self) -> &str {
        match self {
            TradeSource::User => "user",
            TradeSource::Backtest(s) => s.as_str(),
        }
    }
}

/// Ordered list of available trade sources to cycle through with `b`.
const TRADE_SOURCES: &[&str] = &["user", "threshold", "orderflow", "orderflow_merge"];

pub const CRYPTOS: &[&str] = &["btc", "eth", "xrp", "sol", "hype", "doge", "bnb"];

pub struct App {
    pub mode: AppMode,

    // Market browser
    pub crypto_filter: Option<String>,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub markets: Vec<Market>,
    pub market_list_state: ListState,
    pub needs_refresh: bool,
    pub status_message: String,

    // Replay
    pub market_data: Option<MarketData>,
    pub replay: ReplayState,
    pub pnl: PnlTracker,
    pub show_all_trades: bool,

    // Trade source for replay (user trades vs backtest strategy)
    pub trade_source: TradeSource,

    // User addresses to track
    pub user_addresses: Vec<String>,
}

impl App {
    pub fn new(user_addresses: Vec<String>) -> Self {
        let today = chrono::Utc::now().date_naive();
        Self {
            mode: AppMode::MarketBrowser,
            crypto_filter: None,
            date_from: today,
            date_to: today,
            markets: Vec::new(),
            market_list_state: ListState::default(),
            needs_refresh: true,
            status_message: String::new(),
            market_data: None,
            replay: ReplayState::new(),
            pnl: PnlTracker::default(),
            show_all_trades: false,
            trade_source: TradeSource::User,
            user_addresses,
        }
    }

    pub fn selected_market(&self) -> Option<&Market> {
        self.market_list_state
            .selected()
            .and_then(|i| self.markets.get(i))
    }

    pub fn on_tick(&mut self) {
        if self.market_data.is_none() {
            return;
        }
        {
            let up_snaps = self.market_data.as_ref().unwrap().up_snapshots.as_slice();
            self.replay.tick(up_snaps);
        }
        self.sync_pnl();
    }

    /// Recalculate visible trades and update PnL to match current cursor position.
    pub fn sync_pnl(&mut self) {
        if let Some(data) = &self.market_data {
            let visible = self
                .replay
                .visible_trade_count(&data.up_snapshots, &data.user_trades);
            self.pnl.process_trades(&data.user_trades, visible);
        }
    }

    pub fn enter_replay(&mut self, data: MarketData) {
        self.market_data = Some(data);
        self.replay = ReplayState::new();
        self.pnl.reset();
        self.mode = AppMode::Replay;
    }

    pub fn exit_replay(&mut self) {
        self.mode = AppMode::MarketBrowser;
        self.market_data = None;
    }

    pub fn up_snapshots(&self) -> &[crate::model::OrderbookSnapshot] {
        self.market_data
            .as_ref()
            .map(|d| d.up_snapshots.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_up_snapshot(&self) -> Option<&crate::model::OrderbookSnapshot> {
        let snaps = self.up_snapshots();
        snaps.get(self.replay.cursor.min(snaps.len().saturating_sub(1)))
    }

    pub fn current_down_snapshot(&self) -> Option<&crate::model::OrderbookSnapshot> {
        let up = self.current_up_snapshot()?;
        self.market_data.as_ref()?.down_snapshot_at(up.timestamp)
    }

    /// Cycle through available trade sources (user → threshold → user → …).
    pub fn cycle_trade_source(&mut self) {
        let current = self.trade_source.label().to_string();
        let idx = TRADE_SOURCES.iter().position(|&s| s == current).unwrap_or(0);
        let next = TRADE_SOURCES[(idx + 1) % TRADE_SOURCES.len()];
        self.trade_source = if next == "user" {
            TradeSource::User
        } else {
            TradeSource::Backtest(next.to_string())
        };
    }

    pub fn set_crypto_filter(&mut self, idx: usize) {
        if let Some(&crypto) = CRYPTOS.get(idx) {
            self.crypto_filter = Some(crypto.to_string());
            self.needs_refresh = true;
        }
    }

    pub fn clear_filter(&mut self) {
        self.crypto_filter = None;
        self.needs_refresh = true;
    }

    pub fn move_date_back(&mut self) {
        self.date_from -= chrono::Duration::days(1);
        self.date_to = self.date_from;
        self.needs_refresh = true;
    }

    pub fn move_date_forward(&mut self) {
        self.date_from += chrono::Duration::days(1);
        self.date_to = self.date_from;
        self.needs_refresh = true;
    }
}
