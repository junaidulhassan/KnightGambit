mod time_control;

pub use time_control::{DelayMode, TimeControl, TimeControlCategory, TIME_CONTROL_PRESETS};

use std::time::{Duration, Instant};

use shakmaty::Color;

#[derive(Debug, Clone)]
pub struct ChessClock {
    control: TimeControl,
    remaining: [Duration; 2],
    running: Option<RunningTurn>,
    paused_side: Option<Color>,
    flagged: Option<Color>,
}

#[derive(Debug, Clone, Copy)]
struct RunningTurn {
    side: Color,
    started: Instant,
}

fn index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

impl ChessClock {
    pub fn new(control: TimeControl) -> Self {
        let initial = control.initial();
        Self {
            control,
            remaining: [initial, initial],
            running: None,
            paused_side: None,
            flagged: None,
        }
    }

    pub fn control(&self) -> &TimeControl {
        &self.control
    }

    pub fn running_side(&self) -> Option<Color> {
        self.running.map(|turn| turn.side)
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn flagged(&self) -> Option<Color> {
        self.flagged
    }

    pub fn start(&mut self, side: Color, now: Instant) {
        if self.flagged.is_some() {
            return;
        }
        self.paused_side = None;
        self.running = Some(RunningTurn { side, started: now });
    }

    fn charge(&self, side: Color, elapsed: Duration) -> Duration {
        let delay = self.control.delay();
        let remaining = self.remaining[index(side)];
        match self.control.delay_mode() {
            DelayMode::Simple => remaining.saturating_sub(elapsed.saturating_sub(delay)),
            DelayMode::Bronstein | DelayMode::None => remaining.saturating_sub(elapsed),
        }
    }

    pub fn remaining(&self, side: Color, now: Instant) -> Duration {
        match self.running {
            Some(turn) if turn.side == side => {
                self.charge(side, now.saturating_duration_since(turn.started))
            }
            _ => self.remaining[index(side)],
        }
    }

    pub fn delay_remaining(&self, now: Instant) -> Option<Duration> {
        let turn = self.running?;
        if self.control.delay_mode() != DelayMode::Simple || self.control.delay().is_zero() {
            return None;
        }
        let elapsed = now.saturating_duration_since(turn.started);
        let left = self.control.delay().saturating_sub(elapsed);
        (!left.is_zero()).then_some(left)
    }

    pub fn check_flag(&mut self, now: Instant) -> Option<Color> {
        if self.flagged.is_some() {
            return self.flagged;
        }
        let turn = self.running?;
        if self.remaining(turn.side, now).is_zero() {
            self.remaining[index(turn.side)] = Duration::ZERO;
            self.running = None;
            self.flagged = Some(turn.side);
        }
        self.flagged
    }

    pub fn press(&mut self, now: Instant) -> Option<Color> {
        if let Some(flagged) = self.check_flag(now) {
            return Some(flagged);
        }
        let turn = self.running?;
        let elapsed = now.saturating_duration_since(turn.started);
        let mut left = self.charge(turn.side, elapsed);
        if self.control.delay_mode() == DelayMode::Bronstein {
            left += elapsed.min(self.control.delay());
        }
        left += self.control.increment();
        self.remaining[index(turn.side)] = left;
        self.running = Some(RunningTurn {
            side: !turn.side,
            started: now,
        });
        None
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(turn) = self.running.take() {
            let elapsed = now.saturating_duration_since(turn.started);
            self.remaining[index(turn.side)] = self.charge(turn.side, elapsed);
            self.paused_side = Some(turn.side);
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if let Some(side) = self.paused_side.take() {
            self.start(side, now);
        }
    }

    pub fn stop(&mut self, now: Instant) {
        self.pause(now);
        self.paused_side = None;
    }

    pub fn set_remaining(&mut self, side: Color, remaining: Duration) {
        self.remaining[index(side)] = remaining;
    }

    pub fn snapshot(&self, now: Instant) -> [Duration; 2] {
        [self.remaining(Color::White, now), self.remaining(Color::Black, now)]
    }
}

pub fn format_clock(duration: Duration) -> String {
    let total_millis = duration.as_millis();
    let total_seconds = total_millis / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else if total_seconds < 20 {
        let tenths = (total_millis % 1000) / 100;
        format!("{minutes}:{seconds:02}.{tenths}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}
