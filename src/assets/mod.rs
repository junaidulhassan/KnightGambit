mod custom;
mod pieces;
mod texture;

pub use custom::{custom_revision, load_custom_pieces};
pub use pieces::{PieceStyle, piece_svg, render_piece_rgba, render_svg_rgba};
pub use texture::leather_texture_rgba;

pub const APP_ICON_SVG: &str = include_str!("../../data/icons/hicolor/scalable/apps/io.github.knightgambit.KnightGambit.svg");
pub const APP_SYMBOLIC_SVG: &str = include_str!("../../data/icons/hicolor/symbolic/apps/io.github.knightgambit.KnightGambit-symbolic.svg");
