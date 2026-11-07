use shakmaty::{Board, Color, Piece, Role};

use super::game::PlayedMove;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapturedPieces {
    pub by_white: Vec<Role>,
    pub by_black: Vec<Role>,
}

impl CapturedPieces {
    pub fn from_moves(moves: &[PlayedMove]) -> Self {
        let mut captured = Self::default();
        for played in moves {
            if let Some(Piece { role, .. }) = played.captured {
                match played.mover {
                    Color::White => captured.by_white.push(role),
                    Color::Black => captured.by_black.push(role),
                }
            }
        }
        captured.by_white.sort_by_key(|role| role_order(*role));
        captured.by_black.sort_by_key(|role| role_order(*role));
        captured
    }

    pub fn captured_by(&self, color: Color) -> &[Role] {
        match color {
            Color::White => &self.by_white,
            Color::Black => &self.by_black,
        }
    }
}

pub fn role_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

fn role_order(role: Role) -> u8 {
    match role {
        Role::Pawn => 0,
        Role::Knight => 1,
        Role::Bishop => 2,
        Role::Rook => 3,
        Role::Queen => 4,
        Role::King => 5,
    }
}

pub fn material_balance(board: &Board) -> i32 {
    board
        .iter()
        .map(|(_, piece)| {
            let value = role_value(piece.role);
            match piece.color {
                Color::White => value,
                Color::Black => -value,
            }
        })
        .sum()
}
