use shakmaty::Color;

use super::model::{GameRecord, Outcome, color_label};
use crate::review::opening_family;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameFilter {
    pub search: String,
    pub outcome: Option<Outcome>,
    pub color: Option<Color>,
    /// Difficulty label ("Beginner", ...).
    pub opponent: Option<String>,
    /// Opening family ("Sicilian Defense").
    pub opening: Option<String>,
    /// Inclusive unix-time bounds on the end time of the game.
    pub from: Option<i64>,
    pub to: Option<i64>,
}

impl GameFilter {
    pub fn is_active(&self) -> bool {
        *self != GameFilter::default()
    }

    pub fn matches(&self, game: &GameRecord) -> bool {
        if self.outcome.is_some_and(|outcome| outcome != game.outcome)
            || self.color.is_some_and(|color| color != game.user_color)
            || self.from.is_some_and(|from| game.played_at < from)
            || self.to.is_some_and(|to| game.played_at > to)
        {
            return false;
        }
        if let Some(level) = &self.opponent
            && &game.opponent_level != level
        {
            return false;
        }
        if let Some(opening) = &self.opening
            && game.opening_name.as_deref().map(opening_family) != Some(opening.as_str())
        {
            return false;
        }
        let needle = self.search.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let haystack = [
            game.opponent_name.as_str(),
            game.opponent_level.as_str(),
            game.opening_name.as_deref().unwrap_or(""),
            game.eco.as_deref().unwrap_or(""),
            game.outcome.label(),
            color_label(game.user_color),
            game.termination.as_str(),
            game.white_name.as_str(),
            game.black_name.as_str(),
        ]
        .join("\n")
        .to_lowercase();
        needle.split_whitespace().all(|word| haystack.contains(word))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Date,
    Opponent,
    Result,
    Opening,
    Moves,
    Duration,
}

impl SortKey {
    pub const ALL: [SortKey; 6] = [
        SortKey::Date,
        SortKey::Opponent,
        SortKey::Result,
        SortKey::Opening,
        SortKey::Moves,
        SortKey::Duration,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Date => "Date",
            SortKey::Opponent => "Opponent",
            SortKey::Result => "Result",
            SortKey::Opening => "Opening",
            SortKey::Moves => "Number of moves",
            SortKey::Duration => "Duration",
        }
    }
}

fn result_rank(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Win => 0,
        Outcome::Draw => 1,
        Outcome::Loss => 2,
    }
}

/// Filters and sorts games. Ties fall back to newest first.
pub fn select(games: &[GameRecord], filter: &GameFilter, key: SortKey, descending: bool) -> Vec<GameRecord> {
    let mut selected: Vec<GameRecord> = games.iter().filter(|game| filter.matches(game)).cloned().collect();
    selected.sort_by(|a, b| {
        let order = match key {
            SortKey::Date => a.played_at.cmp(&b.played_at),
            SortKey::Opponent => a
                .opponent_level
                .to_lowercase()
                .cmp(&b.opponent_level.to_lowercase())
                .then(a.opponent_name.to_lowercase().cmp(&b.opponent_name.to_lowercase())),
            SortKey::Result => result_rank(a.outcome).cmp(&result_rank(b.outcome)),
            SortKey::Opening => a
                .opening_name
                .clone()
                .unwrap_or_default()
                .to_lowercase()
                .cmp(&b.opening_name.clone().unwrap_or_default().to_lowercase()),
            SortKey::Moves => a.ply_count.cmp(&b.ply_count),
            SortKey::Duration => a.duration_secs.cmp(&b.duration_secs),
        };
        let order = if descending { order.reverse() } else { order };
        order.then(b.played_at.cmp(&a.played_at)).then(b.id.cmp(&a.id))
    });
    selected
}

/// Distinct opening families present in `games`, alphabetical.
pub fn opening_families(games: &[GameRecord]) -> Vec<String> {
    let mut names: Vec<String> = games
        .iter()
        .filter_map(|game| game.opening_name.as_deref().map(|name| opening_family(name).to_owned()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Distinct opponent levels present in `games`, in difficulty order.
pub fn opponent_levels(games: &[GameRecord]) -> Vec<String> {
    let mut levels: Vec<String> = games.iter().map(|game| game.opponent_level.clone()).collect();
    levels.sort();
    levels.dedup();
    let rank = |label: &str| match label {
        "Beginner" => 0,
        "Intermediate" => 1,
        "Advanced" => 2,
        "Maximum Strength" => 3,
        "Custom" => 4,
        _ => 5,
    };
    levels.sort_by_key(|label| rank(label));
    levels
}
