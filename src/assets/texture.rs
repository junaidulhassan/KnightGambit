use resvg::tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

struct XorShift(u64);

impl XorShift {
    fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}

fn crease_path(rng: &mut XorShift, size: f32) -> Option<resvg::tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    let mut x = rng.range(-0.05, 1.05) * size;
    let mut y = rng.range(-0.05, 1.05) * size;
    let mut angle = rng.range(0.0, std::f32::consts::TAU);
    builder.move_to(x, y);
    let segments = 3 + (rng.unit() * 9.0) as usize;
    for _ in 0..segments {
        angle += rng.range(-0.9, 0.9);
        let length = rng.range(0.008, 0.035) * size;
        let control_x = x + angle.cos() * length * 0.5 + rng.range(-1.0, 1.0) * length * 0.3;
        let control_y = y + angle.sin() * length * 0.5 + rng.range(-1.0, 1.0) * length * 0.3;
        x += angle.cos() * length;
        y += angle.sin() * length;
        builder.quad_to(control_x, control_y, x, y);
    }
    builder.finish()
}

pub fn leather_texture_rgba(size: u32, seed: u64) -> Option<Vec<u8>> {
    let mut pixmap = Pixmap::new(size, size)?;
    let mut rng = XorShift(seed.max(1));
    let scale = size as f32 / 1024.0;
    let crease_count = (620.0 * scale * scale) as usize + 40;
    let mut shadow = Paint::default();
    let mut highlight = Paint::default();
    shadow.anti_alias = true;
    highlight.anti_alias = true;
    for _ in 0..crease_count {
        let Some(path) = crease_path(&mut rng, size as f32) else {
            continue;
        };
        let width = rng.range(0.55, 1.35) * scale.max(0.6);
        let strength = rng.range(0.35, 1.0);
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        highlight.set_color(Color::from_rgba(1.0, 1.0, 1.0, 0.12 * strength)?);
        pixmap.stroke_path(
            &path,
            &highlight,
            &stroke,
            Transform::from_translate(width * 0.9, width * 0.9),
            None,
        );
        shadow.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.055 * strength)?);
        pixmap.stroke_path(&path, &shadow, &stroke, Transform::identity(), None);
    }
    let mut data = pixmap.take();
    for (index, pixel) in data.chunks_exact_mut(4).enumerate() {
        let noise = ((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed).rotate_left(17);
        let jitter = (noise >> 59) as u8;
        if pixel[3] == 0 && jitter > 26 {
            pixel[3] = 4;
        }
    }
    Some(data)
}
