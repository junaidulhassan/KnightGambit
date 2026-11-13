use shakmaty::Square;

/// For a knight-shaped step, the corner square of the "L": the arrow runs the
/// long way first (two squares) and then turns for the single-square leg.
/// `None` when `from` to `to` is not a knight move.
pub fn knight_elbow(from: Square, to: Square) -> Option<Square> {
    let file_delta = i32::from(to.file()) - i32::from(from.file());
    let rank_delta = i32::from(to.rank()) - i32::from(from.rank());
    match (file_delta.abs(), rank_delta.abs()) {
        // Long leg along the ranks first: stay on the file, change rank.
        (1, 2) => Some(Square::from_coords(from.file(), to.rank())),
        // Long leg along the files first: stay on the rank, change file.
        (2, 1) => Some(Square::from_coords(to.file(), from.rank())),
        _ => None,
    }
}

/// Colors of hand-drawn arrows and square marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawColor {
    Green,
    Red,
    Blue,
    Yellow,
}

impl DrawColor {
    /// Picks the color from the held modifier keys: none is green, Shift red,
    /// Ctrl blue and Alt yellow.
    pub fn from_modifiers(shift: bool, ctrl: bool, alt: bool) -> Self {
        if alt {
            DrawColor::Yellow
        } else if ctrl {
            DrawColor::Blue
        } else if shift {
            DrawColor::Red
        } else {
            DrawColor::Green
        }
    }

    pub fn rgba(self) -> (f32, f32, f32, f32) {
        match self {
            DrawColor::Green => (0.59, 0.78, 0.13, 0.8),
            DrawColor::Red => (0.92, 0.30, 0.24, 0.8),
            DrawColor::Blue => (0.0, 0.72, 0.94, 0.8),
            DrawColor::Yellow => (1.0, 0.67, 0.0, 0.8),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserArrow {
    pub from: Square,
    pub to: Square,
    pub color: DrawColor,
}

/// Hand-drawn arrows and square marks. Drawing the same arrow (or mark) again
/// removes it, as on common chess sites; a different color replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawings {
    pub arrows: Vec<UserArrow>,
    pub marks: Vec<(Square, DrawColor)>,
}

const MAX_DRAWINGS: usize = 64;

impl Drawings {
    pub fn is_empty(&self) -> bool {
        self.arrows.is_empty() && self.marks.is_empty()
    }

    pub fn clear(&mut self) -> bool {
        let had = !self.is_empty();
        self.arrows.clear();
        self.marks.clear();
        had
    }

    pub fn toggle_arrow(&mut self, arrow: UserArrow) {
        if arrow.from == arrow.to {
            return;
        }
        match self.arrows.iter().position(|a| a.from == arrow.from && a.to == arrow.to) {
            Some(index) if self.arrows[index].color == arrow.color => {
                self.arrows.remove(index);
            }
            Some(index) => self.arrows[index] = arrow,
            None if self.arrows.len() < MAX_DRAWINGS => self.arrows.push(arrow),
            None => {}
        }
    }

    pub fn toggle_mark(&mut self, square: Square, color: DrawColor) {
        match self.marks.iter().position(|(s, _)| *s == square) {
            Some(index) if self.marks[index].1 == color => {
                self.marks.remove(index);
            }
            Some(index) => self.marks[index] = (square, color),
            None if self.marks.len() < MAX_DRAWINGS => self.marks.push((square, color)),
            None => {}
        }
    }
}
