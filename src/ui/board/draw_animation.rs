use shakmaty::Square;

use super::animation::{ease_out_back, ease_out_cubic};
use super::checkmate_animation::ease_in_cubic;

/// Timeline of the draw presentation: both kings' squares dim and show a
/// large "½" with a "Draw" label, which then shrink away and leave a small
/// "½" badge on each king until the next game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawConfig {
    pub enter_ms: f64,
    pub hold_ms: f64,
    pub exit_ms: f64,
}

impl Default for DrawConfig {
    fn default() -> Self {
        Self {
            enter_ms: 280.0,
            hold_ms: 1900.0,
            exit_ms: 380.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawAnimation {
    kings: [Square; 2],
    elapsed_ms: f64,
    config: DrawConfig,
}

impl DrawAnimation {
    pub fn new(kings: [Square; 2], config: DrawConfig) -> Self {
        Self {
            kings,
            elapsed_ms: 0.0,
            config,
        }
    }

    pub fn kings(&self) -> [Square; 2] {
        self.kings
    }

    fn total_ms(&self) -> f64 {
        self.config.enter_ms + self.config.hold_ms + self.config.exit_ms
    }

    pub fn update(&mut self, dt_ms: f64) {
        self.elapsed_ms = (self.elapsed_ms + dt_ms.max(0.0)).min(self.total_ms());
    }

    pub fn is_finished(&self) -> bool {
        self.elapsed_ms >= self.total_ms()
    }

    fn enter(&self) -> f64 {
        (self.elapsed_ms / self.config.enter_ms.max(1.0)).clamp(0.0, 1.0)
    }

    fn exit(&self) -> f64 {
        let start = self.config.enter_ms + self.config.hold_ms;
        ((self.elapsed_ms - start) / self.config.exit_ms.max(1.0)).clamp(0.0, 1.0)
    }

    /// Strength of the large overlay (tint, dimmed king, "½" and label), 0..=1.
    pub fn overlay(&self) -> f64 {
        ease_out_cubic(self.enter()) * (1.0 - ease_in_cubic(self.exit()))
    }

    /// Scale of the large "½" and the "Draw" label: pops in, shrinks away.
    pub fn overlay_scale(&self) -> f64 {
        if self.exit() > 0.0 {
            1.0 - ease_in_cubic(self.exit())
        } else {
            ease_out_back(self.enter())
        }
    }

    /// Scale of the small corner badge that stays after the overlay.
    pub fn badge_scale(&self) -> f64 {
        ease_out_back(self.exit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    fn at(ms: f64) -> DrawAnimation {
        let mut animation = DrawAnimation::new([Square::E1, Square::E8], DrawConfig::default());
        animation.update(ms);
        animation
    }

    #[test]
    fn overlay_pops_in_then_hands_over_to_the_badge() {
        let start = at(0.0);
        assert!(close(start.overlay(), 0.0));
        assert!(close(start.overlay_scale(), 0.0));
        assert!(close(start.badge_scale(), 0.0));
        let held = at(1000.0);
        assert!(close(held.overlay(), 1.0));
        assert!(close(held.overlay_scale(), 1.0));
        assert!(close(held.badge_scale(), 0.0));
        let done = at(10_000.0);
        assert!(done.is_finished());
        assert!(close(done.overlay(), 0.0));
        assert!(close(done.overlay_scale(), 0.0));
        assert!(close(done.badge_scale(), 1.0));
    }
}
