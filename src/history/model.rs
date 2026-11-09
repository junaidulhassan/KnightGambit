use serde::{Deserialize, Serialize};
use shakmaty::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Outcome {
    Win,
    Loss,
    Draw,
}

impl Outcome {
    pub const ALL: [Outcome; 3] = [Outcome::Win, Outcome::Loss, Outcome::Draw];

    pub fn as_db(self) -> &'static str {
        match self {
            Outcome::Win => "win",
            Outcome::Loss => "loss",
            Outcome::Draw => "draw",
        }
    }

    pub fn from_db(text: &str) -> Option<Self> {
        match text {
            "win" => Some(Outcome::Win),
            "loss" => Some(Outcome::Loss),
            "draw" => Some(Outcome::Draw),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Outcome::Win => "Win",
            Outcome::Loss => "Loss",
            Outcome::Draw => "Draw",
        }
    }

    pub fn points(self) -> f64 {
        match self {
            Outcome::Win => 1.0,
            Outcome::Draw => 0.5,
            Outcome::Loss => 0.0,
        }
    }
}

pub fn color_as_db(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Black => "black",
    }
}

pub fn color_from_db(text: &str) -> Color {
    if text == "black" { Color::Black } else { Color::White }
}

pub fn color_label(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

/// One completed game. `id` is 0 until the game has been stored.
/// `pgn` and `moves_uci` are empty in list summaries (see `GameStore::list_games`).
#[derive(Debug, Clone, PartialEq)]
pub struct GameRecord {
    pub id: i64,
    /// Random token identifying the play session; makes recording idempotent.
    pub uid: String,
    /// Unix time (seconds) at which the game ended.
    pub played_at: i64,
    pub duration_secs: i64,
    pub user_color: Color,
    pub opponent_name: String,
    /// Difficulty label, e.g. "Beginner".
    pub opponent_level: String,
    pub opponent_elo: Option<i64>,
    pub outcome: Outcome,
    /// PGN result token: "1-0", "0-1" or "1/2-1/2".
    pub score: String,
    /// Human-readable ending: "Checkmate", "Resignation", "Time out", ...
    pub termination: String,
    pub opening_name: Option<String>,
    pub eco: Option<String>,
    /// Empty for the standard starting position.
    pub start_fen: String,
    pub pgn: String,
    pub moves_uci: String,
    pub ply_count: i64,
    pub time_control: Option<String>,
    /// First ply at which the game reached an endgame, if it did.
    pub endgame_ply: Option<i64>,
    pub white_name: String,
    pub black_name: String,
    pub analyzed: bool,
}

/// One engine-classified move of a stored game.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveReviewRow {
    pub game_id: i64,
    pub ply: i64,
    pub mover: Color,
    pub class: String,
    pub loss: f64,
    pub accuracy: f64,
    pub san: String,
    pub best_san: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameReviewSummary {
    pub engine: String,
    pub reviewed_at: i64,
    pub movetime_ms: i64,
    pub white_accuracy: f64,
    pub black_accuracy: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredReview {
    pub summary: GameReviewSummary,
    pub moves: Vec<MoveReviewRow>,
}
