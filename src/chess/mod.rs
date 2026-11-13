mod arrows;
mod game;
mod material;
mod notation;
mod outcome;
mod pgn;
mod premove;

pub use arrows::{Drawings, DrawColor, UserArrow, knight_elbow};
pub use game::{Game, GameSetupError, PlayedMove};
pub use material::{CapturedPieces, material_balance};
pub use notation::{NotationStyle, format_san};
pub use outcome::{DrawClaim, GameResult, Termination};
pub use premove::{is_promotion_square, premove_destinations};
pub use pgn::{PgnGame, PgnHeaders, PgnParseError, parse_pgn, write_pgn};

pub use shakmaty::{Chess, Color, Move, Piece, Position, Role, Square};

pub const STANDARD_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
