use std::fmt;

use shakmaty::fen::Fen;
use shakmaty::san::SanPlus;
use shakmaty::uci::UciMove;
use shakmaty::zobrist::Zobrist64;
use shakmaty::{
    CastlingMode, CastlingSide, Chess, Color, EnPassantMode, File, Move, Piece, Position, Role,
    Square,
};

use super::outcome::{DrawClaim, GameResult, Termination};

#[derive(Debug, Clone)]
pub struct PlayedMove {
    pub mv: Move,
    pub san: String,
    pub uci: String,
    pub mover: Color,
    pub captured: Option<Piece>,
    pub gives_check: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameSetupError {
    InvalidFen(String),
    IllegalPosition(String),
}

impl fmt::Display for GameSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameSetupError::InvalidFen(reason) => write!(f, "Invalid FEN: {reason}"),
            GameSetupError::IllegalPosition(reason) => write!(f, "Illegal position: {reason}"),
        }
    }
}

impl std::error::Error for GameSetupError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayError {
    GameOver,
    IllegalMove,
}

#[derive(Debug, Clone)]
pub struct Game {
    initial_fen: String,
    positions: Vec<Chess>,
    hashes: Vec<Zobrist64>,
    moves: Vec<PlayedMove>,
    result: Option<GameResult>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        Self::from_position(Chess::default())
    }

    pub fn from_fen(fen: &str) -> Result<Self, GameSetupError> {
        let parsed: Fen = fen
            .trim()
            .parse()
            .map_err(|error: shakmaty::fen::ParseFenError| GameSetupError::InvalidFen(error.to_string()))?;
        let position: Chess = parsed
            .into_position(CastlingMode::Standard)
            .map_err(|error| GameSetupError::IllegalPosition(error.to_string()))?;
        Ok(Self::from_position(position))
    }

    pub fn from_position(position: Chess) -> Self {
        let initial_fen = Fen::from_position(&position, EnPassantMode::Legal).to_string();
        let hash = position.zobrist_hash(EnPassantMode::Legal);
        let mut game = Self {
            initial_fen,
            positions: vec![position],
            hashes: vec![hash],
            moves: Vec::new(),
            result: None,
        };
        game.update_automatic_result();
        game
    }

    pub fn initial_fen(&self) -> &str {
        &self.initial_fen
    }

    pub fn starts_from_standard_position(&self) -> bool {
        self.initial_fen == super::STANDARD_FEN
    }

    pub fn current(&self) -> &Chess {
        self.positions.last().expect("game always holds the initial position")
    }

    pub fn position_at(&self, ply: usize) -> &Chess {
        &self.positions[ply.min(self.positions.len() - 1)]
    }

    pub fn ply_count(&self) -> usize {
        self.moves.len()
    }

    pub fn moves(&self) -> &[PlayedMove] {
        &self.moves
    }

    pub fn last_move(&self) -> Option<&PlayedMove> {
        self.moves.last()
    }

    pub fn turn(&self) -> Color {
        self.current().turn()
    }

    pub fn result(&self) -> Option<GameResult> {
        self.result
    }

    pub fn is_over(&self) -> bool {
        self.result.is_some()
    }

    pub fn fen_at(&self, ply: usize) -> String {
        Fen::from_position(self.position_at(ply), EnPassantMode::Legal).to_string()
    }

    pub fn current_fen(&self) -> String {
        self.fen_at(self.ply_count())
    }

    pub fn uci_moves(&self) -> Vec<String> {
        self.moves.iter().map(|played| played.uci.clone()).collect()
    }

    pub fn legal_moves_from(&self, from: Square) -> Vec<Move> {
        if self.is_over() {
            return Vec::new();
        }
        self.current()
            .legal_moves()
            .into_iter()
            .filter(|mv| mv.from() == Some(from))
            .collect()
    }

    pub fn destination_of(mv: &Move) -> Square {
        match *mv {
            Move::Castle { king, rook } => {
                let file = if rook.file() > king.file() { File::G } else { File::C };
                Square::from_coords(file, king.rank())
            }
            other => other.to(),
        }
    }

    pub fn find_move(&self, from: Square, to: Square, promotion: Option<Role>) -> Option<Move> {
        self.legal_moves_from(from).into_iter().find(|mv| {
            let matches_target = Self::destination_of(mv) == to
                || matches!(mv, Move::Castle { rook, .. } if *rook == to);
            matches_target && mv.promotion() == promotion
        })
    }

    pub fn requires_promotion(&self, from: Square, to: Square) -> bool {
        self.legal_moves_from(from)
            .iter()
            .any(|mv| mv.to() == to && mv.is_promotion())
    }

    pub fn play(&mut self, mv: Move) -> Result<&PlayedMove, PlayError> {
        if self.is_over() {
            return Err(PlayError::GameOver);
        }
        let position = self.current().clone();
        if !position.is_legal(mv) {
            return Err(PlayError::IllegalMove);
        }
        let mover = position.turn();
        let captured = match mv {
            Move::EnPassant { .. } => Some(Piece {
                color: !mover,
                role: Role::Pawn,
            }),
            Move::Normal {
                capture: Some(role),
                ..
            } => Some(Piece { color: !mover, role }),
            _ => None,
        };
        let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
        let mut next = position;
        let san = SanPlus::from_move_and_play_unchecked(&mut next, mv).to_string();
        let gives_check = next.is_check();
        self.hashes.push(next.zobrist_hash(EnPassantMode::Legal));
        self.positions.push(next);
        self.moves.push(PlayedMove {
            mv,
            san,
            uci,
            mover,
            captured,
            gives_check,
        });
        self.update_automatic_result();
        Ok(self.moves.last().expect("move was just pushed"))
    }

    pub fn play_uci(&mut self, uci: &str) -> Result<&PlayedMove, PlayError> {
        let parsed: UciMove = uci.trim().parse().map_err(|_| PlayError::IllegalMove)?;
        let mv = parsed.to_move(self.current()).map_err(|_| PlayError::IllegalMove)?;
        self.play(mv)
    }

    pub fn play_san(&mut self, san: &str) -> Result<&PlayedMove, PlayError> {
        let parsed: SanPlus = san.trim().parse().map_err(|_| PlayError::IllegalMove)?;
        let mv = parsed.san.to_move(self.current()).map_err(|_| PlayError::IllegalMove)?;
        self.play(mv)
    }

    pub fn repetition_count(&self, ply: usize) -> usize {
        let ply = ply.min(self.hashes.len() - 1);
        let target = self.hashes[ply];
        let position = &self.positions[ply];
        let window_start = ply.saturating_sub(position.halfmoves() as usize);
        self.hashes[window_start..=ply]
            .iter()
            .filter(|hash| **hash == target)
            .count()
    }

    pub fn available_claim(&self) -> Option<DrawClaim> {
        if self.is_over() {
            return None;
        }
        if self.repetition_count(self.ply_count()) >= 3 {
            return Some(DrawClaim::ThreefoldRepetition);
        }
        if self.current().halfmoves() >= 100 {
            return Some(DrawClaim::FiftyMoveRule);
        }
        None
    }

    pub fn claim_draw(&mut self) -> Option<GameResult> {
        let claim = self.available_claim()?;
        let termination = match claim {
            DrawClaim::ThreefoldRepetition => Termination::ThreefoldRepetitionClaimed,
            DrawClaim::FiftyMoveRule => Termination::FiftyMoveRuleClaimed,
        };
        self.finish(GameResult::draw(termination))
    }

    pub fn resign(&mut self, loser: Color) -> Option<GameResult> {
        self.finish(GameResult::decisive(!loser, Termination::Resignation))
    }

    pub fn agree_draw(&mut self) -> Option<GameResult> {
        self.finish(GameResult::draw(Termination::DrawAgreed))
    }

    pub fn abandon(&mut self) -> Option<GameResult> {
        self.finish(GameResult {
            winner: None,
            termination: Termination::Abandoned,
        })
    }

    pub fn flag(&mut self, flagged: Color) -> Option<GameResult> {
        let opponent = !flagged;
        let result = if self.current().has_insufficient_material(opponent) {
            GameResult::draw(Termination::TimeoutVsInsufficientMaterial)
        } else {
            GameResult::decisive(opponent, Termination::Timeout)
        };
        self.finish(result)
    }

    pub fn set_result(&mut self, result: GameResult) {
        self.result = Some(result);
    }

    pub fn undo_plies(&mut self, count: usize) -> usize {
        let removable = count.min(self.moves.len());
        for _ in 0..removable {
            self.moves.pop();
            self.positions.pop();
            self.hashes.pop();
        }
        self.result = None;
        self.update_automatic_result();
        removable
    }

    pub fn king_square(&self, ply: usize, color: Color) -> Option<Square> {
        self.position_at(ply).board().king_of(color)
    }

    pub fn checked_king(&self, ply: usize) -> Option<Square> {
        let position = self.position_at(ply);
        if position.is_check() {
            position.board().king_of(position.turn())
        } else {
            None
        }
    }

    pub fn castling_rook_path(mv: &Move) -> Option<(Square, Square)> {
        match *mv {
            Move::Castle { king, rook } => {
                let side = if rook.file() > king.file() {
                    CastlingSide::KingSide
                } else {
                    CastlingSide::QueenSide
                };
                let rook_target = Square::from_coords(side.rook_to_file(), king.rank());
                Some((rook, rook_target))
            }
            _ => None,
        }
    }

    fn finish(&mut self, result: GameResult) -> Option<GameResult> {
        if self.is_over() {
            return None;
        }
        self.result = Some(result);
        self.result
    }

    fn update_automatic_result(&mut self) {
        if self.result.is_some() {
            return;
        }
        let position = self.current();
        let result = if position.is_checkmate() {
            Some(GameResult::decisive(!position.turn(), Termination::Checkmate))
        } else if position.is_stalemate() {
            Some(GameResult::draw(Termination::Stalemate))
        } else if position.is_insufficient_material() {
            Some(GameResult::draw(Termination::InsufficientMaterial))
        } else if self.repetition_count(self.ply_count()) >= 5 {
            Some(GameResult::draw(Termination::FivefoldRepetition))
        } else if position.halfmoves() >= 150 {
            Some(GameResult::draw(Termination::SeventyFiveMoveRule))
        } else {
            None
        };
        self.result = result;
    }
}
