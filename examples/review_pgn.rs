use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use knightgambit::chess::parse_pgn;
use knightgambit::engine::uci::{Score, parse_info};
use knightgambit::review::{MoveClass, PositionEval, classify_game};

fn score_text(score: Score) -> String {
    match score {
        Score::Centipawns(value) => format!("cp:{value}"),
        Score::Mate(moves) => format!("mate:{moves}"),
    }
}

fn parse_score(text: &str) -> Option<Score> {
    let (kind, value) = text.split_once(':')?;
    let value = value.parse().ok()?;
    match kind {
        "cp" => Some(Score::Centipawns(value)),
        "mate" => Some(Score::Mate(value)),
        _ => None,
    }
}

fn save(path: &str, evals: &[PositionEval]) {
    let lines: Vec<String> = evals
        .iter()
        .map(|eval| {
            format!(
                "{} {} {}",
                eval.best.as_deref().unwrap_or("-"),
                score_text(eval.score),
                eval.second.map(score_text).unwrap_or_else(|| "-".to_owned())
            )
        })
        .collect();
    std::fs::write(path, lines.join("\n")).expect("write eval cache");
}

fn load(path: &str) -> Option<Vec<PositionEval>> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let best = parts.next().filter(|best| *best != "-").map(str::to_owned);
            let score = parse_score(parts.next()?)?;
            let second = parts.next().and_then(parse_score);
            Some(PositionEval { best, score, second })
        })
        .collect()
}

fn search(game: &knightgambit::chess::Game, engine: &str, movetime: u64) -> Vec<PositionEval> {
    let mut child = Command::new(engine)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start engine");
    let mut stdin = child.stdin.take().expect("engine stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("engine stdout")).lines();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(4);
    writeln!(
        stdin,
        "uci\nsetoption name Threads value {threads}\nsetoption name Hash value 128\nsetoption name MultiPV value 2\nisready"
    )
    .expect("engine setup");
    for line in lines.by_ref() {
        if line.expect("engine output") == "readyok" {
            break;
        }
    }
    let moves = game.uci_moves();
    let mut evals = Vec::with_capacity(moves.len() + 1);
    for ply in 0..=moves.len() {
        if let Some(eval) = PositionEval::terminal(game.position_at(ply)) {
            evals.push(eval);
            continue;
        }
        let position = if game.starts_from_standard_position() {
            "startpos".to_owned()
        } else {
            format!("fen {}", game.initial_fen())
        };
        writeln!(stdin, "position {position} moves {}\ngo movetime {movetime}", moves[..ply].join(" ")).expect("go");
        let mut best = None;
        let mut second = None;
        let mut best_move = None;
        for line in lines.by_ref() {
            let line = line.expect("engine output");
            if let Some(rest) = line.strip_prefix("bestmove ") {
                best_move = rest.split_whitespace().next().map(str::to_owned);
                break;
            }
            let Some(info) = parse_info(&line) else { continue };
            if info.is_bound {
                continue;
            }
            let Some(score) = info.score else { continue };
            match info.multipv {
                Some(2) => second = Some(score),
                _ => best = Some((score, info.pv.first().cloned())),
            }
        }
        let (score, pv_move) = best.unwrap_or((Score::Centipawns(0), None));
        evals.push(PositionEval {
            best: best_move.filter(|m| m != "(none)" && m != "0000").or(pv_move),
            score,
            second,
        });
        eprint!("\rsearched {}/{}", ply + 1, moves.len() + 1);
    }
    eprintln!();
    let _ = writeln!(stdin, "quit");
    let _ = child.wait();
    evals
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(pgn_path), Some(engine)) = (args.first(), args.get(1)) else {
        eprintln!("usage: review_pgn <game.pgn> <engine> [movetime_ms] [eval_cache]");
        std::process::exit(2);
    };
    let movetime = args.get(2).and_then(|value| value.parse().ok()).unwrap_or(500);
    let cache = args.get(3);
    let text = std::fs::read_to_string(pgn_path).expect("read pgn");
    let parsed = parse_pgn(&text).expect("parse pgn");
    let game = parsed.game;
    let evals = match cache.and_then(|path| load(path)) {
        Some(evals) if evals.len() == game.ply_count() + 1 => evals,
        _ => {
            let evals = search(&game, engine, movetime);
            if let Some(path) = cache {
                save(path, &evals);
            }
            evals
        }
    };
    let review = classify_game(&game, &evals);
    for item in &review.moves {
        let number = item.ply.div_ceil(2);
        let dots = if item.ply % 2 == 1 { "." } else { "..." };
        println!(
            "{number:>3}{dots:<3} {:<8} {:<11} win {:>5.1} -> {:>5.1}  loss {:>5.1}  acc {:>5.1}  eval {:<9} best {}",
            item.san,
            item.class.label(),
            item.win_before,
            item.win_after,
            item.loss,
            item.accuracy,
            review.white_scores[item.ply].display(),
            item.best_san.as_deref().unwrap_or("-"),
        );
    }
    println!();
    println!("{:<12} {:>7} {:>7}", "", "White", "Black");
    println!("{:<12} {:>7.1} {:>7.1}", "Accuracy", review.accuracy[0], review.accuracy[1]);
    let rating = |slot: usize| review.estimated_elo[slot].map(|elo| elo.to_string()).unwrap_or_else(|| "-".to_owned());
    println!("{:<12} {:>7} {:>7}", "Game rating", rating(0), rating(1));
    for class in MoveClass::ALL {
        println!(
            "{:<12} {:>7} {:>7}",
            class.label(),
            review.counts[0][class.index()],
            review.counts[1][class.index()]
        );
    }
}
