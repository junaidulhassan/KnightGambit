use shakmaty::{Piece, Square};

pub fn ease_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_back(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlidingPiece {
    pub piece: Piece,
    pub from: Square,
    pub to: Square,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveAnimation {
    pub start_us: i64,
    pub duration_us: i64,
    pub sliding: Vec<SlidingPiece>,
    pub fading: Vec<(Piece, Square)>,
}

impl MoveAnimation {
    pub fn progress(&self, now_us: i64) -> f64 {
        if self.duration_us <= 0 {
            return 1.0;
        }
        ((now_us - self.start_us) as f64 / self.duration_us as f64).clamp(0.0, 1.0)
    }

    pub fn is_finished(&self, now_us: i64) -> bool {
        self.progress(now_us) >= 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    pub start_us: i64,
    pub duration_us: i64,
}

impl Pulse {
    pub fn new(start_us: i64, duration_ms: i64) -> Self {
        Self {
            start_us,
            duration_us: duration_ms * 1000,
        }
    }

    pub fn progress(&self, now_us: i64) -> f64 {
        if self.duration_us <= 0 {
            return 1.0;
        }
        ((now_us - self.start_us) as f64 / self.duration_us as f64).clamp(0.0, 1.0)
    }

    pub fn is_finished(&self, now_us: i64) -> bool {
        self.progress(now_us) >= 1.0
    }
}
