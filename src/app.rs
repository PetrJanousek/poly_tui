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
    pub show_outcome: String, // "Up" or "Down"

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
            show_outcome: "Up".to_string(),
            user_addresses,
        }
    }

    pub fn selected_market(&self) -> Option<&Market> {
        self.market_list_state
            .selected()
            .and_then(|i| self.markets.get(i))
    }

    pub fn on_tick(&mut self) {
        if let Some(data) = &self.market_data {
            let outcome_snapshots: Vec<_> = data
                .snapshots
                .iter()
                .filter(|s| s.outcome == self.show_outcome)
                .cloned()
                .collect();

            self.replay.tick(&outcome_snapshots);
            self.sync_pnl();
        }
    }

    /// Recalculate visible trades and update PnL to match current cursor position.
    pub fn sync_pnl(&mut self) {
        if let Some(data) = &self.market_data {
            let outcome_snapshots: Vec<_> = data
                .snapshots
                .iter()
                .filter(|s| s.outcome == self.show_outcome)
                .cloned()
                .collect();

            let visible = self
                .replay
                .visible_trade_count(&outcome_snapshots, &data.user_trades);
            self.pnl.process_trades(&data.user_trades, visible);
        }
    }

    pub fn enter_replay(&mut self, data: MarketData) {
        self.market_data = Some(data);
        self.replay = ReplayState::new();
        self.pnl.reset();
        self.show_outcome = "Up".to_string();
        self.mode = AppMode::Replay;
    }

    pub fn exit_replay(&mut self) {
        self.mode = AppMode::MarketBrowser;
        self.market_data = None;
    }

    pub fn toggle_outcome(&mut self) {
        self.show_outcome = if self.show_outcome == "Up" {
            "Down".to_string()
        } else {
            "Up".to_string()
        };
        // Reset replay for new outcome view
        self.replay = ReplayState::new();
        self.pnl.reset();
    }

    pub fn current_snapshots(&self) -> Vec<&crate::model::OrderbookSnapshot> {
        self.market_data
            .as_ref()
            .map(|d| {
                d.snapshots
                    .iter()
                    .filter(|s| s.outcome == self.show_outcome)
                    .collect()
            })
            .unwrap_or_default()
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
