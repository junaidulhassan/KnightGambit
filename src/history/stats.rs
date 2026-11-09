//! Pure statistics over stored games and their engine reviews.
//!
//! Nothing here touches the UI or the database, so it can run on a worker
//! thread and is fully unit-tested. Move-quality figures are computed only from
//! games that have an actual engine review; nothing is estimated.

use std::collections::HashMap;

use shakmaty::Color;

use crate::review::{CLASS_COUNT, MoveClass, opening_family};

use super::model::{GameRecord, GameReviewSummary, MoveReviewRow, Outcome};

/// Plies 1..=OPENING_PLIES count as the opening phase.
pub const OPENING_PLIES: i64 = 20;
/// Minimum games before an opening, level or color is judged.
pub const MIN_SAMPLE: usize = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub wins: usize,
    pub losses: usize,
    pub draws: usize,
}

impl Tally {
    pub fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Win => self.wins += 1,
            Outcome::Loss => self.losses += 1,
            Outcome::Draw => self.draws += 1,
        }
    }

    pub fn games(&self) -> usize {
        self.wins + self.losses + self.draws
    }

    fn pct(&self, count: usize) -> f64 {
        if self.games() == 0 { 0.0 } else { count as f64 * 100.0 / self.games() as f64 }
    }

    pub fn win_rate(&self) -> f64 {
        self.pct(self.wins)
    }

    pub fn loss_rate(&self) -> f64 {
        self.pct(self.losses)
    }

    pub fn draw_rate(&self) -> f64 {
        self.pct(self.draws)
    }

    /// Points per game as a percentage (win = 1, draw = 0.5).
    pub fn score_pct(&self) -> f64 {
        if self.games() == 0 {
            0.0
        } else {
            (self.wins as f64 + self.draws as f64 * 0.5) * 100.0 / self.games() as f64
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overall {
    pub tally: Tally,
    pub avg_duration_secs: Option<f64>,
    pub avg_plies: f64,
    pub longest_win_streak: usize,
    pub longest_loss_streak: usize,
    pub current_streak: Option<(Outcome, usize)>,
    /// Ending types of lost games, most common first.
    pub loss_terminations: Vec<(String, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrendPoint {
    pub game_index: usize,
    pub played_at: i64,
    pub outcome: Outcome,
    pub rolling_win_rate: f64,
    pub cumulative_win_rate: f64,
}

pub const ROLLING_WINDOW: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct LevelStats {
    pub label: String,
    pub elo: Option<i64>,
    pub tally: Tally,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpeningStats {
    /// Opening family, e.g. "Sicilian Defense".
    pub name: String,
    /// Most frequent ECO code seen for the family.
    pub eco: Option<String>,
    pub tally: Tally,
    pub as_white: Tally,
    pub as_black: Tally,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpeningTrend {
    /// Start time of each chunk of consecutive games.
    pub starts: Vec<i64>,
    /// Games per chunk for the most played openings.
    pub series: Vec<(String, Vec<usize>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Opening,
    Middlegame,
    Endgame,
}

impl Phase {
    pub const ALL: [Phase; 3] = [Phase::Opening, Phase::Middlegame, Phase::Endgame];

    pub fn label(self) -> &'static str {
        match self {
            Phase::Opening => "Opening",
            Phase::Middlegame => "Middlegame",
            Phase::Endgame => "Endgame",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Phase::Opening => 0,
            Phase::Middlegame => 1,
            Phase::Endgame => 2,
        }
    }
}

/// Phase of the 1-based `ply` in a game that reached its endgame at `endgame_ply`.
pub fn phase_of(ply: i64, endgame_ply: Option<i64>) -> Phase {
    if endgame_ply.is_some_and(|start| ply >= start) {
        Phase::Endgame
    } else if ply <= OPENING_PLIES {
        Phase::Opening
    } else {
        Phase::Middlegame
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhaseQuality {
    pub moves: usize,
    pub errors: usize,
    pub blunders: usize,
    accuracy_sum: f64,
    accuracy_n: usize,
}

impl PhaseQuality {
    pub fn avg_accuracy(&self) -> Option<f64> {
        (self.accuracy_n > 0).then(|| self.accuracy_sum / self.accuracy_n as f64)
    }

    pub fn error_rate(&self) -> Option<f64> {
        (self.moves > 0).then(|| self.errors as f64 * 100.0 / self.moves as f64)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhaseStats {
    /// Games that reached an endgame.
    pub endgame_reached: Tally,
    /// Games that ended in the middlegame (after the opening, before an endgame).
    pub ended_middlegame: Tally,
    /// Games that ended within the opening phase.
    pub ended_opening: Tally,
    /// Engine-reviewed user moves per phase.
    pub quality: [PhaseQuality; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameQuality {
    pub game_id: i64,
    pub played_at: i64,
    pub outcome: Outcome,
    pub moves: usize,
    pub counts: [usize; CLASS_COUNT],
    pub accuracy: f64,
}

impl GameQuality {
    fn per_hundred(&self, classes: &[MoveClass]) -> f64 {
        if self.moves == 0 {
            return 0.0;
        }
        let total: usize = classes.iter().map(|class| self.counts[class.index()]).sum();
        total as f64 * 100.0 / self.moves as f64
    }

    pub fn inaccuracies_per_100(&self) -> f64 {
        self.per_hundred(&[MoveClass::Inaccuracy])
    }

    pub fn mistakes_per_100(&self) -> f64 {
        self.per_hundred(&[MoveClass::Mistake, MoveClass::Miss])
    }

    pub fn blunders_per_100(&self) -> f64 {
        self.per_hundred(&[MoveClass::Blunder])
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoveQuality {
    pub analyzed_games: usize,
    pub total_games: usize,
    pub counts: [usize; CLASS_COUNT],
    pub total_moves: usize,
    pub avg_accuracy: Option<f64>,
    pub per_game: Vec<GameQuality>,
    /// Class counts and move totals of the user's moves in won / lost games.
    pub in_wins: ([usize; CLASS_COUNT], usize),
    pub in_losses: ([usize; CLASS_COUNT], usize),
    /// Move-class counts per phase.
    pub heatmap: [[usize; CLASS_COUNT]; 3],
}

impl MoveQuality {
    pub fn has_data(&self) -> bool {
        self.analyzed_games > 0
    }

    pub fn share(counts: &[usize; CLASS_COUNT], total: usize, class: MoveClass) -> f64 {
        if total == 0 { 0.0 } else { counts[class.index()] as f64 * 100.0 / total as f64 }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErrorPatterns {
    /// Mistakes, misses and blunders by moved piece: K, Q, R, B, N, pawn.
    pub by_piece: [usize; 6],
    pub captures: usize,
    pub quiet: usize,
    pub total: usize,
}

pub const PIECE_LABELS: [&str; 6] = ["King", "Queen", "Rook", "Bishop", "Knight", "Pawn"];

#[derive(Debug, Clone, PartialEq)]
pub struct RadarAxis {
    pub label: &'static str,
    pub value: f64,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsightKind {
    Strength,
    Weakness,
    Improvement,
    Note,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Insight {
    pub kind: InsightKind,
    pub title: String,
    pub detail: String,
    pub advice: Option<String>,
}

/// Recent form versus the games before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub window: usize,
    pub recent_score: f64,
    pub previous_score: f64,
    pub recent_accuracy: Option<f64>,
    pub previous_accuracy: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub overall: Option<Overall>,
    pub trend: Vec<TrendPoint>,
    pub by_color: [Tally; 2],
    pub by_level: Vec<LevelStats>,
    pub openings: Vec<OpeningStats>,
    pub unclassified: Tally,
    pub opening_trend: OpeningTrend,
    /// ECO classes A–E.
    pub eco_classes: [Tally; 5],
    pub phases: PhaseStats,
    pub quality: MoveQuality,
    pub errors: ErrorPatterns,
    pub radar: Vec<RadarAxis>,
    pub progress: Option<Progress>,
    pub insights: Vec<Insight>,
}

impl Stats {
    pub fn total_games(&self) -> usize {
        self.overall.as_ref().map_or(0, |overall| overall.tally.games())
    }

    /// Openings with at least `min_games`, ordered by score (best first when `best`).
    pub fn ranked_openings(&self, min_games: usize, best: bool, limit: usize) -> Vec<&OpeningStats> {
        let mut ranked: Vec<&OpeningStats> =
            self.openings.iter().filter(|opening| opening.tally.games() >= min_games).collect();
        ranked.sort_by(|a, b| {
            let order = a
                .tally
                .score_pct()
                .total_cmp(&b.tally.score_pct())
                .then(a.tally.games().cmp(&b.tally.games()));
            if best { order.reverse() } else { order }
        });
        ranked.truncate(limit);
        ranked
    }
}

pub struct StatsInput<'a> {
    pub games: &'a [GameRecord],
    pub moves: &'a [MoveReviewRow],
    pub summaries: &'a [(i64, GameReviewSummary)],
}

fn class_from_label(label: &str) -> Option<MoveClass> {
    MoveClass::ALL.iter().copied().find(|class| class.label() == label)
}

fn level_rank(label: &str) -> usize {
    match label {
        "Beginner" => 0,
        "Intermediate" => 1,
        "Advanced" => 2,
        "Maximum Strength" => 3,
        "Custom" => 4,
        _ => 5,
    }
}

fn eco_class_index(eco: &str) -> Option<usize> {
    match eco.chars().next()? {
        'A' => Some(0),
        'B' => Some(1),
        'C' => Some(2),
        'D' => Some(3),
        'E' => Some(4),
        _ => None,
    }
}

fn piece_slot(san: &str) -> usize {
    match san.chars().next() {
        Some('K') | Some('O') => 0,
        Some('Q') => 1,
        Some('R') => 2,
        Some('B') => 3,
        Some('N') => 4,
        _ => 5,
    }
}

fn is_error(class: MoveClass) -> bool {
    matches!(class, MoveClass::Inaccuracy | MoveClass::Mistake | MoveClass::Miss | MoveClass::Blunder)
}

fn is_serious_error(class: MoveClass) -> bool {
    matches!(class, MoveClass::Mistake | MoveClass::Miss | MoveClass::Blunder)
}

pub fn compute(input: &StatsInput<'_>) -> Stats {
    let mut games: Vec<&GameRecord> = input.games.iter().collect();
    games.sort_by_key(|game| (game.played_at, game.id));
    let mut stats = Stats::default();
    if games.is_empty() {
        return stats;
    }

    // ---- Overall, trend, streaks -------------------------------------------
    let mut overall_tally = Tally::default();
    let mut duration_sum = 0i64;
    let mut duration_n = 0usize;
    let mut plies = 0i64;
    let (mut best_win, mut best_loss, mut run_win, mut run_loss) = (0, 0, 0, 0);
    let mut terminations: HashMap<String, usize> = HashMap::new();
    let mut recent: Vec<Outcome> = Vec::new();
    for (index, game) in games.iter().enumerate() {
        overall_tally.add(game.outcome);
        stats.by_color[usize::from(game.user_color == Color::Black)].add(game.outcome);
        if game.duration_secs > 0 {
            duration_sum += game.duration_secs;
            duration_n += 1;
        }
        plies += game.ply_count;
        match game.outcome {
            Outcome::Win => {
                run_win += 1;
                run_loss = 0;
            }
            Outcome::Loss => {
                run_loss += 1;
                run_win = 0;
                *terminations.entry(game.termination.clone()).or_default() += 1;
            }
            Outcome::Draw => {
                run_win = 0;
                run_loss = 0;
            }
        }
        best_win = best_win.max(run_win);
        best_loss = best_loss.max(run_loss);
        recent.push(game.outcome);
        let window = &recent[recent.len().saturating_sub(ROLLING_WINDOW)..];
        let rolling = window.iter().filter(|o| **o == Outcome::Win).count() as f64 * 100.0 / window.len() as f64;
        stats.trend.push(TrendPoint {
            game_index: index + 1,
            played_at: game.played_at,
            outcome: game.outcome,
            rolling_win_rate: rolling,
            cumulative_win_rate: overall_tally.win_rate(),
        });
    }
    let current_streak = games.last().map(|last| {
        let length = games.iter().rev().take_while(|game| game.outcome == last.outcome).count();
        (last.outcome, length)
    });
    let mut loss_terminations: Vec<(String, usize)> = terminations.into_iter().collect();
    loss_terminations.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    stats.overall = Some(Overall {
        tally: overall_tally,
        avg_duration_secs: (duration_n > 0).then(|| duration_sum as f64 / duration_n as f64),
        avg_plies: plies as f64 / games.len() as f64,
        longest_win_streak: best_win,
        longest_loss_streak: best_loss,
        current_streak,
        loss_terminations,
    });

    // ---- Levels --------------------------------------------------------------
    let mut levels: HashMap<String, LevelStats> = HashMap::new();
    for game in &games {
        let entry = levels.entry(game.opponent_level.clone()).or_insert_with(|| LevelStats {
            label: game.opponent_level.clone(),
            elo: game.opponent_elo,
            tally: Tally::default(),
        });
        entry.tally.add(game.outcome);
        if entry.elo.is_none() {
            entry.elo = game.opponent_elo;
        }
    }
    stats.by_level = levels.into_values().collect();
    stats.by_level.sort_by(|a, b| level_rank(&a.label).cmp(&level_rank(&b.label)).then(a.label.cmp(&b.label)));

    // ---- Openings ------------------------------------------------------------
    struct OpeningAccumulator {
        tally: Tally,
        white: Tally,
        black: Tally,
        eco: HashMap<String, usize>,
    }
    let mut openings: HashMap<String, OpeningAccumulator> = HashMap::new();
    for game in &games {
        let Some(name) = game.opening_name.as_deref() else {
            stats.unclassified.add(game.outcome);
            continue;
        };
        let entry = openings
            .entry(opening_family(name).to_owned())
            .or_insert_with(|| OpeningAccumulator {
                tally: Tally::default(),
                white: Tally::default(),
                black: Tally::default(),
                eco: HashMap::new(),
            });
        entry.tally.add(game.outcome);
        match game.user_color {
            Color::White => entry.white.add(game.outcome),
            Color::Black => entry.black.add(game.outcome),
        }
        if let Some(eco) = &game.eco {
            *entry.eco.entry(eco.clone()).or_default() += 1;
            if let Some(slot) = eco_class_index(eco) {
                stats.eco_classes[slot].add(game.outcome);
            }
        }
    }
    stats.openings = openings
        .into_iter()
        .map(|(name, entry)| {
            let mut ecos: Vec<(String, usize)> = entry.eco.into_iter().collect();
            ecos.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            OpeningStats {
                name,
                eco: ecos.into_iter().next().map(|(eco, _)| eco),
                tally: entry.tally,
                as_white: entry.white,
                as_black: entry.black,
            }
        })
        .collect();
    stats.openings.sort_by(|a, b| b.tally.games().cmp(&a.tally.games()).then(a.name.cmp(&b.name)));

    // Opening trend: consecutive chunks of games, top openings per chunk.
    let classified: Vec<(&GameRecord, String)> = games
        .iter()
        .filter_map(|game| game.opening_name.as_deref().map(|name| (*game, opening_family(name).to_owned())))
        .collect();
    if classified.len() >= 2 {
        let chunks = (classified.len() / 3).clamp(1, 6);
        let per_chunk = classified.len().div_ceil(chunks);
        let top: Vec<String> = stats.openings.iter().take(5).map(|opening| opening.name.clone()).collect();
        let mut trend = OpeningTrend {
            starts: Vec::new(),
            series: top.iter().map(|name| (name.clone(), Vec::new())).collect(),
        };
        for chunk in classified.chunks(per_chunk) {
            trend.starts.push(chunk[0].0.played_at);
            for (name, counts) in &mut trend.series {
                counts.push(chunk.iter().filter(|(_, family)| family == name).count());
            }
        }
        stats.opening_trend = trend;
    }

    // ---- Phases from results ---------------------------------------------------
    for game in &games {
        if game.endgame_ply.is_some() {
            stats.phases.endgame_reached.add(game.outcome);
        } else if game.ply_count > OPENING_PLIES {
            stats.phases.ended_middlegame.add(game.outcome);
        } else {
            stats.phases.ended_opening.add(game.outcome);
        }
    }

    // ---- Move quality (engine-reviewed games only) -------------------------------
    let by_id: HashMap<i64, &GameRecord> = games.iter().map(|game| (game.id, *game)).collect();
    let accuracy_of: HashMap<i64, &GameReviewSummary> =
        input.summaries.iter().map(|(id, summary)| (*id, summary)).collect();
    let mut per_game: HashMap<i64, GameQuality> = HashMap::new();
    let mut error_patterns = ErrorPatterns::default();
    for row in input.moves {
        let Some(game) = by_id.get(&row.game_id) else { continue };
        if row.mover != game.user_color {
            continue;
        }
        let Some(class) = class_from_label(&row.class) else { continue };
        let entry = per_game.entry(game.id).or_insert_with(|| GameQuality {
            game_id: game.id,
            played_at: game.played_at,
            outcome: game.outcome,
            moves: 0,
            counts: [0; CLASS_COUNT],
            accuracy: 0.0,
        });
        entry.moves += 1;
        entry.counts[class.index()] += 1;
        stats.quality.counts[class.index()] += 1;
        stats.quality.total_moves += 1;
        let phase = phase_of(row.ply, game.endgame_ply);
        stats.quality.heatmap[phase.index()][class.index()] += 1;
        let phase_quality = &mut stats.phases.quality[phase.index()];
        phase_quality.moves += 1;
        if is_error(class) {
            phase_quality.errors += 1;
        }
        if class == MoveClass::Blunder {
            phase_quality.blunders += 1;
        }
        if class.counts_for_accuracy() {
            phase_quality.accuracy_sum += row.accuracy;
            phase_quality.accuracy_n += 1;
        }
        let bucket = match game.outcome {
            Outcome::Win => Some(&mut stats.quality.in_wins),
            Outcome::Loss => Some(&mut stats.quality.in_losses),
            Outcome::Draw => None,
        };
        if let Some((counts, total)) = bucket {
            counts[class.index()] += 1;
            *total += 1;
        }
        if is_serious_error(class) {
            error_patterns.total += 1;
            error_patterns.by_piece[piece_slot(&row.san)] += 1;
            if row.san.contains('x') {
                error_patterns.captures += 1;
            } else {
                error_patterns.quiet += 1;
            }
        }
    }
    let mut quality_games: Vec<GameQuality> = per_game.into_values().collect();
    for entry in &mut quality_games {
        let game = by_id[&entry.game_id];
        entry.accuracy = accuracy_of
            .get(&entry.game_id)
            .map(|summary| match game.user_color {
                Color::White => summary.white_accuracy,
                Color::Black => summary.black_accuracy,
            })
            .unwrap_or(0.0);
    }
    quality_games.sort_by_key(|entry| (entry.played_at, entry.game_id));
    stats.quality.total_games = games.len();
    stats.quality.analyzed_games = quality_games.len();
    stats.quality.avg_accuracy = (!quality_games.is_empty())
        .then(|| quality_games.iter().map(|entry| entry.accuracy).sum::<f64>() / quality_games.len() as f64);
    stats.quality.per_game = quality_games;
    stats.errors = error_patterns;

    // ---- Progress ------------------------------------------------------------------
    if games.len() >= 6 {
        let window = (games.len() / 2).min(10);
        let score_of = |slice: &[&GameRecord]| {
            let mut tally = Tally::default();
            slice.iter().for_each(|game| tally.add(game.outcome));
            tally.score_pct()
        };
        let recent = &games[games.len() - window..];
        let previous = &games[games.len() - 2 * window..games.len() - window];
        let accuracies = &stats.quality.per_game;
        let (recent_accuracy, previous_accuracy) = if accuracies.len() >= 6 {
            let half = (accuracies.len() / 2).min(10);
            let mean = |slice: &[GameQuality]| slice.iter().map(|g| g.accuracy).sum::<f64>() / slice.len() as f64;
            (
                Some(mean(&accuracies[accuracies.len() - half..])),
                Some(mean(&accuracies[accuracies.len() - 2 * half..accuracies.len() - half])),
            )
        } else {
            (None, None)
        };
        stats.progress = Some(Progress {
            window,
            recent_score: score_of(recent),
            previous_score: score_of(previous),
            recent_accuracy,
            previous_accuracy,
        });
    }

    stats.radar = radar(&stats);
    stats.insights = insights(&stats);
    stats
}

fn radar(stats: &Stats) -> Vec<RadarAxis> {
    let mut axes = Vec::new();
    for (index, label) in [(0usize, "As White"), (1, "As Black")] {
        let tally = stats.by_color[index];
        if tally.games() >= MIN_SAMPLE {
            axes.push(RadarAxis {
                label,
                value: tally.score_pct(),
                detail: format!(
                    "{:.0}% score over {} games ({}W {}D {}L)",
                    tally.score_pct(),
                    tally.games(),
                    tally.wins,
                    tally.draws,
                    tally.losses
                ),
            });
        }
    }
    let endgames = stats.phases.endgame_reached;
    if endgames.games() >= MIN_SAMPLE {
        axes.push(RadarAxis {
            label: "Endgames",
            value: endgames.score_pct(),
            detail: format!("{:.0}% score in {} games that reached an endgame", endgames.score_pct(), endgames.games()),
        });
    }
    for (phase, label) in [(Phase::Opening, "Opening play"), (Phase::Middlegame, "Middlegame play"), (Phase::Endgame, "Endgame play")] {
        let quality = &stats.phases.quality[phase.index()];
        if quality.moves >= 10
            && let Some(accuracy) = quality.avg_accuracy()
        {
            axes.push(RadarAxis {
                label,
                value: accuracy,
                detail: format!("{accuracy:.0}% average move accuracy over {} reviewed moves", quality.moves),
            });
        }
    }
    if stats.quality.total_moves >= 20 {
        let blunders = stats.quality.counts[MoveClass::Blunder.index()] as f64;
        let rate = blunders * 100.0 / stats.quality.total_moves as f64;
        axes.push(RadarAxis {
            label: "Blunder control",
            value: (100.0 - rate * 10.0).clamp(0.0, 100.0),
            detail: format!("{rate:.1} blunders per 100 reviewed moves (100 = none, 0 = 10 or more)"),
        });
    }
    axes
}

fn pct(value: f64) -> String {
    format!("{value:.0}%")
}

fn insights(stats: &Stats) -> Vec<Insight> {
    let mut list = Vec::new();
    let total = stats.total_games();
    if total == 0 {
        return list;
    }
    let mut push = |kind, title: String, detail: String, advice: Option<&str>| {
        list.push(Insight { kind, title, detail, advice: advice.map(str::to_owned) });
    };

    // Openings
    for opening in stats.ranked_openings(MIN_SAMPLE, true, 2) {
        if opening.tally.score_pct() >= 65.0 {
            push(
                InsightKind::Strength,
                format!("{} works well for you", opening.name),
                format!(
                    "{} score from {} games ({}W {}D {}L).",
                    pct(opening.tally.score_pct()),
                    opening.tally.games(),
                    opening.tally.wins,
                    opening.tally.draws,
                    opening.tally.losses
                ),
                Some("Keep it in your repertoire and learn its main follow-ups a little deeper."),
            );
        }
    }
    for opening in stats.ranked_openings(MIN_SAMPLE, false, 2) {
        if opening.tally.score_pct() <= 35.0 {
            let lost_often = opening.tally.loss_rate() >= 60.0;
            push(
                InsightKind::Weakness,
                format!("{} is costing you points", opening.name),
                format!(
                    "Only {} score from {} games; you lost {} of them.",
                    pct(opening.tally.score_pct()),
                    opening.tally.games(),
                    opening.tally.losses
                ),
                Some(if lost_often {
                    "A recurring problem: review these games and pick one reliable line, or switch to a simpler setup."
                } else {
                    "Replay the games and note where you first left the main line."
                }),
            );
        }
    }

    // Colour balance
    let (white, black) = (stats.by_color[0], stats.by_color[1]);
    if white.games() >= 5 && black.games() >= 5 {
        let gap = white.score_pct() - black.score_pct();
        if gap.abs() >= 15.0 {
            let (strong, weak, weak_tally) =
                if gap > 0.0 { ("White", "Black", black) } else { ("Black", "White", white) };
            push(
                InsightKind::Weakness,
                format!("You score better with {strong} than {weak}"),
                format!(
                    "{} as {strong} against {} as {weak}.",
                    pct(white.score_pct().max(black.score_pct())),
                    pct(weak_tally.score_pct())
                ),
                Some(if weak == "Black" {
                    "Build a compact defence against 1.e4 and 1.d4 and play it consistently."
                } else {
                    "Choose one first-move system for White and study its typical plans."
                }),
            );
        } else {
            push(
                InsightKind::Strength,
                "Balanced with both colors".to_owned(),
                format!("{} as White and {} as Black.", pct(white.score_pct()), pct(black.score_pct())),
                None,
            );
        }
    }

    // Opponent levels
    for level in &stats.by_level {
        if level.tally.games() >= 4 && level.tally.score_pct() <= 35.0 {
            push(
                InsightKind::Weakness,
                format!("{} opposition is a challenge", level.label),
                format!("{} score over {} games.", pct(level.tally.score_pct()), level.tally.games()),
                Some("Play a few games one level lower to rebuild confidence, then step back up."),
            );
        } else if level.tally.games() >= 4 && level.tally.score_pct() >= 75.0 {
            push(
                InsightKind::Strength,
                format!("You handle {} opposition", level.label),
                format!("{} score over {} games.", pct(level.tally.score_pct()), level.tally.games()),
                Some("You may be ready to try the next difficulty level."),
            );
        }
    }

    // Endgames and the middlegame
    let endgames = stats.phases.endgame_reached;
    if endgames.games() >= 4 {
        if endgames.score_pct() >= 65.0 {
            push(
                InsightKind::Strength,
                "Solid in endgames".to_owned(),
                format!("{} score in {} games that reached an endgame.", pct(endgames.score_pct()), endgames.games()),
                None,
            );
        } else if endgames.score_pct() <= 40.0 {
            push(
                InsightKind::Weakness,
                "Endgames need work".to_owned(),
                format!("{} score in {} games that reached an endgame.", pct(endgames.score_pct()), endgames.games()),
                Some("Practise king-and-pawn endings and basic rook endgames (Lucena and Philidor positions)."),
            );
        }
    }
    let middlegame = stats.phases.ended_middlegame;
    if middlegame.games() >= 4 && middlegame.loss_rate() >= 60.0 {
        push(
            InsightKind::Weakness,
            "Games are often lost in the middlegame".to_owned(),
            format!("{} of {} games that ended before an endgame were losses.", middlegame.losses, middlegame.games()),
            Some("Before every move, check what your opponent's last move threatens."),
        );
    }

    // Time management
    if let Some(overall) = &stats.overall {
        let timeouts = overall
            .loss_terminations
            .iter()
            .find(|(name, _)| name == "Time out")
            .map_or(0, |(_, count)| *count);
        if timeouts >= 3 && timeouts * 100 >= overall.tally.losses.max(1) * 30 {
            push(
                InsightKind::Weakness,
                "Time management".to_owned(),
                format!("{timeouts} of your {} losses came on the clock.", overall.tally.losses),
                Some("Use a time control with an increment, and spend less time on early moves."),
            );
        }
    }

    // Engine-backed move quality
    let quality = &stats.quality;
    if quality.analyzed_games >= 3 && quality.total_moves >= 30 {
        let moves = quality.total_moves as f64;
        let blunder_rate = quality.counts[MoveClass::Blunder.index()] as f64 * 100.0 / moves;
        let mistake_rate = (quality.counts[MoveClass::Mistake.index()] + quality.counts[MoveClass::Miss.index()]) as f64 * 100.0 / moves;
        if blunder_rate >= 4.0 {
            push(
                InsightKind::Weakness,
                "Frequent blunders".to_owned(),
                format!(
                    "{blunder_rate:.1} blunders per 100 moves across {} reviewed games.",
                    quality.analyzed_games
                ),
                Some("Do a blunder check before each move: is the piece you move, or any piece it leaves, now undefended?"),
            );
        } else if blunder_rate <= 1.0 {
            push(
                InsightKind::Strength,
                "Few blunders".to_owned(),
                format!("Just {blunder_rate:.1} blunders per 100 moves across {} reviewed games.", quality.analyzed_games),
                None,
            );
        }
        if mistake_rate >= 6.0 {
            push(
                InsightKind::Weakness,
                "Recurring mistakes".to_owned(),
                format!("{mistake_rate:.1} mistakes and misses per 100 moves."),
                Some("Slow down in tense positions and look for your opponent's best reply before committing."),
            );
        }
        // Worst phase by error rate.
        let mut worst: Option<(Phase, f64)> = None;
        for phase in Phase::ALL {
            let phase_quality = &stats.phases.quality[phase.index()];
            if phase_quality.moves >= 15
                && let Some(rate) = phase_quality.error_rate()
                && worst.is_none_or(|(_, best)| rate > best)
            {
                worst = Some((phase, rate));
            }
        }
        if let Some((phase, rate)) = worst
            && rate >= 15.0
        {
            push(
                InsightKind::Weakness,
                format!("Most errors come in the {}", phase.label().to_lowercase()),
                format!("{rate:.0}% of your reviewed {} moves were inaccuracies or worse.", phase.label().to_lowercase()),
                Some(match phase {
                    Phase::Opening => "Learn the first 8–10 moves of one opening for each color.",
                    Phase::Middlegame => "Study basic tactics daily: forks, pins and discovered attacks.",
                    Phase::Endgame => "Study the fundamental endgames; technique wins half-points back.",
                }),
            );
        }
        // Piece pattern for serious errors.
        let errors = &stats.errors;
        if errors.total >= 6
            && let Some((slot, count)) = errors.by_piece.iter().copied().enumerate().max_by_key(|(_, count)| *count)
            && count * 100 >= errors.total * 40
        {
            push(
                InsightKind::Note,
                format!("Most serious errors are {} moves", PIECE_LABELS[slot].to_lowercase()),
                format!("{count} of your {} mistakes and blunders moved a {}.", errors.total, PIECE_LABELS[slot].to_lowercase()),
                Some("Double-check where that piece goes and what it stops defending."),
            );
        }
        if errors.total >= 6 && errors.captures * 100 >= errors.total * 50 {
            push(
                InsightKind::Note,
                "Captures often go wrong".to_owned(),
                format!("{} of {} serious errors were captures.", errors.captures, errors.total),
                Some("Before capturing, count attackers and defenders of the target square."),
            );
        }
    }

    // Improvement
    if let Some(progress) = &stats.progress {
        let delta = progress.recent_score - progress.previous_score;
        if delta >= 15.0 {
            push(
                InsightKind::Improvement,
                "Your results are improving".to_owned(),
                format!(
                    "Score over your last {} games is {} against {} in the {} before.",
                    progress.window,
                    pct(progress.recent_score),
                    pct(progress.previous_score),
                    progress.window
                ),
                None,
            );
        } else if delta <= -15.0 {
            push(
                InsightKind::Weakness,
                "Results have dipped recently".to_owned(),
                format!(
                    "Score over your last {} games is {} against {} before.",
                    progress.window,
                    pct(progress.recent_score),
                    pct(progress.previous_score)
                ),
                Some("Take a short break or review your latest losses to find the common thread."),
            );
        }
        if let (Some(recent), Some(previous)) = (progress.recent_accuracy, progress.previous_accuracy) {
            if recent - previous >= 4.0 {
                push(
                    InsightKind::Improvement,
                    "Move accuracy is rising".to_owned(),
                    format!("Average accuracy {recent:.0}% in recent reviewed games, up from {previous:.0}%."),
                    None,
                );
            } else if previous - recent >= 4.0 {
                push(
                    InsightKind::Weakness,
                    "Move accuracy is slipping".to_owned(),
                    format!("Average accuracy {recent:.0}% in recent reviewed games, down from {previous:.0}%."),
                    Some("Slow down and verify tactics before each move."),
                );
            }
        }
    }

    if quality.analyzed_games < 3 {
        push(
            InsightKind::Note,
            "Review more games for deeper insights".to_owned(),
            format!(
                "{} of {} games have an engine review. Open a game in Game History and choose Review with Stockfish to unlock move-quality insights.",
                quality.analyzed_games, quality.total_games
            ),
            None,
        );
    }
    list
}
