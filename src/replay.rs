use std::time::Instant;

use crate::model::OrderbookSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackSpeed {
    X1,
    X2,
    X5,
    X10,
    X30,
    X50,
    X200,
    X2000,
}

impl PlaybackSpeed {
    pub fn multiplier(self) -> f64 {
        match self {
            Self::X1 => 1.0,
            Self::X2 => 2.0,
            Self::X5 => 5.0,
            Self::X10 => 10.0,
            Self::X30 => 30.0,
            Self::X50 => 50.0,
            Self::X200 => 200.0,
            Self::X2000 => 2000.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X5 => "5x",
            Self::X10 => "10x",
            Self::X30 => "30x",
            Self::X50 => "50x",
            Self::X200 => "200x",
            Self::X2000 => "2000x",
        }
    }

    pub fn faster(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X5,
            Self::X5 => Self::X10,
            Self::X10 => Self::X30,
            Self::X30 => Self::X50,
            Self::X50 => Self::X200,
            Self::X200 => Self::X2000,
            Self::X2000 => Self::X2000,
        }
    }

    pub fn slower(self) -> Self {
        match self {
            Self::X1 => Self::X1,
            Self::X2 => Self::X1,
            Self::X5 => Self::X2,
            Self::X10 => Self::X5,
            Self::X30 => Self::X10,
            Self::X50 => Self::X30,
            Self::X200 => Self::X50,
            Self::X2000 => Self::X200,
        }
    }
}

pub struct ReplayState {
    pub cursor: usize,
    pub playing: bool,
    pub speed: PlaybackSpeed,
    pub last_tick: Instant,
}

impl ReplayState {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            playing: false,
            speed: PlaybackSpeed::X1,
            last_tick: Instant::now(),
        }
    }

    pub fn tick(&mut self, snapshots: &[OrderbookSnapshot]) -> bool {
        if !self.playing || snapshots.is_empty() || self.cursor >= snapshots.len() - 1 {
            return false;
        }

        let elapsed = self.last_tick.elapsed();
        self.last_tick = Instant::now();

        // Convert elapsed real time to simulated time budget (ms)
        let mut budget_ms = elapsed.as_secs_f64() * 1000.0 * self.speed.multiplier();

        let mut advanced = false;
        while self.cursor < snapshots.len() - 1 && budget_ms > 0.0 {
            let current_ts = snapshots[self.cursor].timestamp;
            let next_ts = snapshots[self.cursor + 1].timestamp;
            let gap_ms = (next_ts - current_ts).num_milliseconds().max(0) as f64;

            if budget_ms >= gap_ms {
                self.cursor += 1;
                budget_ms -= gap_ms;
                advanced = true;
            } else {
                break;
            }
        }

        advanced
    }

    pub fn step_forward(&mut self, max: usize) {
        if self.cursor < max.saturating_sub(1) {
            self.cursor += 1;
            self.last_tick = Instant::now();
        }
    }

    pub fn step_backward(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.last_tick = Instant::now();
    }

    pub fn toggle_pause(&mut self) {
        self.playing = !self.playing;
        self.last_tick = Instant::now();
    }

    pub fn speed_up(&mut self) {
        self.speed = self.speed.faster();
    }

    pub fn slow_down(&mut self) {
        self.speed = self.speed.slower();
    }

    /// Count how many trades should be visible at the current cursor position.
    pub fn visible_trade_count(
        &self,
        snapshots: &[OrderbookSnapshot],
        trades: &[crate::model::UserTrade],
    ) -> usize {
        if snapshots.is_empty() {
            return 0;
        }
        let current_ts = snapshots[self.cursor].timestamp;
        trades
            .iter()
            .take_while(|t| t.timestamp <= current_ts)
            .count()
    }
}
