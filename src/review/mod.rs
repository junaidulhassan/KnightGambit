use serde::{Deserialize, Serialize};
use shakmaty::san::San;
use shakmaty::uci::UciMove;
use shakmaty::{Chess, Color, Move, Position, Role};

use crate::chess::Game;
use crate::engine::uci::Score;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MoveClass {
    Brilliant,
    Great,
    Best,
    Excellent,
    Good,
    Forced,
    Inaccuracy,
    Mistake,
    Miss,
    Blunder,
}

pub const CLASS_COUNT: usize = 10;

impl MoveClass {
    pub const ALL: [MoveClass; CLASS_COUNT] = [
        MoveClass::Brilliant,
        MoveClass::Great,
        MoveClass::Best,
        MoveClass::Excellent,
        MoveClass::Good,
        MoveClass::Forced,
        MoveClass::Inaccuracy,
        MoveClass::Mistake,
        MoveClass::Miss,
        MoveClass::Blunder,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MoveClass::Brilliant => "Brilliant",
            MoveClass::Great => "Great",
            MoveClass::Best => "Best",
            MoveClass::Excellent => "Excellent",
            MoveClass::Good => "Good",
            MoveClass::Forced => "Forced",
            MoveClass::Miss => "Miss",
            MoveClass::Inaccuracy => "Inaccuracy",
            MoveClass::Mistake => "Mistake",
            MoveClass::Blunder => "Blunder",
        }
    }

    pub fn article_phrase(self) -> &'static str {
        match self {
            MoveClass::Brilliant => "is brilliant",
            MoveClass::Great => "is a great move",
            MoveClass::Best => "is the best move",
            MoveClass::Excellent => "is excellent",
            MoveClass::Good => "is good",
            MoveClass::Forced => "is forced",
            MoveClass::Miss => "is a miss",
            MoveClass::Inaccuracy => "is an inaccuracy",
            MoveClass::Mistake => "is a mistake",
            MoveClass::Blunder => "is a blunder",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            MoveClass::Brilliant => "!!",
            MoveClass::Great => "!",
            MoveClass::Best => "★",
            MoveClass::Excellent => "👍",
            MoveClass::Good => "✓",
            MoveClass::Forced => "→",
            MoveClass::Miss => "✕",
            MoveClass::Inaccuracy => "?!",
            MoveClass::Mistake => "?",
            MoveClass::Blunder => "??",
        }
    }

    pub fn color(self) -> &'static str {
        match self {
            MoveClass::Brilliant => "#1fb5a5",
            MoveClass::Great => "#4b88cf",
            MoveClass::Best => "#6eaa3c",
            MoveClass::Excellent => "#52a06a",
            MoveClass::Good => "#8aa883",
            MoveClass::Forced => "#8e8a84",
            MoveClass::Miss => "#c4527f",
            MoveClass::Inaccuracy => "#e9b93b",
            MoveClass::Mistake => "#ec8a35",
            MoveClass::Blunder => "#d6403a",
        }
    }

    pub fn index(self) -> usize {
        MoveClass::ALL.iter().position(|class| *class == self).unwrap_or(0)
    }

    pub fn is_error(self) -> bool {
        matches!(
            self,
            MoveClass::Inaccuracy | MoveClass::Mistake | MoveClass::Miss | MoveClass::Blunder
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionEval {
    pub best: Option<String>,
    pub score: Score,
    pub second: Option<Score>,
}

impl PositionEval {
    pub fn terminal(position: &Chess) -> Option<PositionEval> {
        if position.is_checkmate() {
            Some(PositionEval {
                best: None,
                score: Score::Mate(0),
                second: None,
            })
        } else if position.is_game_over() {
            Some(PositionEval {
                best: None,
                score: Score::Centipawns(0),
                second: None,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveReview {
    pub ply: usize,
    pub mover: Color,
    pub class: MoveClass,
    pub san: String,
    pub win_before: f64,
    pub win_after: f64,
    pub loss: f64,
    pub accuracy: f64,
    pub best_uci: Option<String>,
    pub best_san: Option<String>,
    pub played_best: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameReview {
    pub moves: Vec<MoveReview>,
    pub white_curve: Vec<f64>,
    pub white_scores: Vec<Score>,
    pub accuracy: [f64; 2],
    pub counts: [[usize; CLASS_COUNT]; 2],
}

impl GameReview {
    pub fn class_at(&self, ply: usize) -> Option<MoveClass> {
        ply.checked_sub(1).and_then(|index| self.moves.get(index)).map(|review| review.class)
    }

    pub fn move_at(&self, ply: usize) -> Option<&MoveReview> {
        ply.checked_sub(1).and_then(|index| self.moves.get(index))
    }
}

fn clamp_cp(score: Score) -> i32 {
    score.as_centipawns().clamp(-10_000, 10_000)
}

pub fn win_percent(score: Score) -> f64 {
    match score {
        Score::Mate(moves) if moves > 0 => 100.0,
        Score::Mate(_) => 0.0,
        Score::Centipawns(_) => {
            let cp = clamp_cp(score).clamp(-1000, 1000) as f64;
            50.0 + 50.0 * (2.0 / (1.0 + (-0.003_682_08 * cp).exp()) - 1.0)
        }
    }
}

pub fn move_accuracy(loss: f64) -> f64 {
    (103.1668 * (-0.04354 * loss.max(0.0)).exp() - 3.1669 + 1.0).clamp(0.0, 100.0)
}

fn piece_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 100,
    }
}

pub fn is_sacrifice(before: &Chess, mv: &Move) -> bool {
    let mover = before.turn();
    let moved_value = piece_value(mv.role());
    if mv.role() == Role::Pawn || mv.role() == Role::King {
        return false;
    }
    let captured_value = mv.capture().map(piece_value).unwrap_or(0);
    let mut after = before.clone();
    after.play_unchecked(*mv);
    if after.is_checkmate() {
        return false;
    }
    let target = mv.to();
    let board = after.board();
    let occupied = board.occupied();
    let attackers = board.attacks_to(target, !mover, occupied);
    if attackers.is_empty() {
        return false;
    }
    let cheapest_attacker = attackers
        .into_iter()
        .filter_map(|square| board.piece_at(square))
        .map(|piece| piece_value(piece.role))
        .min()
        .unwrap_or(100);
    let defenders = board.attacks_to(target, mover, occupied);
    let net_loss = if defenders.is_empty() {
        moved_value - captured_value
    } else {
        moved_value - cheapest_attacker - captured_value
    };
    net_loss >= 2
}

fn uci_to_san(position: &Chess, uci: &str) -> Option<String> {
    let mv = uci.parse::<UciMove>().ok()?.to_move(position).ok()?;
    Some(San::from_move(position, mv).to_string())
}

pub fn classify_game(game: &Game, evals: &[PositionEval]) -> GameReview {
    let plies = game.ply_count().min(evals.len().saturating_sub(1));
    let mut moves = Vec::with_capacity(plies);
    let mut white_curve = Vec::with_capacity(plies + 1);
    let mut white_scores = Vec::with_capacity(plies + 1);
    for (ply, eval) in evals.iter().enumerate().take(plies + 1) {
        let position = game.position_at(ply);
        let side = position.turn();
        let white_score = if side == Color::White { eval.score } else { eval.score.negate() };
        white_scores.push(white_score);
        let white_win = if position.is_checkmate() {
            if side == Color::White { 0.0 } else { 100.0 }
        } else {
            win_percent(white_score)
        };
        white_curve.push(white_win);
    }
    let mut totals = [0.0f64; 2];
    let mut tallies = [0usize; 2];
    let mut counts = [[0usize; CLASS_COUNT]; 2];
    let mut previous: Option<(MoveClass, f64)> = None;
    for index in 0..plies {
        let before_position = game.position_at(index);
        let played = &game.moves()[index];
        let mover = before_position.turn();
        let before = &evals[index];
        let after = &evals[index + 1];
        let win_before = win_percent(before.score);
        let win_after = if game.position_at(index + 1).is_checkmate() {
            100.0
        } else {
            win_percent(after.score.negate())
        };
        let loss = (win_before - win_after).max(0.0);
        let legal_count = before_position.legal_moves().len();
        let played_best = before.best.as_deref() == Some(played.uci.as_str()) || legal_count == 1;
        let second_gap = before
            .second
            .map(|second| win_before - win_percent(second))
            .unwrap_or(0.0);
        let opponent_erred = previous.is_some_and(|(class, _)| matches!(class, MoveClass::Mistake | MoveClass::Blunder));
        let class = if legal_count == 1 {
            MoveClass::Forced
        } else if played_best || loss < 0.6 {
            if loss <= 2.0 && win_after >= 45.0 && win_before < 97.0 && legal_count > 1 && is_sacrifice(before_position, &played.mv) {
                MoveClass::Brilliant
            } else if legal_count > 1 && second_gap >= 15.0 && win_before < 97.0 && win_after >= 30.0 {
                MoveClass::Great
            } else {
                MoveClass::Best
            }
        } else if loss <= 2.0 {
            MoveClass::Excellent
        } else if loss <= 5.0 {
            MoveClass::Good
        } else if opponent_erred && win_after >= 35.0 {
            MoveClass::Miss
        } else if loss <= 10.0 {
            MoveClass::Inaccuracy
        } else if loss <= 20.0 {
            MoveClass::Mistake
        } else {
            MoveClass::Blunder
        };
        let accuracy = move_accuracy(loss);
        let slot = match mover {
            Color::White => 0,
            Color::Black => 1,
        };
        if class != MoveClass::Forced {
            totals[slot] += accuracy;
            tallies[slot] += 1;
        }
        counts[slot][class.index()] += 1;
        previous = Some((class, loss));
        let best_san = before.best.as_deref().and_then(|uci| uci_to_san(before_position, uci));
        moves.push(MoveReview {
            ply: index + 1,
            mover,
            class,
            san: played.san.clone(),
            win_before,
            win_after,
            loss,
            accuracy,
            best_uci: before.best.clone(),
            best_san,
            played_best,
        });
    }
    let accuracy = [0, 1].map(|slot| {
        if tallies[slot] == 0 {
            0.0
        } else {
            totals[slot] / tallies[slot] as f64
        }
    });
    GameReview {
        moves,
        white_curve,
        white_scores,
        accuracy,
        counts,
    }
}

pub fn review_icon_svg(class: MoveClass) -> String {
    let glyph = match class {
        MoveClass::Brilliant => concat!(
            "<rect x=\"6.6\" y=\"5\" width=\"3.4\" height=\"9.2\" rx=\"1.7\" fill=\"#fff\"/><circle cx=\"8.3\" cy=\"17.6\" r=\"1.9\" fill=\"#fff\"/>",
            "<rect x=\"14\" y=\"5\" width=\"3.4\" height=\"9.2\" rx=\"1.7\" fill=\"#fff\"/><circle cx=\"15.7\" cy=\"17.6\" r=\"1.9\" fill=\"#fff\"/>"
        )
        .to_owned(),
        MoveClass::Great => "<rect x=\"10.3\" y=\"5\" width=\"3.4\" height=\"9.2\" rx=\"1.7\" fill=\"#fff\"/><circle cx=\"12\" cy=\"17.6\" r=\"1.9\" fill=\"#fff\"/>".to_owned(),
        MoveClass::Best => "<path d=\"M12 4.6 14.2 9.4 19.4 9.9 15.5 13.4 16.6 18.6 12 15.9 7.4 18.6 8.5 13.4 4.6 9.9 9.8 9.4Z\" fill=\"#fff\"/>".to_owned(),
        MoveClass::Excellent => "<path d=\"M6.2 11h2.9v7.6H6.2z M10.3 18.6V11.2l2.9-4.9c.5-.8 1.8-.5 1.8.5v3.4h3c1 0 1.8.9 1.6 1.9l-1.1 5c-.2.9-1 1.5-1.9 1.5z\" fill=\"#fff\"/>".to_owned(),
        MoveClass::Good => "<path d=\"M6.8 12.4 10.4 16 17.4 8.4\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.9\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>".to_owned(),
        MoveClass::Forced => "<path d=\"M6 12h10.5 M12.6 7.6 17 12l-4.4 4.4\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>".to_owned(),
        MoveClass::Miss => "<path d=\"M8 8l8 8M16 8l-8 8\" fill=\"none\" stroke=\"#fff\" stroke-width=\"3\" stroke-linecap=\"round\"/>".to_owned(),
        MoveClass::Inaccuracy => concat!(
            "<path d=\"M5.6 9.2C5.6 5.4 11.6 5.4 11.6 9.2C11.6 11.6 8.6 11.6 8.6 14\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.7\" stroke-linecap=\"round\"/><circle cx=\"8.6\" cy=\"17.7\" r=\"1.8\" fill=\"#fff\"/>",
            "<rect x=\"14.2\" y=\"5\" width=\"3.3\" height=\"9.2\" rx=\"1.65\" fill=\"#fff\"/><circle cx=\"15.85\" cy=\"17.7\" r=\"1.85\" fill=\"#fff\"/>"
        )
        .to_owned(),
        MoveClass::Mistake => "<path d=\"M8.4 9.2C8.4 4.8 15.6 4.8 15.6 9.2C15.6 12 12 12 12 14.2\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.9\" stroke-linecap=\"round\"/><circle cx=\"12\" cy=\"17.8\" r=\"1.9\" fill=\"#fff\"/>".to_owned(),
        MoveClass::Blunder => concat!(
            "<path d=\"M4.6 9.2C4.6 5.4 10.6 5.4 10.6 9.2C10.6 11.6 7.6 11.6 7.6 14\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.7\" stroke-linecap=\"round\"/><circle cx=\"7.6\" cy=\"17.7\" r=\"1.8\" fill=\"#fff\"/>",
            "<path d=\"M13.4 9.2C13.4 5.4 19.4 5.4 19.4 9.2C19.4 11.6 16.4 11.6 16.4 14\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.7\" stroke-linecap=\"round\"/><circle cx=\"16.4\" cy=\"17.7\" r=\"1.8\" fill=\"#fff\"/>"
        )
        .to_owned(),
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12.6\" r=\"11\" fill=\"#000\" opacity=\"0.22\"/><circle cx=\"12\" cy=\"12\" r=\"11\" fill=\"{}\"/><circle cx=\"12\" cy=\"12\" r=\"10.2\" fill=\"none\" stroke=\"#fff\" stroke-opacity=\"0.25\" stroke-width=\"1\"/>{glyph}</svg>",
        class.color()
    )
}
