mod board_theme;
mod store;

pub use board_theme::{BOARD_PRESETS, BoardPreset, Rgba, parse_hex_color};
pub use store::{SettingsStore, load_settings, save_settings};

use serde::{Deserialize, Serialize};

use crate::assets::PieceStyle;
use crate::chess::NotationStyle;
use crate::clock::TimeControl;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CoordinateMode {
    #[default]
    Inside,
    Border,
    Hidden,
}

impl CoordinateMode {
    pub const ALL: [CoordinateMode; 3] = [
        CoordinateMode::Inside,
        CoordinateMode::Border,
        CoordinateMode::Hidden,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CoordinateMode::Inside => "Inside squares",
            CoordinateMode::Border => "On the board frame",
            CoordinateMode::Hidden => "Hidden",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnimationSpeed {
    Off,
    Fast,
    #[default]
    Normal,
    Slow,
}

impl AnimationSpeed {
    pub const ALL: [AnimationSpeed; 4] = [
        AnimationSpeed::Off,
        AnimationSpeed::Fast,
        AnimationSpeed::Normal,
        AnimationSpeed::Slow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AnimationSpeed::Off => "Off",
            AnimationSpeed::Fast => "Fast",
            AnimationSpeed::Normal => "Normal",
            AnimationSpeed::Slow => "Slow",
        }
    }

    pub fn move_millis(self) -> u64 {
        match self {
            AnimationSpeed::Off => 0,
            AnimationSpeed::Fast => 110,
            AnimationSpeed::Normal => 190,
            AnimationSpeed::Slow => 320,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReviewDepth {
    Fast,
    #[default]
    Standard,
    Deep,
}

impl ReviewDepth {
    pub const ALL: [ReviewDepth; 3] = [ReviewDepth::Fast, ReviewDepth::Standard, ReviewDepth::Deep];

    pub fn label(self) -> &'static str {
        match self {
            ReviewDepth::Fast => "Fast (0.2 s per position)",
            ReviewDepth::Standard => "Standard (0.5 s per position)",
            ReviewDepth::Deep => "Deep (1.5 s per position)",
        }
    }

    pub fn movetime_ms(self) -> u64 {
        match self {
            ReviewDepth::Fast => 200,
            ReviewDepth::Standard => 500,
            ReviewDepth::Deep => 1500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GameMode {
    #[default]
    VersusEngine,
    TwoPlayers,
    Practice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorChoice {
    #[default]
    White,
    Black,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Difficulty {
    Beginner,
    #[default]
    Intermediate,
    Advanced,
    Maximum,
    Custom,
}

impl Difficulty {
    pub const ALL: [Difficulty; 5] = [
        Difficulty::Beginner,
        Difficulty::Intermediate,
        Difficulty::Advanced,
        Difficulty::Maximum,
        Difficulty::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Beginner => "Beginner",
            Difficulty::Intermediate => "Intermediate",
            Difficulty::Advanced => "Advanced",
            Difficulty::Maximum => "Maximum Strength",
            Difficulty::Custom => "Custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Difficulty::Beginner => "About 1350 Elo, quick replies",
            Difficulty::Intermediate => "About 1800 Elo",
            Difficulty::Advanced => "About 2300 Elo",
            Difficulty::Maximum => "Full engine strength",
            Difficulty::Custom => "Choose Elo, depth and thinking time",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrengthSettings {
    pub difficulty: Difficulty,
    pub custom_elo: u32,
    pub custom_depth: u32,
    pub think_time_ms: u64,
}

impl Default for StrengthSettings {
    fn default() -> Self {
        Self {
            difficulty: Difficulty::Intermediate,
            custom_elo: 2000,
            custom_depth: 0,
            think_time_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewGameSettings {
    pub mode: GameMode,
    pub color: ColorChoice,
    pub engine_id: Option<String>,
    pub strength: StrengthSettings,
    pub clock_enabled: bool,
    pub time_control: TimeControl,
    pub start_fen: String,
}

impl Default for NewGameSettings {
    fn default() -> Self {
        Self {
            mode: GameMode::VersusEngine,
            color: ColorChoice::White,
            engine_id: None,
            strength: StrengthSettings::default(),
            clock_enabled: true,
            time_control: TimeControl::fischer(600, 0),
            start_fen: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemePreference,
    pub board_preset: String,
    pub light_square: String,
    pub dark_square: String,
    pub board_texture: bool,
    pub piece_style: PieceStyle,
    pub custom_piece_dir: Option<String>,
    pub coordinates: CoordinateMode,
    pub show_legal_moves: bool,
    pub highlight_last_move: bool,
    pub animation_speed: AnimationSpeed,
    pub sound_enabled: bool,
    pub sound_volume: f64,
    pub notation: NotationStyle,
    pub auto_queen: bool,
    pub premoves: bool,
    pub show_analysis: bool,
    pub review_depth: ReviewDepth,
    pub clock_tenths: bool,
    pub low_time_warning_seconds: u64,
    pub default_engine: Option<String>,
    pub default_strength: StrengthSettings,
    pub max_threads: u32,
    pub max_hash_mb: u32,
    pub last_game: NewGameSettings,
    pub window_width: i32,
    pub window_height: i32,
    pub window_maximized: bool,
    pub sidebar_width: i32,
}

impl Default for Settings {
    fn default() -> Self {
        let preset = &BOARD_PRESETS[0];
        let threads = std::thread::available_parallelism()
            .map(|count| count.get() as u32)
            .unwrap_or(2);
        Self {
            theme: ThemePreference::System,
            board_preset: preset.id.to_owned(),
            light_square: preset.light.to_owned(),
            dark_square: preset.dark.to_owned(),
            board_texture: true,
            piece_style: PieceStyle::ForestWood,
            custom_piece_dir: None,
            coordinates: CoordinateMode::Inside,
            show_legal_moves: true,
            highlight_last_move: true,
            animation_speed: AnimationSpeed::Normal,
            sound_enabled: true,
            sound_volume: 0.7,
            notation: NotationStyle::Algebraic,
            auto_queen: false,
            premoves: true,
            show_analysis: true,
            review_depth: ReviewDepth::Standard,
            clock_tenths: true,
            low_time_warning_seconds: 20,
            default_engine: Some("stockfish".to_owned()),
            default_strength: StrengthSettings::default(),
            max_threads: (threads / 2).max(1),
            max_hash_mb: 256,
            last_game: NewGameSettings::default(),
            window_width: 1240,
            window_height: 860,
            window_maximized: false,
            sidebar_width: 430,
        }
    }
}

impl Settings {
    pub fn light_color(&self) -> Rgba {
        parse_hex_color(&self.light_square).unwrap_or(Rgba::rgb(0xec, 0xec, 0xe9))
    }

    pub fn dark_color(&self) -> Rgba {
        parse_hex_color(&self.dark_square).unwrap_or(Rgba::rgb(0x36, 0x69, 0x4d))
    }

    pub fn total_threads() -> u32 {
        std::thread::available_parallelism()
            .map(|count| count.get() as u32)
            .unwrap_or(2)
    }

    pub fn total_memory_mb() -> u32 {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|info| {
                info.lines()
                    .find(|line| line.starts_with("MemTotal:"))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .map(|kb| (kb / 1024) as u32)
            .unwrap_or(4096)
    }
}
