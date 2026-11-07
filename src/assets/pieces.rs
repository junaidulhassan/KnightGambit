use serde::{Deserialize, Serialize};
use shakmaty::{Color, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PieceStyle {
    #[default]
    ForestWood,
    WalnutMaple,
    ClassicFlat,
    Custom,
}

impl PieceStyle {
    pub const ALL: [PieceStyle; 4] = [
        PieceStyle::ForestWood,
        PieceStyle::WalnutMaple,
        PieceStyle::ClassicFlat,
        PieceStyle::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PieceStyle::ForestWood => "Ivory & Charcoal Wood",
            PieceStyle::WalnutMaple => "Walnut & Maple",
            PieceStyle::ClassicFlat => "Classic Flat",
            PieceStyle::Custom => "Custom Folder",
        }
    }
}

struct Palette {
    light: &'static str,
    mid: &'static str,
    shade: &'static str,
    outline: &'static str,
    detail: &'static str,
    grain: Option<(&'static str, f32)>,
    sheen: Option<(&'static str, f32)>,
}

fn palette(style: PieceStyle, color: Color) -> Palette {
    match (style, color) {
        (PieceStyle::ForestWood, Color::White) => Palette {
            light: "#f6f0e4",
            mid: "#ece3d2",
            shade: "#cdbea6",
            outline: "#1d1b19",
            detail: "#1d1b19",
            grain: Some(("#8c6a43", 0.30)),
            sheen: None,
        },
        (PieceStyle::ForestWood, Color::Black) => Palette {
            light: "#5d5753",
            mid: "#4b4643",
            shade: "#383431",
            outline: "#191716",
            detail: "#191716",
            grain: Some(("#1a1716", 0.45)),
            sheen: None,
        },
        (PieceStyle::WalnutMaple, Color::White) => Palette {
            light: "#f5ddb0",
            mid: "#ebc894",
            shade: "#c99d63",
            outline: "#3b2414",
            detail: "#3b2414",
            grain: Some(("#8b5a2b", 0.40)),
            sheen: Some(("#fff4dc", 0.55)),
        },
        (PieceStyle::WalnutMaple, Color::Black) => Palette {
            light: "#77482a",
            mid: "#623a20",
            shade: "#45270f",
            outline: "#1f1006",
            detail: "#1f1006",
            grain: Some(("#1a0c04", 0.45)),
            sheen: Some(("#a3704a", 0.55)),
        },
        (PieceStyle::ClassicFlat, Color::White) => Palette {
            light: "#ffffff",
            mid: "#fbfbfb",
            shade: "#d9d9d9",
            outline: "#202020",
            detail: "#202020",
            grain: None,
            sheen: None,
        },
        (PieceStyle::Custom, color) => palette(PieceStyle::ForestWood, color),
        (PieceStyle::ClassicFlat, Color::Black) => Palette {
            light: "#4a4a4a",
            mid: "#3a3a3a",
            shade: "#2a2a2a",
            outline: "#111111",
            detail: "#111111",
            grain: None,
            sheen: Some(("#7a7a7a", 0.45)),
        },
    }
}

struct Part {
    path: &'static str,
    evenodd: bool,
}

const fn part(path: &'static str) -> Part {
    Part {
        path,
        evenodd: false,
    }
}

const BASE_SLAB: &str = "M 20 92 L 80 92 Q 81.5 92 81.5 90.5 C 81.5 84 80 79 75.5 76.5 C 71 74.5 62 74 50 74 C 38 74 29 74.5 24.5 76.5 C 20 79 18.5 84 18.5 90.5 Q 18.5 92 20 92 Z";

const PAWN: &[Part] = &[
    part("M 44 47 C 44.5 58 39 66 31 72.5 C 25 77 22.5 83 23 89.5 Q 23.3 92 26 92 L 74 92 Q 76.7 92 77 89.5 C 77.5 83 75 77 69 72.5 C 61 66 55.5 58 56 47 Z"),
    part("M 35 45.5 C 35 42 37.5 40.5 40.5 40.5 L 59.5 40.5 C 62.5 40.5 65 42 65 45.5 C 65 49 62.5 50 59.5 50 L 40.5 50 C 37.5 50 35 49 35 45.5 Z"),
    part("M 50 17 C 56.8 17 62 22.2 62 29 C 62 35.8 56.8 41 50 41 C 43.2 41 38 35.8 38 29 C 38 22.2 43.2 17 50 17 Z"),
];

const ROOK: &[Part] = &[
    part("M 31.5 33 L 68.5 33 C 68 45 68.5 58 70 66 C 71 71 73 74 75 76 L 25 76 C 27 74 29 71 30 66 C 31.5 58 32 45 31.5 33 Z"),
    part("M 27.5 12 L 37 12 Q 38.5 12 38.5 13.5 L 38.5 19 L 44.5 19 L 44.5 13.5 Q 44.5 12 46 12 L 54 12 Q 55.5 12 55.5 13.5 L 55.5 19 L 61.5 19 L 61.5 13.5 Q 61.5 12 63 12 L 72.5 12 Q 74 12 74 13.5 L 74 29 C 74 32.5 72 35 68.5 35.5 L 31.5 35.5 C 28 35 26 32.5 26 29 L 26 13.5 Q 26 12 27.5 12 Z"),
    part(BASE_SLAB),
];

const KNIGHT: &[Part] = &[
    part("M 31 76 C 30 69 32.5 63 38 58.5 C 41 56 43 53 43.5 50 C 39 53.5 33 56 27.5 57.5 C 23 58.8 19.5 57.5 18.2 54.5 C 17 51.5 18 48.5 20.5 46.5 C 26 42 31.5 36.5 35 29.5 C 37 25 40 20 43.5 16.5 L 46.5 8.5 L 50.5 14 C 63 15.5 74 26.5 76.5 42 C 78.5 54.5 75.5 66.5 71 76 Z"),
    part(BASE_SLAB),
];

const BISHOP: &[Part] = &[
    part("M 50.5 21 L 50.5 47.5 Q 50.5 50.5 53 50.5 Q 55.5 50.5 55.8 48 L 57.5 34 C 58 31 61 30.2 63 32.5 C 68.5 39 71.5 47 71.5 55.5 C 71.5 63.5 67.5 70.5 62 76 L 38 76 C 32.5 70.5 28.5 63.5 28.5 55.5 C 28.5 41.5 36 30 44.5 22.5 Q 47.5 19.5 50.5 21 Z"),
    part("M 50 8.5 C 53 8.5 55.5 11 55.5 14 C 55.5 17 53 19.5 50 19.5 C 47 19.5 44.5 17 44.5 14 C 44.5 11 47 8.5 50 8.5 Z"),
    part(BASE_SLAB),
];

const QUEEN: &[Part] = &[
    part("M 32 76 L 21 41 L 37 55 L 39 22 L 50 46 L 61 22 L 63 55 L 79 41 L 68 76 Z"),
    part("M 20 32.5 C 23 32.5 25.5 35 25.5 38 C 25.5 41 23 43.5 20 43.5 C 17 43.5 14.5 41 14.5 38 C 14.5 35 17 32.5 20 32.5 Z"),
    part("M 39 13.5 C 42 13.5 44.5 16 44.5 19 C 44.5 22 42 24.5 39 24.5 C 36 24.5 33.5 22 33.5 19 C 33.5 16 36 13.5 39 13.5 Z"),
    part("M 61 13.5 C 64 13.5 66.5 16 66.5 19 C 66.5 22 64 24.5 61 24.5 C 58 24.5 55.5 22 55.5 19 C 55.5 16 58 13.5 61 13.5 Z"),
    part("M 80 32.5 C 83 32.5 85.5 35 85.5 38 C 85.5 41 83 43.5 80 43.5 C 77 43.5 74.5 41 74.5 38 C 74.5 35 77 32.5 80 32.5 Z"),
    part(BASE_SLAB),
];

const KING: &[Part] = &[
    part("M 46.5 7 L 53.5 7 L 53.5 12.5 L 59 12.5 L 59 19.5 L 53.5 19.5 L 53.5 29 L 46.5 29 L 46.5 19.5 L 41 19.5 L 41 12.5 L 46.5 12.5 Z"),
    Part {
        path: "M 50 32.5 C 43.5 24.5 28.5 22.5 19.5 29 C 10 36 11 50.5 19.5 60 C 24 65 27.5 70 29 76 L 71 76 C 72.5 70 76 65 80.5 60 C 89 50.5 90 36 80.5 29 C 71.5 22.5 56.5 24.5 50 32.5 Z M 35.5 43 C 30 46.5 29.5 54.5 34.5 58.5 L 43 58.5 C 44 52.5 41.5 46.5 35.5 43 Z M 64.5 43 C 58.5 46.5 56 52.5 57 58.5 L 65.5 58.5 C 70.5 54.5 70 46.5 64.5 43 Z",
        evenodd: true,
    },
    part(BASE_SLAB),
];

fn parts(role: Role) -> &'static [Part] {
    match role {
        Role::Pawn => PAWN,
        Role::Rook => ROOK,
        Role::Knight => KNIGHT,
        Role::Bishop => BISHOP,
        Role::Queen => QUEEN,
        Role::King => KING,
    }
}

fn details(role: Role, detail: &str, sheen: Option<(&str, f32)>) -> String {
    let mut output = String::new();
    if role == Role::Knight {
        output.push_str(&format!(
            "<ellipse cx=\"38.5\" cy=\"28.5\" rx=\"2.5\" ry=\"1.9\" fill=\"{detail}\"/>\
             <circle cx=\"22.5\" cy=\"51\" r=\"1.4\" fill=\"{detail}\"/>"
        ));
    }
    if let Some((color, opacity)) = sheen {
        let strokes: &[&str] = match role {
            Role::Pawn => &["M 43.5 21 C 44.5 19 46.5 18 48.5 18"],
            Role::Rook => &["M 34.5 42 L 34 62"],
            Role::Knight => &["M 33 34.5 C 30 39 26 42 22.5 45"],
            Role::Bishop => &["M 37.5 45 C 39 39 42 34 45.5 30"],
            Role::Queen => &["M 31 60 L 27 46"],
            Role::King => &["M 21.5 39 C 23 35 27 32.5 31.5 32"],
        };
        for d in strokes {
            output.push_str(&format!(
                "<path d=\"{d}\" stroke=\"{color}\" stroke-width=\"2.2\" stroke-linecap=\"round\" fill=\"none\" opacity=\"{opacity}\"/>"
            ));
        }
    }
    output
}

pub fn piece_svg(role: Role, color: Color, style: PieceStyle) -> String {
    if style == PieceStyle::Custom
        && let Some(svg) = super::custom::custom_svg(role, color)
    {
        return svg;
    }
    let palette = palette(style, color);
    let mut svg = String::with_capacity(4096);
    svg.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><defs>");
    svg.push_str(&format!(
        "<linearGradient id=\"fill\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0.25\">\
         <stop offset=\"0\" stop-color=\"{light}\"/>\
         <stop offset=\"0.56\" stop-color=\"{mid}\"/>\
         <stop offset=\"0.64\" stop-color=\"{shade}\"/>\
         <stop offset=\"1\" stop-color=\"{shade}\"/></linearGradient>",
        light = palette.light,
        mid = palette.mid,
        shade = palette.shade
    ));
    if let Some((grain_color, _)) = palette.grain {
        let (r, g, b) = hex_to_unit(grain_color);
        svg.push_str(&format!(
            "<filter id=\"grain\" x=\"0\" y=\"0\" width=\"100\" height=\"100\" filterUnits=\"userSpaceOnUse\" color-interpolation-filters=\"sRGB\">\
             <feTurbulence type=\"fractalNoise\" baseFrequency=\"0.018 0.26\" numOctaves=\"3\" seed=\"{seed}\" result=\"noise\"/>\
             <feColorMatrix in=\"noise\" type=\"matrix\" values=\"0 0 0 0 {r:.3} 0 0 0 0 {g:.3} 0 0 0 0 {b:.3} 0 0 0 3.2 -1.45\"/>\
             </filter>",
            seed = role_seed(role)
        ));
    }
    for (index, piece_part) in parts(role).iter().enumerate() {
        let rule = if piece_part.evenodd { "evenodd" } else { "nonzero" };
        svg.push_str(&format!(
            "<clipPath id=\"clip{index}\"><path d=\"{d}\" clip-rule=\"{rule}\"/></clipPath>",
            d = piece_part.path
        ));
    }
    svg.push_str("</defs>");
    for (index, piece_part) in parts(role).iter().enumerate() {
        let rule = if piece_part.evenodd { "evenodd" } else { "nonzero" };
        svg.push_str(&format!(
            "<path d=\"{d}\" fill=\"url(#fill)\" fill-rule=\"{rule}\"/>",
            d = piece_part.path
        ));
        if let Some((_, opacity)) = palette.grain {
            svg.push_str(&format!(
                "<rect x=\"0\" y=\"0\" width=\"100\" height=\"100\" filter=\"url(#grain)\" clip-path=\"url(#clip{index})\" opacity=\"{opacity}\"/>"
            ));
        }
        svg.push_str(&format!(
            "<path d=\"{d}\" fill=\"none\" fill-rule=\"{rule}\" stroke=\"{outline}\" stroke-width=\"3.6\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
            d = piece_part.path,
            outline = palette.outline
        ));
    }
    svg.push_str(&details(role, palette.detail, palette.sheen));
    svg.push_str("</svg>");
    svg
}

fn role_seed(role: Role) -> u32 {
    match role {
        Role::Pawn => 3,
        Role::Knight => 11,
        Role::Bishop => 17,
        Role::Rook => 23,
        Role::Queen => 31,
        Role::King => 41,
    }
}

fn hex_to_unit(hex: &str) -> (f32, f32, f32) {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    (
        ((value >> 16) & 0xff) as f32 / 255.0,
        ((value >> 8) & 0xff) as f32 / 255.0,
        (value & 0xff) as f32 / 255.0,
    )
}

pub fn render_svg_rgba(svg: &str, size: u32) -> Option<Vec<u8>> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &options).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let tree_size = tree.size();
    let scale = size as f32 / tree_size.width().max(tree_size.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(pixmap.take())
}

pub fn render_piece_rgba(role: Role, color: Color, style: PieceStyle, size: u32) -> Option<Vec<u8>> {
    render_svg_rgba(&piece_svg(role, color, style), size)
}
