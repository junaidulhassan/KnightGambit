//! Live, per-move feedback while playing or analysing: a cache of engine
//! evaluations keyed by position, and the glue that turns two cached
//! evaluations into a move classification identical to the full review.

use std::collections::HashMap;

use shakmaty::fen::Epd;
use shakmaty::san::San;
use shakmaty::uci::UciMove;
use shakmaty::{Chess, EnPassantMode, Position};

use super::{MoveClass, MoveInput, PositionEval, classify_move, opening_book, opponent_gave_chance};
use crate::chess::Game;
use crate::engine::uci::Score;

/// Depth at which the engine's answer is trusted for a classification.
pub const READY_DEPTH: u32 = 14;
/// Shallowest evaluation still accepted for the position before a move.
pub const MIN_BEFORE_DEPTH: u32 = 8;
/// After this long a shallower result is accepted rather than waiting.
pub const PATIENCE_MS: u128 = 1500;

/// Whether a search that reached `depth` after `elapsed_ms` is good enough.
pub fn is_ready(depth: u32, elapsed_ms: u128) -> bool {
    depth >= READY_DEPTH || (depth >= MIN_BEFORE_DEPTH && elapsed_ms >= PATIENCE_MS)
}

/// Identifies a position independent of the move counters.
pub fn position_key(position: &Chess) -> String {
    Epd::from_position(position, EnPassantMode::Legal).to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedEval {
    pub eval: PositionEval,
    pub depth: u32,
}

/// Evaluations remembered per position. A deeper result replaces a shallower one.
#[derive(Debug, Default)]
pub struct EvalCache {
    map: HashMap<String, CachedEval>,
}

impl EvalCache {
    pub fn insert(&mut self, key: String, eval: PositionEval, depth: u32) {
        match self.map.get(&key) {
            Some(existing) if existing.depth >= depth => {}
            _ => {
                self.map.insert(key, CachedEval { eval, depth });
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&CachedEval> {
        self.map.get(key)
    }

    /// An evaluation that is deep enough to judge a move against.
    pub fn usable(&self, key: &str) -> Option<&PositionEval> {
        self.map
            .get(key)
            .filter(|cached| cached.depth >= MIN_BEFORE_DEPTH)
            .map(|cached| &cached.eval)
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Feedback on one move, shown on the board, in the move list and in the
/// feedback card.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveFeedback {
    pub class: MoveClass,
    pub san: String,
    pub best_uci: Option<String>,
    pub best_san: Option<String>,
    pub played_best: bool,
    pub loss: f64,
    pub win_before: f64,
    pub win_after: f64,
    /// Evaluation after the move from White's point of view.
    pub score_after_white: Score,
    /// Evaluation of the best alternative from White's point of view.
    pub score_best_white: Score,
}

/// Win share of White in 0..=1 for a score from White's point of view,
/// as used by the evaluation bar and the analysis panel.
pub fn white_share(score_white: Score) -> f64 {
    match score_white {
        Score::Mate(moves) if moves > 0 => 1.0,
        Score::Mate(_) => 0.0,
        Score::Centipawns(cp) => 1.0 / (1.0 + (-0.004 * f64::from(cp)).exp()),
    }
}

/// Whether the first `ply` moves of `game` are all inside the opening book.
fn in_book_through(game: &Game, ply: usize) -> bool {
    if !game.starts_from_standard_position() {
        return false;
    }
    let book = opening_book();
    (1..=ply).all(|index| book.contains(game.position_at(index)))
}

/// Classifies the move that produced `ply` (1-based) of `game`. `before` and
/// `after` are the engine evaluations of the positions before and after it;
/// `earlier` holds the classes of the moves played so far, where known
/// (`earlier[0]` is the first move of the game).
pub fn feedback_for_move(
    game: &Game,
    ply: usize,
    before: &PositionEval,
    after: &PositionEval,
    earlier: &[Option<MoveClass>],
) -> Option<MoveFeedback> {
    let index = ply.checked_sub(1)?;
    let played = game.moves().get(index)?;
    let before_position = game.position_at(index);
    let after_position = game.position_at(ply);
    let verdict = classify_move(&MoveInput {
        before_position,
        after_position,
        played,
        before,
        after,
        in_book: in_book_through(game, ply),
        opponent_erred: opponent_gave_chance(|at| earlier.get(at).copied().flatten(), index),
        previous_to: index.checked_sub(1).map(|at| game.moves()[at].mv.to()),
    });
    let mover_is_white = before_position.turn() == shakmaty::Color::White;
    let to_white = |score: Score| if mover_is_white { score } else { score.negate() };
    let best_san = before.best.as_deref().and_then(|uci| {
        let mv = uci.parse::<UciMove>().ok()?.to_move(before_position).ok()?;
        Some(San::from_move(before_position, mv).to_string())
    });
    Some(MoveFeedback {
        class: verdict.class,
        san: played.san.clone(),
        best_uci: before.best.clone(),
        best_san,
        played_best: verdict.played_best,
        loss: verdict.loss,
        win_before: verdict.win_before,
        win_after: verdict.win_after,
        score_after_white: to_white(after.score.negate()),
        score_best_white: to_white(before.score),
    })
}

/// Short label for the evaluation bar: pawns with one decimal, or `M3`.
pub fn bar_label(score_white: Score) -> String {
    match score_white {
        Score::Mate(moves) => format!("M{}", moves.abs()),
        Score::Centipawns(cp) => format!("{:.1}", f64::from(cp.abs()) / 100.0),
    }
}
