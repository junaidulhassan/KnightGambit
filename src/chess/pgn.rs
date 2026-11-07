use std::fmt;
use std::ops::ControlFlow;

use pgn_reader::{RawTag, Reader, SanPlus, Visitor};
use shakmaty::{Color, KnownOutcome, Outcome, Position};

use super::game::Game;
use super::outcome::{GameResult, Termination};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PgnHeaders {
    pub event: String,
    pub site: String,
    pub date: String,
    pub round: String,
    pub white: String,
    pub black: String,
    pub time_control: Option<String>,
    pub extra: Vec<(String, String)>,
}

impl PgnHeaders {
    pub fn new(white: &str, black: &str) -> Self {
        Self {
            event: "KnightGambit Game".to_owned(),
            site: "KnightGambit".to_owned(),
            date: String::new(),
            round: "-".to_owned(),
            white: white.to_owned(),
            black: black.to_owned(),
            time_control: None,
            extra: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PgnGame {
    pub headers: PgnHeaders,
    pub game: Game,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgnParseError {
    Empty,
    Io(String),
    Setup(String),
    IllegalMove { ply: usize, san: String },
}

impl fmt::Display for PgnParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgnParseError::Empty => write!(f, "No game was found in the PGN text"),
            PgnParseError::Io(reason) => write!(f, "Could not read PGN: {reason}"),
            PgnParseError::Setup(reason) => write!(f, "Invalid starting position: {reason}"),
            PgnParseError::IllegalMove { ply, san } => {
                let number = ply / 2 + 1;
                let dots = if ply % 2 == 0 { "." } else { "..." };
                write!(f, "Illegal move {number}{dots} {san}")
            }
        }
    }
}

impl std::error::Error for PgnParseError {}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn push_tag(output: &mut String, name: &str, value: &str) {
    output.push_str(&format!("[{name} \"{}\"]\n", escape(value)));
}

pub fn write_pgn(game: &Game, headers: &PgnHeaders) -> String {
    let result_token = game
        .result()
        .filter(|result| result.termination != Termination::Abandoned || result.winner.is_some())
        .map(|result| result.score_text())
        .unwrap_or("*");
    let mut output = String::new();
    push_tag(&mut output, "Event", non_empty(&headers.event, "?"));
    push_tag(&mut output, "Site", non_empty(&headers.site, "?"));
    push_tag(&mut output, "Date", non_empty(&headers.date, "????.??.??"));
    push_tag(&mut output, "Round", non_empty(&headers.round, "-"));
    push_tag(&mut output, "White", non_empty(&headers.white, "?"));
    push_tag(&mut output, "Black", non_empty(&headers.black, "?"));
    push_tag(&mut output, "Result", result_token);
    if !game.starts_from_standard_position() {
        push_tag(&mut output, "SetUp", "1");
        push_tag(&mut output, "FEN", game.initial_fen());
    }
    if let Some(time_control) = &headers.time_control {
        push_tag(&mut output, "TimeControl", time_control);
    }
    if let Some(result) = game.result() {
        push_tag(&mut output, "Termination", result.pgn_termination());
    }
    for (name, value) in &headers.extra {
        push_tag(&mut output, name, value);
    }
    output.push('\n');

    let start = game.position_at(0);
    let mut move_number = start.fullmoves().get();
    let mut white_to_move = start.turn() == Color::White;
    let mut tokens = Vec::with_capacity(game.ply_count() * 2 + 1);
    for (index, played) in game.moves().iter().enumerate() {
        if white_to_move {
            tokens.push(format!("{move_number}."));
        } else if index == 0 {
            tokens.push(format!("{move_number}..."));
        }
        tokens.push(played.san.clone());
        if !white_to_move {
            move_number += 1;
        }
        white_to_move = !white_to_move;
    }
    tokens.push(result_token.to_owned());

    let mut line = String::new();
    for token in tokens {
        if !line.is_empty() && line.len() + 1 + token.len() > 79 {
            output.push_str(&line);
            output.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&token);
    }
    output.push_str(&line);
    output.push('\n');
    output
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() { fallback } else { value }
}

#[derive(Default)]
struct CollectedGame {
    tags: Vec<(String, String)>,
    sans: Vec<SanPlus>,
    outcome: Option<Outcome>,
}

struct Collector;

impl Visitor for Collector {
    type Tags = Vec<(String, String)>;
    type Movetext = CollectedGame;
    type Output = CollectedGame;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(Vec::new())
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        tags.push((
            String::from_utf8_lossy(name).into_owned(),
            value.decode_utf8_lossy().into_owned(),
        ));
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(CollectedGame {
            tags,
            ..CollectedGame::default()
        })
    }

    fn san(&mut self, movetext: &mut Self::Movetext, san_plus: SanPlus) -> ControlFlow<Self::Output> {
        movetext.sans.push(san_plus);
        ControlFlow::Continue(())
    }

    fn outcome(&mut self, movetext: &mut Self::Movetext, outcome: Outcome) -> ControlFlow<Self::Output> {
        movetext.outcome = Some(outcome);
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Self::Movetext) -> Self::Output {
        movetext
    }
}

pub fn parse_pgn(text: &str) -> Result<PgnGame, PgnParseError> {
    let mut reader = Reader::new(text.as_bytes());
    let collected = reader
        .read_game(&mut Collector)
        .map_err(|error| PgnParseError::Io(error.to_string()))?
        .ok_or(PgnParseError::Empty)?;
    if collected.tags.is_empty() && collected.sans.is_empty() {
        return Err(PgnParseError::Empty);
    }

    let tag = |name: &str| {
        collected
            .tags
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    };

    let mut game = match tag("FEN") {
        Some(fen) => Game::from_fen(&fen).map_err(|error| PgnParseError::Setup(error.to_string()))?,
        None => Game::new(),
    };

    for (ply, san_plus) in collected.sans.iter().enumerate() {
        let mv = san_plus
            .san
            .to_move(game.current())
            .map_err(|_| PgnParseError::IllegalMove {
                ply,
                san: san_plus.to_string(),
            })?;
        game.play(mv).map_err(|_| PgnParseError::IllegalMove {
            ply,
            san: san_plus.to_string(),
        })?;
    }

    if !game.is_over() {
        let termination_tag = tag("Termination").unwrap_or_default().to_ascii_lowercase();
        if let Some(Outcome::Known(known)) = collected.outcome {
            let result = match known {
                KnownOutcome::Decisive { winner } => {
                    let termination = if termination_tag.contains("time") {
                        Termination::Timeout
                    } else {
                        Termination::Resignation
                    };
                    GameResult::decisive(winner, termination)
                }
                KnownOutcome::Draw => GameResult::draw(Termination::DrawAgreed),
            };
            game.set_result(result);
        }
    }

    let known_tags = [
        "Event", "Site", "Date", "Round", "White", "Black", "Result", "TimeControl", "FEN", "SetUp",
        "Termination",
    ];
    let headers = PgnHeaders {
        event: tag("Event").unwrap_or_default(),
        site: tag("Site").unwrap_or_default(),
        date: tag("Date").unwrap_or_default(),
        round: tag("Round").unwrap_or_default(),
        white: tag("White").unwrap_or_else(|| "White".to_owned()),
        black: tag("Black").unwrap_or_else(|| "Black".to_owned()),
        time_control: tag("TimeControl"),
        extra: collected
            .tags
            .iter()
            .filter(|(key, _)| !known_tags.iter().any(|known| known.eq_ignore_ascii_case(key)))
            .cloned()
            .collect(),
    };

    Ok(PgnGame { headers, game })
}
