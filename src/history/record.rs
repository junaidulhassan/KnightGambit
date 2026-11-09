use shakmaty::{Color, Position, Role};

use crate::chess::{Game, GameResult, PgnHeaders, Termination, write_pgn};
use crate::review::{GameReview, opening_of};

use super::model::{GameRecord, GameReviewSummary, MoveReviewRow, Outcome};

/// Who the user played and under which conditions.
#[derive(Debug, Clone)]
pub struct GameContext {
    pub uid: String,
    pub user_color: Color,
    pub opponent_name: String,
    pub opponent_level: String,
    pub opponent_elo: Option<i64>,
    pub duration_secs: i64,
    pub played_at: i64,
}

pub fn termination_label(termination: Termination) -> &'static str {
    match termination {
        Termination::Checkmate => "Checkmate",
        Termination::Stalemate => "Stalemate",
        Termination::InsufficientMaterial => "Insufficient material",
        Termination::FivefoldRepetition => "Fivefold repetition",
        Termination::SeventyFiveMoveRule => "Seventy-five-move rule",
        Termination::ThreefoldRepetitionClaimed => "Threefold repetition",
        Termination::FiftyMoveRuleClaimed => "Fifty-move rule",
        Termination::DrawAgreed => "Draw agreed",
        Termination::Resignation => "Resignation",
        Termination::Timeout => "Time out",
        Termination::TimeoutVsInsufficientMaterial => "Time out (draw)",
        Termination::Abandoned => "Abandoned",
    }
}

pub fn outcome_for(result: &GameResult, user: Color) -> Outcome {
    match result.winner {
        Some(winner) if winner == user => Outcome::Win,
        Some(_) => Outcome::Loss,
        None => Outcome::Draw,
    }
}

/// First ply at which no more than six queens, rooks, bishops and knights
/// remain on the board (the usual middlegame/endgame boundary).
pub fn endgame_ply(game: &Game) -> Option<i64> {
    (0..=game.ply_count()).find_map(|ply| {
        let board = game.position_at(ply).board();
        let pieces: usize = [Role::Queen, Role::Rook, Role::Bishop, Role::Knight]
            .into_iter()
            .map(|role| board.by_role(role).count())
            .sum();
        (pieces <= 6).then(|| (ply as i64).max(1))
    })
}

/// Builds the record for a finished game, or `None` if it should not be stored
/// (no result, abandoned, or no moves).
pub fn build_record(game: &Game, headers: &PgnHeaders, context: &GameContext) -> Option<GameRecord> {
    let result = game.result()?;
    if result.termination == Termination::Abandoned || game.ply_count() == 0 {
        return None;
    }
    let opening = opening_of(game);
    let mut stored_headers = headers.clone();
    if let Some((name, eco)) = &opening {
        stored_headers.extra.push(("Opening".to_owned(), name.clone()));
        if let Some(eco) = eco {
            stored_headers.extra.push(("ECO".to_owned(), eco.clone()));
        }
    }
    stored_headers.extra.push(("KnightGambitOpponent".to_owned(), context.opponent_level.clone()));
    Some(GameRecord {
        id: 0,
        uid: context.uid.clone(),
        played_at: context.played_at,
        duration_secs: context.duration_secs,
        user_color: context.user_color,
        opponent_name: context.opponent_name.clone(),
        opponent_level: context.opponent_level.clone(),
        opponent_elo: context.opponent_elo,
        outcome: outcome_for(&result, context.user_color),
        score: result.score_text().to_owned(),
        termination: termination_label(result.termination).to_owned(),
        opening_name: opening.as_ref().map(|(name, _)| name.clone()),
        eco: opening.and_then(|(_, eco)| eco),
        start_fen: if game.starts_from_standard_position() { String::new() } else { game.initial_fen().to_owned() },
        pgn: write_pgn(game, &stored_headers),
        moves_uci: game.uci_moves().join(" "),
        ply_count: game.ply_count() as i64,
        time_control: headers.time_control.clone(),
        endgame_ply: endgame_ply(game),
        white_name: headers.white.clone(),
        black_name: headers.black.clone(),
        analyzed: false,
    })
}

/// Converts an engine review into rows ready to store.
pub fn review_rows(game_id: i64, review: &GameReview) -> Vec<MoveReviewRow> {
    review
        .moves
        .iter()
        .map(|item| MoveReviewRow {
            game_id,
            ply: item.ply as i64,
            mover: item.mover,
            class: item.class.label().to_owned(),
            loss: item.loss,
            accuracy: item.accuracy,
            san: item.san.clone(),
            best_san: item.best_san.clone(),
        })
        .collect()
}

pub fn review_summary(review: &GameReview, engine: &str, movetime_ms: u64, reviewed_at: i64) -> GameReviewSummary {
    GameReviewSummary {
        engine: engine.to_owned(),
        reviewed_at,
        movetime_ms: movetime_ms as i64,
        white_accuracy: review.accuracy[0],
        black_accuracy: review.accuracy[1],
    }
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// A fresh random identifier for one play session.
pub fn new_session_uid() -> String {
    format!("{:032x}", rand::random::<u128>())
}
