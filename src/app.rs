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
