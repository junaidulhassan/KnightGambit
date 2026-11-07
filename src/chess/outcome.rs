use shakmaty::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Checkmate,
    Stalemate,
    InsufficientMaterial,
    FivefoldRepetition,
    SeventyFiveMoveRule,
    ThreefoldRepetitionClaimed,
    FiftyMoveRuleClaimed,
    DrawAgreed,
    Resignation,
    Timeout,
    TimeoutVsInsufficientMaterial,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameResult {
    pub winner: Option<Color>,
    pub termination: Termination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawClaim {
    ThreefoldRepetition,
    FiftyMoveRule,
}

impl GameResult {
    pub fn decisive(winner: Color, termination: Termination) -> Self {
        Self {
            winner: Some(winner),
            termination,
        }
    }

    pub fn draw(termination: Termination) -> Self {
        Self {
            winner: None,
            termination,
        }
    }

    pub fn score_text(&self) -> &'static str {
        match self.winner {
            Some(Color::White) => "1-0",
            Some(Color::Black) => "0-1",
            None => "1/2-1/2",
        }
    }

    pub fn headline(&self) -> String {
        match (self.termination, self.winner) {
            (Termination::Checkmate, _) => "Checkmate".to_owned(),
            (Termination::Stalemate, _) => "Stalemate".to_owned(),
            (Termination::Resignation, _) => "Resignation".to_owned(),
            (Termination::Timeout, _) => "Time Out".to_owned(),
            (Termination::Abandoned, _) => "Game Abandoned".to_owned(),
            _ => "Draw".to_owned(),
        }
    }

    pub fn summary(&self) -> String {
        let side = |color: Color| match color {
            Color::White => "White",
            Color::Black => "Black",
        };
        match (self.termination, self.winner) {
            (Termination::Checkmate, Some(winner)) => format!("{} wins by checkmate", side(winner)),
            (Termination::Resignation, Some(winner)) => {
                format!("{} resigned — {} wins", side(!winner), side(winner))
            }
            (Termination::Timeout, Some(winner)) => {
                format!("{} ran out of time — {} wins", side(!winner), side(winner))
            }
            (Termination::Abandoned, Some(winner)) => format!("{} wins", side(winner)),
            (Termination::Stalemate, _) => "Draw by stalemate".to_owned(),
            (Termination::InsufficientMaterial, _) => "Draw by insufficient material".to_owned(),
            (Termination::FivefoldRepetition, _) => "Draw by fivefold repetition".to_owned(),
            (Termination::SeventyFiveMoveRule, _) => "Draw by the seventy-five-move rule".to_owned(),
            (Termination::ThreefoldRepetitionClaimed, _) => {
                "Draw claimed by threefold repetition".to_owned()
            }
            (Termination::FiftyMoveRuleClaimed, _) => "Draw claimed by the fifty-move rule".to_owned(),
            (Termination::DrawAgreed, _) => "Draw by agreement".to_owned(),
            (Termination::TimeoutVsInsufficientMaterial, _) => {
                "Time out, but the opponent cannot checkmate — draw".to_owned()
            }
            (Termination::Abandoned, None) => "Game abandoned".to_owned(),
            (Termination::Checkmate | Termination::Resignation | Termination::Timeout, None) => {
                "Game over".to_owned()
            }
        }
    }

    pub fn pgn_termination(&self) -> &'static str {
        match self.termination {
            Termination::Timeout | Termination::TimeoutVsInsufficientMaterial => "time forfeit",
            Termination::Abandoned => "abandoned",
            _ => "normal",
        }
    }
}
