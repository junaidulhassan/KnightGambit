use std::io::Write;

use knightgambit::assets::{PieceStyle, leather_texture_rgba, render_piece_rgba};
use shakmaty::{Color, Role};

fn blend(dst: &mut [u8], src: &[u8]) {
    let alpha = src[3] as u32;
    for channel in 0..3 {
        dst[channel] = (src[channel] as u32 + dst[channel] as u32 * (255 - alpha) / 255).min(255) as u8;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let output = args.get(1).cloned().unwrap_or_else(|| "preview.ppm".to_owned());
    let style = match args.get(2).map(String::as_str) {
        Some("walnut") => PieceStyle::WalnutMaple,
        Some("flat") => PieceStyle::ClassicFlat,
        Some("staunton") => PieceStyle::Staunton,
        _ => PieceStyle::ForestWood,
    };
    let square = 150u32;
    let roles = [Role::King, Role::Queen, Role::Rook, Role::Bishop, Role::Knight, Role::Pawn];
    let width = square * roles.len() as u32;
    let height = square * 2;
    let light = [0xec, 0xec, 0xe9];
    let dark = [0x36, 0x69, 0x4d];
    let mut image = vec![0u8; (width * height * 3) as usize];
    let texture = leather_texture_rgba(width.max(height), 7).unwrap();
    for y in 0..height {
        for x in 0..width {
            let is_light = ((x / square) + (y / square)) % 2 == 0;
            let base = if is_light { light } else { dark };
            let offset = ((y * width + x) * 3) as usize;
            image[offset..offset + 3].copy_from_slice(&base);
            let t = ((y * width.max(height) + x) * 4) as usize;
            let mut pixel = [image[offset], image[offset + 1], image[offset + 2]];
            blend(&mut pixel, &texture[t..t + 4]);
            image[offset..offset + 3].copy_from_slice(&pixel);
        }
    }
    let size = square;
    let pad = (square - size) / 2;
    for (row, color) in [Color::Black, Color::White].into_iter().enumerate() {
        for (column, role) in roles.into_iter().enumerate() {
            let rgba = render_piece_rgba(role, color, style, size).unwrap();
            for y in 0..size {
                for x in 0..size {
                    let s = ((y * size + x) * 4) as usize;
                    let gx = column as u32 * square + pad + x;
                    let gy = row as u32 * square + pad + y;
                    let d = ((gy * width + gx) * 3) as usize;
                    let mut pixel = [image[d], image[d + 1], image[d + 2]];
                    blend(&mut pixel, &rgba[s..s + 4]);
                    image[d..d + 3].copy_from_slice(&pixel);
                }
            }
        }
    }
    let mut file = std::fs::File::create(output).unwrap();
    write!(file, "P6\n{width} {height}\n255\n").unwrap();
    file.write_all(&image).unwrap();
}
