use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DelayMode {
    #[default]
    None,
    Simple,
    Bronstein,
}

impl DelayMode {
    pub const ALL: [DelayMode; 3] = [DelayMode::None, DelayMode::Simple, DelayMode::Bronstein];

    pub fn label(self) -> &'static str {
        match self {
            DelayMode::None => "Fischer increment only",
            DelayMode::Simple => "Simple delay (US)",
            DelayMode::Bronstein => "Bronstein delay",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeControlCategory {
    Bullet,
    Blitz,
    Rapid,
    Classical,
}

impl TimeControlCategory {
    pub fn label(self) -> &'static str {
        match self {
            TimeControlCategory::Bullet => "Bullet",
            TimeControlCategory::Blitz => "Blitz",
            TimeControlCategory::Rapid => "Rapid",
            TimeControlCategory::Classical => "Classical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeControl {
    pub initial_seconds: u64,
    pub increment_seconds: u64,
    pub delay_seconds: u64,
    pub delay_mode: DelayMode,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self::fischer(600, 0)
    }
}

impl TimeControl {
    pub const fn fischer(initial_seconds: u64, increment_seconds: u64) -> Self {
        Self {
            initial_seconds,
            increment_seconds,
            delay_seconds: 0,
            delay_mode: DelayMode::None,
        }
    }

    pub fn initial(&self) -> Duration {
        Duration::from_secs(self.initial_seconds.max(1))
    }

    pub fn increment(&self) -> Duration {
        Duration::from_secs(self.increment_seconds)
    }

    pub fn delay(&self) -> Duration {
        match self.delay_mode {
            DelayMode::None => Duration::ZERO,
            _ => Duration::from_secs(self.delay_seconds),
        }
    }

    pub fn delay_mode(&self) -> DelayMode {
        if self.delay_seconds == 0 {
            DelayMode::None
        } else {
            self.delay_mode
        }
    }

    pub fn category(&self) -> TimeControlCategory {
        let estimated = self.initial_seconds + 40 * self.increment_seconds;
        match estimated {
            0..180 => TimeControlCategory::Bullet,
            180..480 => TimeControlCategory::Blitz,
            480..1500 => TimeControlCategory::Rapid,
            _ => TimeControlCategory::Classical,
        }
    }

    pub fn label(&self) -> String {
        let minutes = self.initial_seconds / 60;
        let seconds = self.initial_seconds % 60;
        let base = if seconds == 0 {
            minutes.to_string()
        } else if minutes == 0 {
            format!("{seconds}s")
        } else {
            format!("{minutes}:{seconds:02}")
        };
        let mut label = format!("{base}+{}", self.increment_seconds);
        match self.delay_mode() {
            DelayMode::Simple => label.push_str(&format!(" d{}", self.delay_seconds)),
            DelayMode::Bronstein => label.push_str(&format!(" b{}", self.delay_seconds)),
            DelayMode::None => {}
        }
        label
    }

    pub fn pgn_tag(&self) -> String {
        if self.increment_seconds > 0 {
            format!("{}+{}", self.initial_seconds, self.increment_seconds)
        } else {
            self.initial_seconds.to_string()
        }
    }
}

pub const TIME_CONTROL_PRESETS: [TimeControl; 13] = [
    TimeControl::fischer(60, 0),
    TimeControl::fischer(120, 1),
    TimeControl::fischer(180, 0),
    TimeControl::fischer(180, 2),
    TimeControl::fischer(300, 0),
    TimeControl::fischer(300, 3),
    TimeControl::fischer(600, 0),
    TimeControl::fischer(600, 5),
    TimeControl::fischer(900, 10),
    TimeControl::fischer(1800, 0),
    TimeControl::fischer(1800, 20),
    TimeControl::fischer(3600, 0),
    TimeControl::fischer(5400, 30),
];
