use shakmaty::attacks::{bishop_attacks, king_attacks, knight_attacks, queen_attacks, rook_attacks};
use shakmaty::{Bitboard, Board, Color, File, Rank, Role, Square};

pub fn premove_destinations(board: &Board, from: Square) -> Vec<Square> {
    let Some(piece) = board.piece_at(from) else {
        return Vec::new();
    };
    let empty = Bitboard::EMPTY;
    let targets: Bitboard = match piece.role {
        Role::Knight => knight_attacks(from),
        Role::Bishop => bishop_attacks(from, empty),
        Role::Rook => rook_attacks(from, empty),
        Role::Queen => queen_attacks(from, empty),
        Role::King => {
            let mut squares = king_attacks(from);
            let home_rank = match piece.color {
                Color::White => Rank::First,
                Color::Black => Rank::Eighth,
            };
            if from == Square::from_coords(File::E, home_rank) {
                for file in [File::G, File::C] {
                    squares = squares.with(Square::from_coords(file, home_rank));
                }
            }
            squares
        }
        Role::Pawn => {
            let (direction, start_rank) = match piece.color {
                Color::White => (1, Rank::Second),
                Color::Black => (-1, Rank::Seventh),
            };
            let mut squares = Bitboard::EMPTY;
            if let Some(one) = from.offset(8 * direction) {
                squares = squares.with(one);
                if from.rank() == start_rank
                    && let Some(two) = from.offset(16 * direction)
                {
                    squares = squares.with(two);
                }
            }
            for side in [-1, 1] {
                if let Some(file) = from.file().offset(side)
                    && let Some(rank) = from.rank().offset(direction)
                {
                    squares = squares.with(Square::from_coords(file, rank));
                }
            }
            squares
        }
    };
    targets.into_iter().filter(|square| *square != from).collect()
}

pub fn is_promotion_square(board: &Board, from: Square, to: Square) -> bool {
    match board.piece_at(from) {
        Some(piece) if piece.role == Role::Pawn => match piece.color {
            Color::White => to.rank() == Rank::Eighth,
            Color::Black => to.rank() == Rank::First,
        },
        _ => false,
    }
}
