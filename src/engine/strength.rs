use std::time::Duration;

use super::catalog::{EngineDefinition, StrengthProfile};
use super::session::EngineIdentity;
use super::uci::UciOptionKind;
use crate::settings::{Difficulty, StrengthSettings};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrengthPlan {
    pub options: Vec<(String, String)>,
    pub depth: Option<u32>,
    pub nodes: Option<u64>,
    pub movetime_ms: u64,
    pub limited: bool,
    pub description: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ClockState {
    pub white: Duration,
    pub black: Duration,
    pub increment: Duration,
}

fn target_elo(strength: &StrengthSettings) -> Option<u32> {
    match strength.difficulty {
        Difficulty::Beginner => Some(1350),
        Difficulty::Intermediate => Some(1800),
        Difficulty::Advanced => Some(2300),
        Difficulty::Maximum => None,
        Difficulty::Custom => (strength.custom_elo < 3000).then_some(strength.custom_elo),
    }
}

fn movetime(strength: &StrengthSettings) -> u64 {
    match strength.difficulty {
        Difficulty::Beginner => 350,
        Difficulty::Intermediate => 700,
        Difficulty::Advanced => 1200,
        Difficulty::Maximum | Difficulty::Custom => strength.think_time_ms.clamp(100, 600_000),
    }
}

fn level_index(elo: u32) -> usize {
    match elo {
        0..1600 => 0,
        1600..2100 => 1,
        _ => 2,
    }
}

const GENERIC_PROFILE: StrengthProfile = StrengthProfile {
    limited_nodes: [600, 12_000, 250_000],
    limited_depth: [3, 7, 14],
};

pub fn plan_strength(
    definition: Option<&EngineDefinition>,
    identity: &EngineIdentity,
    strength: &StrengthSettings,
) -> StrengthPlan {
    let profile = definition.map(|d| d.strength).unwrap_or(GENERIC_PROFILE);
    let mut plan = StrengthPlan {
        options: Vec::new(),
        depth: None,
        nodes: None,
        movetime_ms: movetime(strength),
        limited: false,
        description: String::new(),
    };
    let elo_option = identity.find_option("UCI_Elo");
    let supports_limit = identity.supports("UCI_LimitStrength") && elo_option.is_some();
    let skill_option = identity.find_option("Skill Level");

    match target_elo(strength) {
        None => {
            if supports_limit {
                plan.options.push(("UCI_LimitStrength".to_owned(), "false".to_owned()));
            }
            if let Some(option) = skill_option
                && let UciOptionKind::Spin { max, .. } = option.kind
            {
                plan.options.push((option.name.clone(), max.to_string()));
            }
            plan.description = "Full strength".to_owned();
        }
        Some(elo) => {
            plan.limited = true;
            if let (true, Some(option)) = (supports_limit, elo_option) {
                let (min, max) = match option.kind {
                    UciOptionKind::Spin { min, max, .. } => (min, max),
                    _ => (1000, 3000),
                };
                let clamped = (elo as i64).clamp(min, max);
                plan.options.push(("UCI_LimitStrength".to_owned(), "true".to_owned()));
                plan.options.push((option.name.clone(), clamped.to_string()));
                plan.description = format!("Limited to {clamped} Elo");
            } else if let Some(option) = skill_option {
                let (min, max) = match option.kind {
                    UciOptionKind::Spin { min, max, .. } => (min, max),
                    _ => (0, 20),
                };
                let skill = ((elo as i64 - 1300) / 85).clamp(min, max);
                plan.options.push((option.name.clone(), skill.to_string()));
                plan.description = format!("Skill level {skill}");
            } else {
                let index = level_index(elo);
                plan.nodes = Some(profile.limited_nodes[index]).filter(|nodes| *nodes > 0);
                plan.depth = Some(profile.limited_depth[index]).filter(|depth| *depth > 0);
                plan.description = match (plan.nodes, plan.depth) {
                    (Some(nodes), _) => format!("Search limited to {nodes} nodes"),
                    (None, Some(depth)) => format!("Search limited to depth {depth}"),
                    _ => "Reduced strength".to_owned(),
                };
            }
        }
    }
    if strength.difficulty == Difficulty::Custom && strength.custom_depth > 0 {
        plan.depth = Some(strength.custom_depth);
    }
    plan
}

pub fn go_command(plan: &StrengthPlan, clock: Option<ClockState>) -> String {
    let mut command = String::from("go");
    match clock {
        Some(state) => {
            command.push_str(&format!(
                " wtime {} btime {} winc {} binc {}",
                state.white.as_millis().max(1),
                state.black.as_millis().max(1),
                state.increment.as_millis(),
                state.increment.as_millis()
            ));
            if plan.limited {
                command.push_str(&format!(" movetime {}", plan.movetime_ms));
            }
        }
        None => command.push_str(&format!(" movetime {}", plan.movetime_ms)),
    }
    if let Some(depth) = plan.depth {
        command.push_str(&format!(" depth {depth}"));
    }
    if let Some(nodes) = plan.nodes {
        command.push_str(&format!(" nodes {nodes}"));
    }
    command
}

pub fn position_command(initial_fen: &str, standard_start: bool, moves: &[String]) -> String {
    let mut command = if standard_start {
        String::from("position startpos")
    } else {
        format!("position fen {initial_fen}")
    };
    if !moves.is_empty() {
        command.push_str(" moves ");
        command.push_str(&moves.join(" "));
    }
    command
}
