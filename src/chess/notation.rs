use serde::{Deserialize, Serialize};
use shakmaty::{Color, Move, Role};

use super::game::PlayedMove;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotationStyle {
    #[default]
    Algebraic,
    Figurine,
    LongAlgebraic,
}

impl NotationStyle {
    pub const ALL: [NotationStyle; 3] = [
        NotationStyle::Algebraic,
        NotationStyle::Figurine,
        NotationStyle::LongAlgebraic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NotationStyle::Algebraic => "Standard Algebraic (Nf3)",
            NotationStyle::Figurine => "Figurine Algebraic (♘f3)",
            NotationStyle::LongAlgebraic => "Long Algebraic (Ng1-f3)",
        }
    }
}

fn role_letter(role: Role) -> &'static str {
    match role {
        Role::King => "K",
        Role::Queen => "Q",
        Role::Rook => "R",
        Role::Bishop => "B",
        Role::Knight => "N",
        Role::Pawn => "",
    }
}

fn figurine(letter: char, color: Color) -> Option<char> {
    let glyph = match (letter, color) {
        ('K', Color::White) => '♔',
        ('Q', Color::White) => '♕',
        ('R', Color::White) => '♖',
        ('B', Color::White) => '♗',
        ('N', Color::White) => '♘',
        ('K', Color::Black) => '♚',
        ('Q', Color::Black) => '♛',
        ('R', Color::Black) => '♜',
        ('B', Color::Black) => '♝',
        ('N', Color::Black) => '♞',
        _ => return None,
    };
    Some(glyph)
}

fn suffix_of(san: &str) -> &str {
    let trimmed = san.trim_end_matches(['+', '#']);
    &san[trimmed.len()..]
}

fn long_algebraic(played: &PlayedMove) -> String {
    let suffix = suffix_of(&played.san);
    let body = match played.mv {
        Move::Castle { .. } => played.san.trim_end_matches(['+', '#']).to_owned(),
        Move::Normal {
            role,
            from,
            capture,
            to,
            promotion,
        } => {
            let separator = if capture.is_some() { "x" } else { "-" };
            let promotion = promotion
                .map(|role| format!("={}", role_letter(role)))
                .unwrap_or_default();
            format!("{}{from}{separator}{to}{promotion}", role_letter(role))
        }
        Move::EnPassant { from, to } => format!("{from}x{to} e.p."),
        Move::Put { role, to } => format!("{}@{to}", role_letter(role)),
    };
    format!("{body}{suffix}")
}

pub fn format_san(played: &PlayedMove, style: NotationStyle) -> String {
    match style {
        NotationStyle::Algebraic => played.san.clone(),
        NotationStyle::Figurine => played
            .san
            .chars()
            .map(|ch| figurine(ch, played.mover).unwrap_or(ch))
            .collect(),
        NotationStyle::LongAlgebraic => long_algebraic(played),
    }
}
