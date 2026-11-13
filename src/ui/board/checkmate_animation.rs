use std::f64::consts::TAU;

use shakmaty::Square;

pub use super::animation::ease_out_back;

pub fn ease_in_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckmateConfig {
    pub enter_ms: f64,
    pub hold_ms: f64,
    pub exit_ms: f64,
    pub pulse_period_ms: f64,
    pub red_alpha: f64,
    pub red_pulse: f64,
    pub green_alpha: f64,
    pub king_pulse: f64,
    pub persist_highlights_until_new_game: bool,
}

impl CheckmateConfig {
    pub fn total_ms(&self) -> f64 {
        self.enter_ms + self.hold_ms + self.exit_ms
    }
}

impl Default for CheckmateConfig {
    fn default() -> Self {
        Self {
            enter_ms: 250.0,
            hold_ms: 2250.0,
            exit_ms: 400.0,
            pulse_period_ms: 1200.0,
            red_alpha: 0.85,
            red_pulse: 0.05,
            green_alpha: 0.9,
            king_pulse: 0.18,
            persist_highlights_until_new_game: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimPhase {
    Enter,
    Hold,
    Exit,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckmateAnimation {
    loser: Square,
    winner: Square,
    elapsed_ms: f64,
    config: CheckmateConfig,
}

impl CheckmateAnimation {
    pub fn new(loser: Square, winner: Square, config: CheckmateConfig) -> Self {
        Self {
            loser,
            winner,
            elapsed_ms: 0.0,
            config,
        }
    }

    pub fn update(&mut self, dt_ms: f64) {
        self.elapsed_ms = (self.elapsed_ms + dt_ms.max(0.0)).min(self.total_ms());
    }

    pub fn loser(&self) -> Square {
        self.loser
    }

    pub fn winner(&self) -> Square {
        self.winner
    }

    pub fn config(&self) -> &CheckmateConfig {
        &self.config
    }

    fn total_ms(&self) -> f64 {
        self.config.total_ms()
    }

    fn exit_start_ms(&self) -> f64 {
        self.config.enter_ms + self.config.hold_ms
    }

    pub fn phase(&self) -> AnimPhase {
        if self.elapsed_ms < self.config.enter_ms {
            AnimPhase::Enter
        } else if self.elapsed_ms < self.exit_start_ms() {
            AnimPhase::Hold
        } else if self.elapsed_ms < self.total_ms() {
            AnimPhase::Exit
        } else {
            AnimPhase::Done
        }
    }

    pub fn is_finished(&self) -> bool {
        self.phase() == AnimPhase::Done
    }

    fn enter_progress(&self) -> f64 {
        (self.elapsed_ms / self.config.enter_ms.max(1.0)).clamp(0.0, 1.0)
    }

    fn exit_progress(&self) -> f64 {
        ((self.elapsed_ms - self.exit_start_ms()) / self.config.exit_ms.max(1.0)).clamp(0.0, 1.0)
    }

    fn pulse_angle(&self) -> f64 {
        let held = (self.elapsed_ms - self.config.enter_ms).clamp(0.0, self.config.hold_ms);
        TAU * held / self.config.pulse_period_ms.max(1.0)
    }

    fn held_red_alpha(&self) -> f64 {
        self.config.red_alpha + self.config.red_pulse * self.pulse_angle().sin()
    }

    pub fn highlight_fade(&self) -> f64 {
        match self.phase() {
            AnimPhase::Enter => self.enter_progress(),
            AnimPhase::Hold => 1.0,
            _ if self.config.persist_highlights_until_new_game => 1.0,
            AnimPhase::Exit => 1.0 - ease_in_cubic(self.exit_progress()),
            AnimPhase::Done => 0.0,
        }
    }

    pub fn red_alpha(&self) -> f64 {
        match self.phase() {
            AnimPhase::Enter => self.config.red_alpha * self.enter_progress(),
            AnimPhase::Hold => self.held_red_alpha(),
            _ if self.config.persist_highlights_until_new_game => {
                lerp(self.held_red_alpha(), self.config.red_alpha, self.exit_progress())
            }
            _ => self.held_red_alpha() * self.highlight_fade(),
        }
    }

    pub fn green_alpha(&self) -> f64 {
        self.config.green_alpha * self.highlight_fade()
    }

    pub fn badge_scale(&self) -> f64 {
        match self.phase() {
            AnimPhase::Enter => ease_out_back(self.enter_progress()),
            _ => 1.0,
        }
    }

    pub fn badge_alpha(&self) -> f64 {
        match self.phase() {
            AnimPhase::Enter => 1.0,
            _ => self.highlight_fade(),
        }
    }

    pub fn king_scale(&self) -> f64 {
        match self.phase() {
            AnimPhase::Enter => 1.0 + self.config.king_pulse * (self.enter_progress() * TAU / 2.0).sin(),
            _ => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: f64, persist: bool) -> CheckmateAnimation {
        let config = CheckmateConfig {
            persist_highlights_until_new_game: persist,
            ..CheckmateConfig::default()
        };
        let mut animation = CheckmateAnimation::new(Square::E8, Square::G1, config);
        let mut remaining = ms;
        while remaining > 0.0 {
            let step = remaining.min(16.0);
            animation.update(step);
            remaining -= step;
        }
        animation
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    #[test]
    fn easing_endpoints_and_shape() {
        for ease in [ease_in_cubic, ease_out_back] {
            assert!(close(ease(0.0), 0.0));
            assert!(close(ease(1.0), 1.0));
            assert!(close(ease(-1.0), 0.0));
            assert!(close(ease(2.0), 1.0));
        }
        assert!(close(ease_in_cubic(0.5), 0.125));
        assert!(ease_out_back(0.7) > 1.0);
    }

    #[test]
    fn timeline_at_start() {
        let animation = at(0.0, false);
        assert_eq!(animation.phase(), AnimPhase::Enter);
        assert!(close(animation.red_alpha(), 0.0));
        assert!(close(animation.green_alpha(), 0.0));
        assert!(close(animation.badge_scale(), 0.0));
        assert!(close(animation.king_scale(), 1.0));
        assert!(!animation.is_finished());
    }

    #[test]
    fn timeline_mid_enter() {
        let animation = at(125.0, false);
        assert_eq!(animation.phase(), AnimPhase::Enter);
        assert!(close(animation.red_alpha(), 0.425));
        assert!(close(animation.king_scale(), 1.18));
        assert!(animation.badge_scale() > 0.5);
    }

    #[test]
    fn timeline_hold() {
        let animation = at(1000.0, false);
        assert_eq!(animation.phase(), AnimPhase::Hold);
        assert!(close(animation.badge_scale(), 1.0));
        assert!((0.80 - 1e-9..=0.90 + 1e-9).contains(&animation.red_alpha()));
        assert!(close(animation.green_alpha(), 0.9));
    }

    #[test]
    fn timeline_exit() {
        let animation = at(2700.0, false);
        assert_eq!(animation.phase(), AnimPhase::Exit);
        assert!(close(animation.king_scale(), 1.0));
        assert!(close(animation.highlight_fade(), 0.875));
        let kept = at(2700.0, true);
        assert!(close(kept.highlight_fade(), 1.0));
    }

    #[test]
    fn timeline_done() {
        let animation = at(3000.0, false);
        assert_eq!(animation.phase(), AnimPhase::Done);
        assert!(animation.is_finished());
        assert!(close(animation.red_alpha(), 0.0));
        assert!(close(animation.green_alpha(), 0.0));
        let kept = at(3000.0, true);
        assert!(kept.is_finished());
        assert!(close(kept.red_alpha(), 0.85));
        assert!(close(kept.green_alpha(), 0.9));
        assert!(close(kept.badge_alpha(), 1.0));
    }
}
