//! Local game history: a SQLite database of completed games and their engine
//! reviews, plus the analytics computed from it.

mod db;
mod filter;
mod model;
mod record;
pub mod stats;

pub use db::{HistoryError, HistoryStore, OpenReport, SCHEMA_VERSION};
pub use filter::{GameFilter, SortKey, opening_families, opponent_levels, select};
pub use model::{
    GameRecord, GameReviewSummary, MoveReviewRow, Outcome, StoredReview, color_label,
};
pub use record::{
    GameContext, build_record, endgame_ply, new_session_uid, outcome_for, review_rows, review_summary,
    termination_label, unix_now,
};
pub use stats::{Stats, StatsInput};

/// Loads everything needed and computes the statistics. Runs on a worker thread.
pub fn compute_stats(store: &HistoryStore) -> Result<Stats, HistoryError> {
    let games = store.list_games()?;
    let moves = store.load_all_reviews()?;
    let summaries = store.list_review_summaries()?;
    Ok(stats::compute(&StatsInput {
        games: &games,
        moves: &moves,
        summaries: &summaries,
    }))
}
