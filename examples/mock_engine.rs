use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Move, Position, Role};

fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight => 300,
        Role::Bishop => 320,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 0,
    }
}

fn choose(position: &Chess, salt: usize) -> Option<(Move, i32)> {
    let moves = position.legal_moves();
    let mut best: Option<(Move, i32)> = None;
    for (index, mv) in moves.iter().enumerate() {
        let mut next = position.clone();
        next.play_unchecked(*mv);
        let mut score = mv.capture().map(value).unwrap_or(0) - if mv.capture().is_some() { value(mv.role()) / 10 } else { 0 };
        if next.is_checkmate() {
            score += 100_000;
        }
        if next.is_check() {
            score += 30;
        }
        if mv.is_promotion() {
            score += 800;
        }
        score += ((index * 7919 + salt * 104_729) % 13) as i32;
        if best.as_ref().is_none_or(|(_, current)| score > *current) {
            best = Some((*mv, score));
        }
    }
    best
}

fn parse_position(line: &str) -> Option<Chess> {
    let rest = line.strip_prefix("position ")?;
    let (setup, moves) = rest.split_once(" moves ").unwrap_or((rest, ""));
    let mut position = if setup.trim() == "startpos" {
        Chess::default()
    } else {
        let fen: Fen = setup.trim().strip_prefix("fen ")?.parse().ok()?;
        fen.into_position(CastlingMode::Standard).ok()?
    };
    for token in moves.split_whitespace() {
        let mv = token.parse::<UciMove>().ok()?.to_move(&position).ok()?;
        position.play_unchecked(mv);
    }
    Some(position)
}

fn main() {
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let say = {
        let stdout = stdout.clone();
        move |text: &str| {
            let mut out = stdout.lock().unwrap();
            let _ = writeln!(out, "{text}");
            let _ = out.flush();
        }
    };
    let mut position = Chess::default();
    let searching = Arc::new(AtomicBool::new(false));
    let mut worker: Option<thread::JoinHandle<()>> = None;
    let mut salt = 0usize;
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim().to_owned();
        if line == "uci" {
            say("id name KnightGambit Test Engine 1.0");
            say("id author KnightGambit");
            say("option name Hash type spin default 16 min 1 max 4096");
            say("option name Threads type spin default 1 min 1 max 64");
            say("option name UCI_LimitStrength type check default false");
            say("option name UCI_Elo type spin default 1500 min 1320 max 3190");
            say("option name Style type combo default Normal var Solid var Normal var Risky");
            say("option name Contempt type spin default 0 min -100 max 100");
            say("uciok");
        } else if line == "isready" {
            say("readyok");
        } else if line.starts_with("position") {
            if let Some(parsed) = parse_position(&line) {
                position = parsed;
            }
        } else if line.starts_with("go") {
            salt += 1;
            let infinite = line.contains("infinite");
            let movetime = line
                .split_whitespace()
                .skip_while(|t| *t != "movetime")
                .nth(1)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(250)
                .min(5000);
            let current = position.clone();
            let flag = searching.clone();
            flag.store(true, Ordering::SeqCst);
            let speak = say.clone();
            let local_salt = salt;
            worker = Some(thread::spawn(move || {
                let choice = choose(&current, local_salt);
                let mut depth = 1;
                let mut elapsed = 0;
                loop {
                    if let Some((mv, score)) = &choice {
                        let uci = UciMove::from_move(*mv, CastlingMode::Standard);
                        let mut reply = current.clone();
                        reply.play_unchecked(*mv);
                        let follow = choose(&reply, local_salt + 1)
                            .map(|(m, _)| format!(" {}", UciMove::from_move(m, CastlingMode::Standard)))
                            .unwrap_or_default();
                        speak(&format!(
                            "info depth {depth} seldepth {} score cp {} nodes {} nps 250000 time {elapsed} pv {uci}{follow}",
                            depth + 3,
                            (score.min(&2000) / 4),
                            depth * 4000
                        ));
                    }
                    thread::sleep(Duration::from_millis(50));
                    elapsed += 50;
                    depth += 1;
                    let stop = !flag.load(Ordering::SeqCst);
                    if (!infinite && elapsed >= movetime) || stop {
                        break;
                    }
                }
                flag.store(false, Ordering::SeqCst);
                match choice {
                    Some((mv, _)) => speak(&format!("bestmove {}", UciMove::from_move(mv, CastlingMode::Standard))),
                    None => speak("bestmove (none)"),
                }
            }));
        } else if line == "stop" {
            searching.store(false, Ordering::SeqCst);
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        } else if line == "quit" {
            searching.store(false, Ordering::SeqCst);
            break;
        }
    }
}
