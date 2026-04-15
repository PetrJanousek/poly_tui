use std::time::Instant;

use crate::model::OrderbookSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackSpeed {
    X1,
    X2,
    X5,
    X10,
}

impl PlaybackSpeed {
    pub fn multiplier(self) -> f64 {
        match self {
            Self::X1 => 1.0,
            Self::X2 => 2.0,
            Self::X5 => 5.0,
            Self::X10 => 10.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X5 => "5x",
            Self::X10 => "10x",
        }
    }

    pub fn faster(self) -> Self {
        match self {
            Self::X1 => Self::X2,
            Self::X2 => Self::X5,
            Self::X5 => Self::X10,
            Self::X10 => Self::X10,
        }
    }

    pub fn slower(self) -> Self {
        match self {
            Self::X1 => Self::X1,
            Self::X2 => Self::X1,
            Self::X5 => Self::X2,
            Self::X10 => Self::X5,
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
        let current_ts = snapshots[self.cursor].timestamp;
        let next_ts = snapshots[self.cursor + 1].timestamp;
        let real_gap_ms = (next_ts - current_ts).num_milliseconds().max(0) as f64;
        let scaled_gap_ms = real_gap_ms / self.speed.multiplier();

        if elapsed.as_millis() as f64 >= scaled_gap_ms {
            self.cursor += 1;
            self.last_tick = Instant::now();
            true
        } else {
            false
        }
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
