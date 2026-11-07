use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UciOptionKind {
    Check { default: bool },
    Spin { default: i64, min: i64, max: i64 },
    Combo { default: String, choices: Vec<String> },
    Button,
    Text { default: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UciOption {
    pub name: String,
    pub kind: UciOptionKind,
}

impl UciOption {
    pub fn default_value(&self) -> Option<String> {
        match &self.kind {
            UciOptionKind::Check { default } => Some(default.to_string()),
            UciOptionKind::Spin { default, .. } => Some(default.to_string()),
            UciOptionKind::Combo { default, .. } => Some(default.clone()),
            UciOptionKind::Text { default } => Some(default.clone()),
            UciOptionKind::Button => None,
        }
    }

    pub fn is_managed_by_app(&self) -> bool {
        let lowered = self.name.to_ascii_lowercase();
        lowered.starts_with("uci_")
            || matches!(
                lowered.as_str(),
                "ponder" | "multipv" | "threads" | "hash" | "skill level"
            )
            || matches!(self.kind, UciOptionKind::Button)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    Centipawns(i32),
    Mate(i32),
}

impl Score {
    pub fn negate(self) -> Score {
        match self {
            Score::Centipawns(value) => Score::Centipawns(-value),
            Score::Mate(moves) => Score::Mate(-moves),
        }
    }

    pub fn as_centipawns(self) -> i32 {
        match self {
            Score::Centipawns(value) => value,
            Score::Mate(moves) if moves > 0 => 100_000 - moves,
            Score::Mate(moves) => -100_000 - moves,
        }
    }

    pub fn display(self) -> String {
        match self {
            Score::Centipawns(value) => format!("{:+.2}", value as f64 / 100.0),
            Score::Mate(moves) if moves > 0 => format!("M{moves}"),
            Score::Mate(moves) => format!("-M{}", moves.abs()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchInfo {
    pub depth: Option<u32>,
    pub seldepth: Option<u32>,
    pub multipv: Option<u32>,
    pub score: Option<Score>,
    pub is_bound: bool,
    pub nodes: Option<u64>,
    pub nps: Option<u64>,
    pub time_ms: Option<u64>,
    pub pv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineMessage {
    IdName(String),
    IdAuthor(String),
    Option(UciOption),
    UciOk,
    ReadyOk,
    Info(SearchInfo),
    BestMove { best: String, ponder: Option<String> },
    Other(String),
}

pub fn parse_line(line: &str) -> EngineMessage {
    let trimmed = line.trim();
    let mut tokens = trimmed.split_whitespace();
    match tokens.next() {
        Some("uciok") => EngineMessage::UciOk,
        Some("readyok") => EngineMessage::ReadyOk,
        Some("id") => match tokens.next() {
            Some("name") => EngineMessage::IdName(rest_after(trimmed, 2)),
            Some("author") => EngineMessage::IdAuthor(rest_after(trimmed, 2)),
            _ => EngineMessage::Other(trimmed.to_owned()),
        },
        Some("option") => parse_option(trimmed)
            .map(EngineMessage::Option)
            .unwrap_or_else(|| EngineMessage::Other(trimmed.to_owned())),
        Some("info") => parse_info(trimmed)
            .map(EngineMessage::Info)
            .unwrap_or_else(|| EngineMessage::Other(trimmed.to_owned())),
        Some("bestmove") => match tokens.next() {
            Some(best) => {
                let ponder = match (tokens.next(), tokens.next()) {
                    (Some("ponder"), Some(mv)) => Some(mv.to_owned()),
                    _ => None,
                };
                EngineMessage::BestMove {
                    best: best.to_owned(),
                    ponder,
                }
            }
            None => EngineMessage::Other(trimmed.to_owned()),
        },
        _ => EngineMessage::Other(trimmed.to_owned()),
    }
}

fn rest_after(line: &str, skip: usize) -> String {
    line.split_whitespace().skip(skip).collect::<Vec<_>>().join(" ")
}

const OPTION_KEYWORDS: [&str; 6] = ["name", "type", "default", "min", "max", "var"];

pub fn parse_option(line: &str) -> Option<UciOption> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.first() != Some(&"option") {
        return None;
    }
    let mut name = Vec::new();
    let mut kind = None;
    let mut default: Option<Vec<&str>> = None;
    let mut min = None;
    let mut max = None;
    let mut choices: Vec<String> = Vec::new();
    let mut index = 1;
    while index < tokens.len() {
        let keyword = tokens[index];
        index += 1;
        let start = index;
        let is_name = keyword == "name";
        while index < tokens.len() {
            let candidate = tokens[index];
            let terminates = if is_name {
                candidate == "type"
            } else {
                OPTION_KEYWORDS.contains(&candidate)
            };
            if terminates {
                break;
            }
            index += 1;
        }
        let value = &tokens[start..index];
        match keyword {
            "name" => name.extend_from_slice(value),
            "type" => kind = value.first().copied(),
            "default" => default = Some(value.to_vec()),
            "min" => min = value.first().and_then(|v| v.parse::<i64>().ok()),
            "max" => max = value.first().and_then(|v| v.parse::<i64>().ok()),
            "var" => choices.push(value.join(" ")),
            _ => {}
        }
    }
    if name.is_empty() {
        return None;
    }
    let default_text = default
        .map(|words| words.join(" "))
        .filter(|text| text != "<empty>")
        .unwrap_or_default();
    let kind = match kind? {
        "check" => UciOptionKind::Check {
            default: default_text.eq_ignore_ascii_case("true"),
        },
        "spin" => {
            let min = min.unwrap_or(i64::MIN);
            let max = max.unwrap_or(i64::MAX);
            let default = default_text.parse::<i64>().unwrap_or(min.max(0)).clamp(min, max);
            UciOptionKind::Spin { default, min, max }
        }
        "combo" => UciOptionKind::Combo {
            default: default_text,
            choices,
        },
        "button" => UciOptionKind::Button,
        "string" => UciOptionKind::Text {
            default: default_text,
        },
        _ => return None,
    };
    Some(UciOption {
        name: name.join(" "),
        kind,
    })
}

pub fn parse_info(line: &str) -> Option<SearchInfo> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.first() != Some(&"info") {
        return None;
    }
    let mut info = SearchInfo::default();
    let mut index = 1;
    let mut meaningful = false;
    while index < tokens.len() {
        let keyword = tokens[index];
        index += 1;
        let next_number = |position: usize| tokens.get(position).and_then(|t| t.parse::<u64>().ok());
        match keyword {
            "depth" => {
                info.depth = next_number(index).map(|v| v as u32);
                index += 1;
                meaningful = true;
            }
            "seldepth" => {
                info.seldepth = next_number(index).map(|v| v as u32);
                index += 1;
            }
            "multipv" => {
                info.multipv = next_number(index).map(|v| v as u32);
                index += 1;
            }
            "nodes" => {
                info.nodes = next_number(index);
                index += 1;
            }
            "nps" => {
                info.nps = next_number(index);
                index += 1;
            }
            "time" => {
                info.time_ms = next_number(index);
                index += 1;
            }
            "score" => {
                let kind = tokens.get(index).copied();
                let value = tokens.get(index + 1).and_then(|t| t.parse::<i32>().ok());
                index += 2;
                info.score = match (kind, value) {
                    (Some("cp"), Some(v)) => Some(Score::Centipawns(v)),
                    (Some("mate"), Some(v)) => Some(Score::Mate(v)),
                    _ => None,
                };
                while let Some(&bound) = tokens.get(index) {
                    if bound == "lowerbound" || bound == "upperbound" {
                        info.is_bound = true;
                        index += 1;
                    } else {
                        break;
                    }
                }
                meaningful = true;
            }
            "pv" => {
                info.pv = tokens[index..].iter().map(|t| (*t).to_owned()).collect();
                index = tokens.len();
                meaningful = true;
            }
            "string" => {
                index = tokens.len();
            }
            "currmove" | "currmovenumber" | "hashfull" | "tbhits" | "cpuload" | "sbhits" => {
                index += 1;
            }
            _ => {}
        }
    }
    meaningful.then_some(info)
}
