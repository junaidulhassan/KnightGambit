mod eco;
mod live;
mod openings;

pub use eco::{eco_code, opening_family, opening_of};
pub use live::{EvalCache, bar_label, MoveFeedback, READY_DEPTH, feedback_for_move, is_ready, position_key, white_share};

pub use openings::{OPENING_LINES, OpeningBook, invalid_lines, opening_book};

use serde::{Deserialize, Serialize};
use shakmaty::san::San;
use shakmaty::uci::UciMove;
use shakmaty::{Chess, Color, Move, Position, Role, Square};

use crate::chess::Game;
use crate::engine::uci::Score;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MoveClass {
    Brilliant,
    Great,
    Book,
    Best,
    Excellent,
    Good,
    Forced,
    Inaccuracy,
    Mistake,
    Miss,
    Blunder,
}

pub const CLASS_COUNT: usize = 11;

impl MoveClass {
    pub const ALL: [MoveClass; CLASS_COUNT] = [
        MoveClass::Brilliant,
        MoveClass::Great,
        MoveClass::Book,
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
            MoveClass::Book => "Book",
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
            MoveClass::Book => "is a book move",
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
            MoveClass::Book => "📖",
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
            MoveClass::Brilliant => "#26c2a3",
            MoveClass::Great => "#749bbf",
            MoveClass::Book => "#d5a47d",
            MoveClass::Best => "#81b64c",
            MoveClass::Excellent => "#81b64c",
            MoveClass::Good => "#95b776",
            MoveClass::Forced => "#9e9a93",
            MoveClass::Inaccuracy => "#f7c631",
            MoveClass::Mistake => "#ffa459",
            MoveClass::Miss => "#ff7769",
            MoveClass::Blunder => "#fa412d",
        }
    }

    pub fn highlighted_in_list(self) -> bool {
        matches!(
            self,
            MoveClass::Brilliant
                | MoveClass::Great
                | MoveClass::Best
                | MoveClass::Inaccuracy
                | MoveClass::Mistake
                | MoveClass::Miss
                | MoveClass::Blunder
        )
    }

    pub fn colored_in_list(self) -> bool {
        self.highlighted_in_list() || self == MoveClass::Book
    }

    pub fn counts_for_accuracy(self) -> bool {
        !matches!(self, MoveClass::Book | MoveClass::Forced)
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
    pub estimated_elo: [Option<u32>; 2],
    pub opening: Option<String>,
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

/// Accuracy to performance rating, calibrated against chess.com game reviews.
const ELO_CURVE: [(f64, f64); 11] = [
    (0.0, 100.0),
    (40.0, 250.0),
    (50.0, 450.0),
    (60.0, 750.0),
    (70.0, 1150.0),
    (75.0, 1450.0),
    (80.0, 1775.0),
    (85.0, 2025.0),
    (90.0, 2350.0),
    (95.0, 2750.0),
    (100.0, 3200.0),
];

pub fn estimate_elo(accuracy: f64, scored_moves: usize) -> Option<u32> {
    if scored_moves < 6 {
        return None;
    }
    let accuracy = accuracy.clamp(0.0, 100.0);
    let mut estimate = ELO_CURVE[ELO_CURVE.len() - 1].1;
    for window in ELO_CURVE.windows(2) {
        let (low, high) = (window[0], window[1]);
        if accuracy <= high.0 {
            let t = (accuracy - low.0) / (high.0 - low.0);
            estimate = low.1 + (high.1 - low.1) * t;
            break;
        }
    }
    let rounded = (estimate.max(100.0) / 50.0).round() * 50.0;
    Some(rounded as u32)
}

/// A single move never drags the harmonic mean below this accuracy, so one
/// blunder in an otherwise clean game does not wipe out the whole score.
const HARMONIC_FLOOR: f64 = 10.0;

/// Game accuracy per side: the average of the arithmetic and the harmonic
/// mean of the move accuracies. Book and forced moves are left out.
pub fn game_accuracy(moves: &[MoveReview]) -> [f64; 2] {
    let mut sums = [(0.0f64, 0.0f64, 0usize); 2];
    for item in moves.iter().filter(|item| item.class.counts_for_accuracy()) {
        let slot = match item.mover {
            Color::White => 0,
            Color::Black => 1,
        };
        sums[slot].0 += item.accuracy;
        sums[slot].1 += 1.0 / item.accuracy.max(HARMONIC_FLOOR);
        sums[slot].2 += 1;
    }
    sums.map(|(sum, inverse, count)| {
        if count == 0 {
            return 0.0;
        }
        let arithmetic = sum / count as f64;
        let harmonic = count as f64 / inverse;
        ((arithmetic + harmonic) / 2.0).clamp(0.0, 100.0)
    })
}

/// Beyond ten pawns the game is decided and engine scores swing wildly, so
/// differences past this point are not counted as lost material.
const REVIEW_CP_CAP: i32 = 1000;

fn review_cp(score: Score) -> i32 {
    score.as_centipawns().clamp(-REVIEW_CP_CAP, REVIEW_CP_CAP)
}

pub fn effective_loss(before: Score, after: Score, win_before: f64, win_after: f64, delivered_mate: bool) -> f64 {
    let win_loss = (win_before - win_after).max(0.0);
    if delivered_mate || win_after >= 85.0 {
        return win_loss;
    }
    let best_cp = review_cp(before);
    let played_cp = -review_cp(after);
    let cp_loss = (best_cp - played_cp).max(0);
    let material_equivalent = 50.0 - win_percent(Score::Centipawns(-cp_loss));
    win_loss.max(material_equivalent)
}

pub fn move_accuracy(loss: f64) -> f64 {
    let loss = loss.max(0.0);
    (103.1668 * (-0.04354 * loss).exp() - 3.1669 + 1.0).clamp(0.0, 100.0)
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

/// Moves anyone would find: taking back on the square the opponent just moved
/// to, or stepping away with a piece that is hanging or attacked by a cheaper
/// one. They are never singled out as great moves.
fn is_obvious(before: &Chess, mv: &Move, previous_to: Option<Square>) -> bool {
    if mv.is_capture() && previous_to == Some(mv.to()) {
        return true;
    }
    let Some(from) = mv.from() else { return false };
    if mv.role() == Role::King {
        return false;
    }
    let mover = before.turn();
    let board = before.board();
    let occupied = board.occupied();
    let attackers = board.attacks_to(from, !mover, occupied);
    if attackers.is_empty() {
        return false;
    }
    let cheapest_attacker = attackers
        .into_iter()
        .filter_map(|square| board.piece_at(square))
        .map(|piece| piece_value(piece.role))
        .min()
        .unwrap_or(100);
    cheapest_attacker < piece_value(mv.role()) || board.attacks_to(from, mover, occupied).is_empty()
}

/// Whether the opponent's recent play handed the mover a chance: their last
/// move was a mistake or blunder, or the one before it was and the mover has
/// kept the chance alive since. `class_at` returns the class of a 0-based
/// move index; `index` is the move being judged.
pub fn opponent_gave_chance(class_at: impl Fn(usize) -> Option<MoveClass>, index: usize) -> bool {
    let gift = |at: usize| matches!(class_at(at), Some(MoveClass::Mistake | MoveClass::Blunder));
    let sound = |at: usize| class_at(at).is_some_and(|class| !class.is_error());
    (index >= 1 && gift(index - 1)) || (index >= 3 && gift(index - 3) && sound(index - 2) && sound(index - 1))
}

fn uci_to_san(position: &Chess, uci: &str) -> Option<String> {
    let mv = uci.parse::<UciMove>().ok()?.to_move(position).ok()?;
    Some(San::from_move(position, mv).to_string())
}

/// Everything needed to classify one move.
pub struct MoveInput<'a> {
    pub before_position: &'a Chess,
    pub after_position: &'a Chess,
    pub played: &'a crate::chess::PlayedMove,
    /// Engine evaluation of the position before the move (side to move's view).
    pub before: &'a PositionEval,
    /// Engine evaluation of the position after the move (side to move's view).
    pub after: &'a PositionEval,
    /// The line is still inside the opening book.
    pub in_book: bool,
    /// The opponent recently erred, see [`opponent_gave_chance`].
    pub opponent_erred: bool,
    /// Destination square of the opponent's previous move.
    pub previous_to: Option<Square>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveVerdict {
    pub class: MoveClass,
    pub win_before: f64,
    pub win_after: f64,
    pub loss: f64,
    pub accuracy: f64,
    pub played_best: bool,
}

/// Classifies a single move. Used for full-game reviews and live feedback so
/// both always agree.
pub fn classify_move(input: &MoveInput<'_>) -> MoveVerdict {
    let MoveInput {
        before_position,
        after_position,
        played,
        before,
        after,
        in_book,
        opponent_erred,
        previous_to,
    } = *input;
    let win_before = win_percent(before.score);
    let delivered_mate = after_position.is_checkmate();
    let win_after = if delivered_mate { 100.0 } else { win_percent(after.score.negate()) };
    let loss = effective_loss(before.score, after.score, win_before, win_after, delivered_mate);
    let legal_count = before_position.legal_moves().len();
    let played_best = before.best.as_deref() == Some(played.uci.as_str()) || legal_count == 1;
    let second_gap = before
        .second
        .map(|second| win_before - win_percent(second))
        .unwrap_or(0.0);
    let class = if in_book {
        MoveClass::Book
    } else if legal_count == 1 {
        MoveClass::Forced
    } else if played_best {
        if loss <= 2.0 && win_after >= 45.0 && win_before < 97.0 && is_sacrifice(before_position, &played.mv) {
            MoveClass::Brilliant
        } else if second_gap >= 15.0
            && win_before < 97.0
            && win_after >= 30.0
            && !is_obvious(before_position, &played.mv, previous_to)
        {
            MoveClass::Great
        } else {
            MoveClass::Best
        }
    } else if loss <= 2.0 {
        MoveClass::Excellent
    } else if loss <= 5.0 {
        MoveClass::Good
    } else if loss <= 10.0 {
        MoveClass::Inaccuracy
    } else if opponent_erred && win_before >= 55.0 && win_after >= 40.0 {
        MoveClass::Miss
    } else if loss <= 20.0 {
        MoveClass::Mistake
    } else {
        MoveClass::Blunder
    };
    MoveVerdict {
        class,
        win_before,
        win_after,
        loss,
        accuracy: move_accuracy(loss),
        played_best,
    }
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
    let mut tallies = [0usize; 2];
    let mut counts = [[0usize; CLASS_COUNT]; 2];
    let book = opening_book();
    let mut in_book = game.starts_from_standard_position();
    let mut opening: Option<String> = None;
    for index in 0..plies {
        let before_position = game.position_at(index);
        let after_position = game.position_at(index + 1);
        let played = &game.moves()[index];
        let mover = before_position.turn();
        in_book = in_book && book.contains(after_position);
        if in_book && let Some(name) = book.name_of(after_position) {
            opening = Some(name.to_owned());
        }
        let verdict = classify_move(&MoveInput {
            before_position,
            after_position,
            played,
            before: &evals[index],
            after: &evals[index + 1],
            in_book,
            opponent_erred: opponent_gave_chance(|at| moves.get(at).map(|item: &MoveReview| item.class), index),
            previous_to: index.checked_sub(1).map(|at| game.moves()[at].mv.to()),
        });
        let class = verdict.class;
        let slot = match mover {
            Color::White => 0,
            Color::Black => 1,
        };
        if class.counts_for_accuracy() {
            tallies[slot] += 1;
        }
        counts[slot][class.index()] += 1;
        let before = &evals[index];
        let best_san = before.best.as_deref().and_then(|uci| uci_to_san(before_position, uci));
        moves.push(MoveReview {
            ply: index + 1,
            mover,
            class,
            san: played.san.clone(),
            win_before: verdict.win_before,
            win_after: verdict.win_after,
            loss: verdict.loss,
            accuracy: verdict.accuracy,
            best_uci: before.best.clone(),
            best_san,
            played_best: verdict.played_best,
        });
    }
    let accuracy = game_accuracy(&moves);
    let estimated_elo = [0, 1].map(|slot| estimate_elo(accuracy[slot], tallies[slot]));
    GameReview {
        moves,
        white_curve,
        white_scores,
        accuracy,
        counts,
        estimated_elo,
        opening,
    }
}

fn exclamation(x: f32) -> String {
    format!(
        "<rect x=\"{:.2}\" y=\"5.2\" width=\"3.6\" height=\"8.8\" rx=\"1.8\" fill=\"#fff\"/><circle cx=\"{x:.2}\" cy=\"17.6\" r=\"2.05\" fill=\"#fff\"/>",
        x - 1.8
    )
}

fn question(x: f32) -> String {
    format!(
        "<path d=\"M{:.2} 9.1C{:.2} 5.3 {:.2} 5.3 {:.2} 9.1C{:.2} 11.4 {x:.2} 11.6 {x:.2} 13.9\" fill=\"none\" stroke=\"#fff\" stroke-width=\"3.1\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/><circle cx=\"{x:.2}\" cy=\"17.7\" r=\"2.05\" fill=\"#fff\"/>",
        x - 3.1,
        x - 3.1,
        x + 3.1,
        x + 3.1,
        x + 3.1
    )
}

pub fn review_icon_svg(class: MoveClass) -> String {
    let glyph = match class {
        MoveClass::Brilliant => format!("{}{}", exclamation(8.6), exclamation(15.4)),
        MoveClass::Great => exclamation(12.0),
        MoveClass::Book => "<path d=\"M12 7.4C10.2 6 7.8 5.6 5.2 6v11.1c2.6-.4 5 0 6.8 1.4 1.8-1.4 4.2-1.8 6.8-1.4V6c-2.6-.4-5 0-6.8 1.4Z\" fill=\"#fff\"/><path d=\"M12 7.6v10.6\" stroke-width=\"1.3\" fill=\"none\" stroke-opacity=\"0.85\" stroke=\"#d5a47d\"/>".to_owned(),
        MoveClass::Best => "<path d=\"M12 4.4 14.35 9.25 19.6 9.95 15.75 13.6 16.75 18.85 12 16.3 7.25 18.85 8.25 13.6 4.4 9.95 9.65 9.25Z\" fill=\"#fff\" stroke=\"#fff\" stroke-width=\"0.8\" stroke-linejoin=\"round\"/>".to_owned(),
        MoveClass::Excellent => "<path d=\"M5.6 10.8h3.2v8H5.6z\" fill=\"#fff\"/><path d=\"M10 18.8v-7.7l3-5.1c.55-.9 2-.55 2 .55v3.6h3.2c1.05 0 1.85 1 1.62 2.02l-1.15 5.1c-.2.9-1 1.53-1.92 1.53z\" fill=\"#fff\"/>".to_owned(),
        MoveClass::Good => "<path d=\"M6.6 12.3 10.3 16 17.4 8.3\" fill=\"none\" stroke=\"#fff\" stroke-width=\"3.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>".to_owned(),
        MoveClass::Forced => "<path d=\"M6 12h10.6M12.6 7.6 17 12l-4.4 4.4\" fill=\"none\" stroke=\"#fff\" stroke-width=\"3\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>".to_owned(),
        MoveClass::Inaccuracy => format!("{}{}", question(8.4), exclamation(16.4)),
        MoveClass::Mistake => question(12.0),
        MoveClass::Miss => "<path d=\"M8 8l8 8M16 8l-8 8\" fill=\"none\" stroke=\"#fff\" stroke-width=\"3.2\" stroke-linecap=\"round\"/>".to_owned(),
        MoveClass::Blunder => format!("{}{}", question(7.9), question(16.1)),
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"11.5\" fill=\"{}\"/>{glyph}</svg>",
        class.color()
    )
}
