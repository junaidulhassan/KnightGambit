#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Rgba {
    pub const fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::new(
            red as f32 / 255.0,
            green as f32 / 255.0,
            blue as f32 / 255.0,
            1.0,
        )
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }

    pub fn luminance(self) -> f32 {
        0.2126 * self.red + 0.7152 * self.green + 0.0722 * self.blue
    }

    pub fn mix(self, other: Rgba, amount: f32) -> Rgba {
        let amount = amount.clamp(0.0, 1.0);
        Rgba::new(
            self.red + (other.red - self.red) * amount,
            self.green + (other.green - self.green) * amount,
            self.blue + (other.blue - self.blue) * amount,
            self.alpha + (other.alpha - self.alpha) * amount,
        )
    }

    pub fn to_hex(self) -> String {
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(self.red),
            channel(self.green),
            channel(self.blue)
        )
    }
}

pub fn parse_hex_color(text: &str) -> Option<Rgba> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Rgba::rgb(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub light: &'static str,
    pub dark: &'static str,
}

pub const BOARD_PRESETS: [BoardPreset; 5] = [
    BoardPreset {
        id: "forest",
        name: "Forest Green & Ivory",
        light: "#ecece9",
        dark: "#36694d",
    },
    BoardPreset {
        id: "walnut",
        name: "Traditional Brown & Cream",
        light: "#f0d9b5",
        dark: "#b58863",
    },
    BoardPreset {
        id: "meadow",
        name: "Tournament Green",
        light: "#eeeed2",
        dark: "#769656",
    },
    BoardPreset {
        id: "glacier",
        name: "Blue & White",
        light: "#e3e9ee",
        dark: "#7c9cb4",
    },
    BoardPreset {
        id: "graphite",
        name: "Charcoal & Light Gray",
        light: "#d7d8d6",
        dark: "#55595d",
    },
];
