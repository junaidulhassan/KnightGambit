use gtk::graphene;
use shakmaty::{File, Rank, Square};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardGeometry {
    pub origin_x: f32,
    pub origin_y: f32,
    pub square: f32,
    pub frame: f32,
    pub flipped: bool,
}

impl BoardGeometry {
    pub fn compute(width: f32, height: f32, flipped: bool, frame_ratio: f32) -> Self {
        let available = width.min(height).max(64.0);
        let frame = (available * frame_ratio).round();
        let board = ((available - frame * 2.0) / 8.0).floor() * 8.0;
        let square = board / 8.0;
        let outer = board + frame * 2.0;
        let origin_x = ((width - outer) / 2.0).round() + frame;
        let origin_y = ((height - outer) / 2.0).round() + frame;
        Self {
            origin_x,
            origin_y,
            square,
            frame,
            flipped,
        }
    }

    pub fn board_size(&self) -> f32 {
        self.square * 8.0
    }

    pub fn board_rect(&self) -> graphene::Rect {
        graphene::Rect::new(self.origin_x, self.origin_y, self.board_size(), self.board_size())
    }

    pub fn outer_rect(&self) -> graphene::Rect {
        graphene::Rect::new(
            self.origin_x - self.frame,
            self.origin_y - self.frame,
            self.board_size() + self.frame * 2.0,
            self.board_size() + self.frame * 2.0,
        )
    }

    pub fn cell_of(&self, square: Square) -> (f32, f32) {
        let file = square.file().to_u32() as f32;
        let rank = square.rank().to_u32() as f32;
        if self.flipped {
            (7.0 - file, rank)
        } else {
            (file, 7.0 - rank)
        }
    }

    pub fn cell_rect(&self, column: f32, row: f32) -> graphene::Rect {
        graphene::Rect::new(
            self.origin_x + column * self.square,
            self.origin_y + row * self.square,
            self.square,
            self.square,
        )
    }

    pub fn square_rect(&self, square: Square) -> graphene::Rect {
        let (column, row) = self.cell_of(square);
        self.cell_rect(column, row)
    }

    pub fn square_at(&self, x: f64, y: f64) -> Option<Square> {
        let column = ((x as f32 - self.origin_x) / self.square).floor();
        let row = ((y as f32 - self.origin_y) / self.square).floor();
        if !(0.0..8.0).contains(&column) || !(0.0..8.0).contains(&row) {
            return None;
        }
        let (file, rank) = if self.flipped {
            (7 - column as u32, row as u32)
        } else {
            (column as u32, 7 - row as u32)
        };
        Some(Square::from_coords(File::new(file), Rank::new(rank)))
    }

    pub fn display_square(&self, column: u32, row: u32) -> Square {
        let (file, rank) = if self.flipped {
            (7 - column, row)
        } else {
            (column, 7 - row)
        };
        Square::from_coords(File::new(file), Rank::new(rank))
    }
}
