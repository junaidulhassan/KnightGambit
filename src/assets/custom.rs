use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use shakmaty::{Color, Role};

static CUSTOM_SET: OnceLock<RwLock<HashMap<(Role, Color), String>>> = OnceLock::new();
static REVISION: AtomicU64 = AtomicU64::new(1);

const ROLES: [(Role, char); 6] = [
    (Role::King, 'k'),
    (Role::Queen, 'q'),
    (Role::Rook, 'r'),
    (Role::Bishop, 'b'),
    (Role::Knight, 'n'),
    (Role::Pawn, 'p'),
];

fn set() -> &'static RwLock<HashMap<(Role, Color), String>> {
    CUSTOM_SET.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn custom_svg(role: Role, color: Color) -> Option<String> {
    set().read().ok()?.get(&(role, color)).cloned()
}

pub fn custom_revision() -> u64 {
    REVISION.load(Ordering::SeqCst)
}

fn file_candidates(role_letter: char, color: Color) -> Vec<String> {
    let color_letter = match color {
        Color::White => 'w',
        Color::Black => 'b',
    };
    let role_name = match role_letter {
        'k' => "king",
        'q' => "queen",
        'r' => "rook",
        'b' => "bishop",
        'n' => "knight",
        _ => "pawn",
    };
    let color_name = match color {
        Color::White => "white",
        Color::Black => "black",
    };
    vec![
        format!("{color_letter}{role_letter}"),
        format!("{color_letter}_{role_name}"),
        format!("{color_name}_{role_name}"),
        format!("{color_name}-{role_name}"),
    ]
}

pub fn load_custom_pieces(directory: Option<&Path>) -> usize {
    let mut loaded = HashMap::new();
    if let Some(directory) = directory
        && let Ok(entries) = fs::read_dir(directory)
    {
        let files: Vec<(String, std::path::PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")))
            .filter_map(|path| {
                path.file_stem()
                    .map(|stem| (stem.to_string_lossy().to_ascii_lowercase(), path.clone()))
            })
            .collect();
        for color in [Color::White, Color::Black] {
            for (role, letter) in ROLES {
                let candidates = file_candidates(letter, color);
                if let Some((_, path)) = files.iter().find(|(stem, _)| candidates.contains(stem))
                    && let Ok(text) = fs::read_to_string(path)
                    && super::render_svg_rgba(&text, 16).is_some()
                {
                    loaded.insert((role, color), text);
                }
            }
        }
    }
    let count = loaded.len();
    if let Ok(mut guard) = set().write() {
        *guard = loaded;
    }
    REVISION.fetch_add(1, Ordering::SeqCst);
    count
}
