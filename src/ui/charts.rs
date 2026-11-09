//! Small, dependency-free chart widgets drawn with Cairo.
//!
//! Every chart is a `gtk::DrawingArea` that highlights the element under the
//! pointer and explains it in a tooltip. Colours follow the system light/dark
//! foreground so the charts stay readable in both themes.

use std::cell::Cell;
use std::f64::consts::{PI, TAU};
use std::rc::Rc;

use gtk::cairo::{Context, FontSlant, FontWeight};
use gtk::prelude::*;

pub const WIN: &str = "#3f9b6a";
pub const DRAW: &str = "#98a0a8";
pub const LOSS: &str = "#d9564a";
pub const ACCENT: &str = "#2f7a52";
pub const SERIES: [&str; 5] = ["#3584e4", "#e5a50a", "#9141ac", "#2f9d6a", "#e66100"];

#[derive(Clone, Copy)]
struct Rgb(f64, f64, f64);

fn rgb(hex: &str) -> Rgb {
    let digits = hex.trim_start_matches('#');
    let channel = |range: std::ops::Range<usize>| {
        digits
            .get(range)
            .and_then(|part| u8::from_str_radix(part, 16).ok())
            .map_or(0.5, |value| f64::from(value) / 255.0)
    };
    Rgb(channel(0..2), channel(2..4), channel(4..6))
}

fn set_color(cr: &Context, hex: &str, alpha: f64) {
    let Rgb(r, g, b) = rgb(hex);
    cr.set_source_rgba(r, g, b, alpha);
}

/// Theme foreground colour, used for text, grid lines and tracks.
struct Ink(gtk::gdk::RGBA);

impl Ink {
    fn set(&self, cr: &Context, alpha: f64) {
        cr.set_source_rgba(
            f64::from(self.0.red()),
            f64::from(self.0.green()),
            f64::from(self.0.blue()),
            alpha * f64::from(self.0.alpha()),
        );
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

// A plain drawing helper; grouping the style arguments would only add noise at every call site.
#[allow(clippy::too_many_arguments)]
fn text(cr: &Context, ink: &Ink, content: &str, x: f64, y: f64, size: f64, align: Align, bold: bool, alpha: f64) {
    cr.select_font_face("Sans", FontSlant::Normal, if bold { FontWeight::Bold } else { FontWeight::Normal });
    cr.set_font_size(size);
    let Ok(extents) = cr.text_extents(content) else { return };
    let left = match align {
        Align::Left => x,
        Align::Center => x - extents.width() / 2.0 - extents.x_bearing(),
        Align::Right => x - extents.width() - extents.x_bearing(),
    };
    ink.set(cr, alpha);
    cr.move_to(left, y);
    let _ = cr.show_text(content);
}

/// Shortens `content` with an ellipsis so it fits in `max_width` pixels.
fn fit(cr: &Context, content: &str, size: f64, max_width: f64) -> String {
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(size);
    let width = |s: &str| cr.text_extents(s).map_or(0.0, |extents| extents.x_advance());
    if width(content) <= max_width {
        return content.to_owned();
    }
    let mut chars: Vec<char> = content.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if width(&candidate) <= max_width {
            return candidate;
        }
    }
    String::new()
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

type HitFn = dyn Fn(f64, f64, f64, f64) -> Option<(usize, String)>;

/// Builds a drawing area that tracks the hovered element. `hit(x, y, width,
/// height)` returns the element index and tooltip text under a point.
fn interactive(
    description: &str,
    min_width: i32,
    height: i32,
    draw: impl Fn(&Context, f64, f64, &Ink, Option<usize>) + 'static,
    hit: impl Fn(f64, f64, f64, f64) -> Option<(usize, String)> + 'static,
) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(min_width);
    area.set_content_height(height);
    area.set_hexpand(true);
    area.update_property(&[gtk::accessible::Property::Label(description)]);
    let hover = Rc::new(Cell::new(None::<usize>));
    let hit: Rc<HitFn> = Rc::new(hit);
    {
        let hover = hover.clone();
        area.set_draw_func(move |area, cr, width, height| {
            let ink = Ink(area.color());
            draw(cr, f64::from(width), f64::from(height), &ink, hover.get());
        });
    }
    let motion = gtk::EventControllerMotion::new();
    {
        let hover = hover.clone();
        let hit = hit.clone();
        let area = area.clone();
        motion.connect_motion(move |_, x, y| {
            let index = hit(x, y, f64::from(area.width()), f64::from(area.height())).map(|(index, _)| index);
            if hover.get() != index {
                hover.set(index);
                area.queue_draw();
            }
        });
    }
    {
        let hover = hover.clone();
        let area = area.clone();
        motion.connect_leave(move |_| {
            if hover.get().is_some() {
                hover.set(None);
                area.queue_draw();
            }
        });
    }
    area.add_controller(motion);
    area.set_has_tooltip(true);
    area.connect_query_tooltip(move |area, x, y, _, tooltip| {
        match hit(f64::from(x), f64::from(y), f64::from(area.width()), f64::from(area.height())) {
            Some((_, content)) => {
                tooltip.set_text(Some(&content));
                true
            }
            None => false,
        }
    });
    area
}

// ---------------------------------------------------------------- donut ----

pub struct Slice {
    pub label: String,
    pub value: f64,
    pub color: String,
    pub tooltip: String,
}

fn donut_geometry(w: f64, h: f64) -> (f64, f64, f64, f64) {
    let radius = (w.min(h) / 2.0 - 8.0).max(20.0);
    (w / 2.0, h / 2.0, radius * 0.68, radius)
}

pub fn donut(slices: Vec<Slice>, center_title: String, center_caption: String) -> gtk::DrawingArea {
    let slices = Rc::new(slices);
    let total: f64 = slices.iter().map(|slice| slice.value).sum();
    let description = format!("{center_title} {center_caption}");
    let draw_slices = slices.clone();
    let hit_slices = slices;
    interactive(
        &description,
        180,
        200,
        move |cr, w, h, ink, hover| {
            let (cx, cy, inner, outer) = donut_geometry(w, h);
            let mid = (inner + outer) / 2.0;
            let thickness = outer - inner;
            if total <= 0.0 {
                ink.set(cr, 0.12);
                cr.set_line_width(thickness);
                cr.arc(cx, cy, mid, 0.0, TAU);
                let _ = cr.stroke();
            } else {
                let mut start = -PI / 2.0;
                for (index, slice) in draw_slices.iter().enumerate() {
                    if slice.value <= 0.0 {
                        continue;
                    }
                    let sweep = slice.value / total * TAU;
                    let gap = if draw_slices.iter().filter(|s| s.value > 0.0).count() > 1 { 0.025 } else { 0.0 };
                    let active = hover == Some(index);
                    set_color(cr, &slice.color, if hover.is_none() || active { 1.0 } else { 0.45 });
                    cr.set_line_width(thickness + if active { 8.0 } else { 0.0 });
                    cr.arc(cx, cy, mid, start + gap / 2.0, start + sweep - gap / 2.0);
                    let _ = cr.stroke();
                    start += sweep;
                }
            }
            let (title, caption) = match hover.and_then(|index| draw_slices.get(index)) {
                Some(slice) => (
                    format!("{:.0}%", slice.value / total.max(1.0) * 100.0),
                    format!("{} · {}", slice.label, slice.value),
                ),
                None => (center_title.clone(), center_caption.clone()),
            };
            text(cr, ink, &title, cx, cy + 2.0, (inner * 0.52).clamp(14.0, 30.0), Align::Center, true, 1.0);
            text(cr, ink, &caption, cx, cy + 20.0, 11.0, Align::Center, false, 0.65);
        },
        move |x, y, w, h| {
            let (cx, cy, inner, outer) = donut_geometry(w, h);
            let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            if distance < inner - 4.0 || distance > outer + 6.0 || total <= 0.0 {
                return None;
            }
            let angle = ((y - cy).atan2(x - cx) + PI / 2.0).rem_euclid(TAU);
            let mut start = 0.0;
            for (index, slice) in hit_slices.iter().enumerate() {
                let sweep = slice.value / total * TAU;
                if slice.value > 0.0 && angle >= start && angle < start + sweep {
                    return Some((index, slice.tooltip.clone()));
                }
                start += sweep;
            }
            None
        },
    )
}

// ----------------------------------------------------------------- line ----

pub struct LineSeries {
    pub name: String,
    pub color: String,
    pub values: Vec<f64>,
}

pub struct LineSpec {
    pub series: Vec<LineSeries>,
    /// Tooltip for each x position.
    pub tooltips: Vec<String>,
    /// Axis labels as (x index, text).
    pub x_labels: Vec<(usize, String)>,
    pub y_max: f64,
    pub y_suffix: &'static str,
    pub description: String,
}

const LINE_MARGIN: (f64, f64, f64, f64) = (44.0, 14.0, 12.0, 26.0);

fn line_x(index: usize, count: usize, w: f64) -> f64 {
    let (left, right, _, _) = LINE_MARGIN;
    let plot = (w - left - right).max(1.0);
    if count <= 1 { left + plot / 2.0 } else { left + plot * index as f64 / (count - 1) as f64 }
}

pub fn line_chart(spec: LineSpec) -> gtk::DrawingArea {
    let spec = Rc::new(spec);
    let count = spec.series.iter().map(|series| series.values.len()).max().unwrap_or(0);
    let description = spec.description.clone();
    let draw_spec = spec.clone();
    interactive(
        &description,
        260,
        220,
        move |cr, w, h, ink, hover| {
            let (left, right, top, bottom) = LINE_MARGIN;
            let plot_h = (h - top - bottom).max(1.0);
            let y_of = |value: f64| top + plot_h * (1.0 - (value / draw_spec.y_max).clamp(0.0, 1.0));
            cr.set_line_width(1.0);
            for step in 0..=4 {
                let value = draw_spec.y_max * f64::from(step) / 4.0;
                let y = y_of(value).round() + 0.5;
                ink.set(cr, 0.10);
                cr.move_to(left, y);
                cr.line_to(w - right, y);
                let _ = cr.stroke();
                text(cr, ink, &format!("{value:.0}{}", draw_spec.y_suffix), left - 6.0, y + 3.5, 10.0, Align::Right, false, 0.6);
            }
            for (index, label) in &draw_spec.x_labels {
                let x = line_x(*index, count, w).clamp(left + 12.0, w - right - 12.0);
                text(cr, ink, label, x, h - 8.0, 10.0, Align::Center, false, 0.6);
            }
            if let Some(index) = hover {
                let x = line_x(index, count, w).round() + 0.5;
                ink.set(cr, 0.25);
                cr.move_to(x, top);
                cr.line_to(x, top + plot_h);
                let _ = cr.stroke();
            }
            for (series_index, series) in draw_spec.series.iter().enumerate() {
                if series.values.is_empty() {
                    continue;
                }
                if series_index == 0 && series.values.len() > 1 {
                    set_color(cr, &series.color, 0.12);
                    cr.move_to(line_x(0, series.values.len(), w), top + plot_h);
                    for (index, value) in series.values.iter().enumerate() {
                        cr.line_to(line_x(index, series.values.len(), w), y_of(*value));
                    }
                    cr.line_to(line_x(series.values.len() - 1, series.values.len(), w), top + plot_h);
                    cr.close_path();
                    let _ = cr.fill();
                }
                set_color(cr, &series.color, 1.0);
                cr.set_line_width(2.2);
                cr.set_line_join(gtk::cairo::LineJoin::Round);
                for (index, value) in series.values.iter().enumerate() {
                    let (x, y) = (line_x(index, series.values.len(), w), y_of(*value));
                    if index == 0 { cr.move_to(x, y) } else { cr.line_to(x, y) }
                }
                let _ = cr.stroke();
                let show_dots = series.values.len() <= 24;
                for (index, value) in series.values.iter().enumerate() {
                    let active = hover == Some(index);
                    if show_dots || active || index + 1 == series.values.len() {
                        set_color(cr, &series.color, 1.0);
                        cr.arc(line_x(index, series.values.len(), w), y_of(*value), if active { 5.0 } else { 3.0 }, 0.0, TAU);
                        let _ = cr.fill();
                    }
                }
            }
        },
        move |x, _, w, _| {
            if count == 0 {
                return None;
            }
            let index = (0..count)
                .min_by(|a, b| (line_x(*a, count, w) - x).abs().total_cmp(&(line_x(*b, count, w) - x).abs()))?;
            spec.tooltips.get(index).map(|tip| (index, tip.clone()))
        },
    )
}

// ------------------------------------------------------- horizontal bars ----

pub struct HBar {
    pub label: String,
    pub value: f64,
    pub color: String,
    pub value_text: String,
    pub tooltip: String,
}

const ROW_HEIGHT: f64 = 26.0;

pub fn hbar_chart(rows: Vec<HBar>, description: &str) -> gtk::DrawingArea {
    let rows = Rc::new(rows);
    let max = rows.iter().map(|row| row.value).fold(0.0, f64::max).max(1e-9);
    let height = (rows.len() as f64 * ROW_HEIGHT + 8.0) as i32;
    let draw_rows = rows.clone();
    interactive(
        description,
        240,
        height,
        move |cr, w, _, ink, hover| {
            let label_w = (w * 0.38).clamp(80.0, 170.0);
            let bar_x = label_w + 8.0;
            let track = (w - bar_x - 84.0).max(20.0);
            for (index, row) in draw_rows.iter().enumerate() {
                let y = 4.0 + index as f64 * ROW_HEIGHT;
                let active = hover == Some(index);
                if active {
                    ink.set(cr, 0.06);
                    rounded_rect(cr, 0.0, y, w, ROW_HEIGHT - 2.0, 6.0);
                    let _ = cr.fill();
                }
                let label = fit(cr, &row.label, 12.0, label_w);
                text(cr, ink, &label, 4.0, y + 16.0, 12.0, Align::Left, false, 0.9);
                ink.set(cr, 0.08);
                rounded_rect(cr, bar_x, y + 6.0, track, 12.0, 6.0);
                let _ = cr.fill();
                let fill = (row.value / max * track).max(if row.value > 0.0 { 4.0 } else { 0.0 });
                if fill > 0.0 {
                    set_color(cr, &row.color, 1.0);
                    rounded_rect(cr, bar_x, y + 6.0, fill, 12.0, 6.0);
                    let _ = cr.fill();
                }
                text(cr, ink, &row.value_text, w - 4.0, y + 16.0, 11.0, Align::Right, true, 0.85);
            }
        },
        move |_, y, _, _| {
            let index = ((y - 4.0) / ROW_HEIGHT).floor();
            if index < 0.0 {
                return None;
            }
            rows.get(index as usize).map(|row| (index as usize, row.tooltip.clone()))
        },
    )
}

// ----------------------------------------------------------- stacked bars ----

pub struct StackRow {
    pub label: String,
    pub right_text: String,
    /// Wins, draws, losses.
    pub counts: [usize; 3],
    pub tooltips: [String; 3],
}

const STACK_ROW: f64 = 30.0;
const STACK_COLORS: [&str; 3] = [WIN, DRAW, LOSS];

pub fn stacked_bars(rows: Vec<StackRow>, description: &str) -> gtk::DrawingArea {
    let rows = Rc::new(rows);
    let height = (rows.len() as f64 * STACK_ROW + 8.0) as i32;
    let draw_rows = rows.clone();
    let layout = |w: f64| {
        let label_w = (w * 0.34).clamp(90.0, 190.0);
        let right_w = 64.0;
        (label_w + 8.0, (w - label_w - 8.0 - right_w - 6.0).max(30.0))
    };
    interactive(
        description,
        280,
        height,
        move |cr, w, _, ink, hover| {
            let label_w = (w * 0.34).clamp(90.0, 190.0);
            let (bar_x, track) = layout(w);
            for (row_index, row) in draw_rows.iter().enumerate() {
                let y = 4.0 + row_index as f64 * STACK_ROW;
                let label = fit(cr, &row.label, 12.0, label_w);
                text(cr, ink, &label, 4.0, y + 18.0, 12.0, Align::Left, false, 0.9);
                text(cr, ink, &row.right_text, w - 4.0, y + 18.0, 11.0, Align::Right, false, 0.65);
                let total: usize = row.counts.iter().sum();
                cr.save().ok();
                rounded_rect(cr, bar_x, y + 5.0, track, 18.0, 6.0);
                cr.clip();
                ink.set(cr, 0.08);
                cr.rectangle(bar_x, y + 5.0, track, 18.0);
                let _ = cr.fill();
                let mut x = bar_x;
                for (segment, count) in row.counts.iter().enumerate() {
                    if *count == 0 || total == 0 {
                        continue;
                    }
                    let width = track * *count as f64 / total as f64;
                    let active = hover == Some(row_index * 3 + segment);
                    set_color(cr, STACK_COLORS[segment], if hover.is_none() || active { 1.0 } else { 0.5 });
                    cr.rectangle(x, y + 5.0, width + 0.5, 18.0);
                    let _ = cr.fill();
                    if width >= 34.0 {
                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                        let share = format!("{:.0}%", *count as f64 * 100.0 / total as f64);
                        cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
                        cr.set_font_size(10.5);
                        if let Ok(extents) = cr.text_extents(&share) {
                            cr.move_to(x + width / 2.0 - extents.width() / 2.0 - extents.x_bearing(), y + 18.0);
                            let _ = cr.show_text(&share);
                        }
                    }
                    x += width;
                }
                cr.restore().ok();
            }
        },
        move |x, y, w, _| {
            let row = ((y - 4.0) / STACK_ROW).floor();
            if row < 0.0 {
                return None;
            }
            let row = row as usize;
            let data = rows.get(row)?;
            let (bar_x, track) = layout(w);
            let total: usize = data.counts.iter().sum();
            if total == 0 || x < bar_x || x > bar_x + track {
                return None;
            }
            let mut start = bar_x;
            for (segment, count) in data.counts.iter().enumerate() {
                let width = track * *count as f64 / total as f64;
                if *count > 0 && x >= start && x <= start + width {
                    return Some((row * 3 + segment, data.tooltips[segment].clone()));
                }
                start += width;
            }
            None
        },
    )
}

// ---------------------------------------------------------------- heatmap ----

pub struct HeatRow {
    pub label: String,
    pub color: String,
}

pub struct HeatCell {
    /// Share of the column total, 0..=100.
    pub share: f64,
    pub text: String,
    pub tooltip: String,
}

pub fn heatmap(rows: Vec<HeatRow>, columns: Vec<String>, cells: Vec<Vec<HeatCell>>, description: &str) -> gtk::DrawingArea {
    let max_share = cells.iter().flatten().map(|cell| cell.share).fold(0.0, f64::max).max(1.0);
    let rows = Rc::new(rows);
    let cells = Rc::new(cells);
    let columns = Rc::new(columns);
    let cell_h = 26.0;
    let header = 26.0;
    let label_w = 112.0;
    let height = (header + rows.len() as f64 * cell_h + 6.0) as i32;
    let (draw_rows, draw_cells, draw_columns) = (rows.clone(), cells.clone(), columns.clone());
    let column_count = columns.len().max(1);
    interactive(
        description,
        300,
        height,
        move |cr, w, _, ink, hover| {
            let cell_w = ((w - label_w - 4.0) / column_count as f64).max(20.0);
            for (column, name) in draw_columns.iter().enumerate() {
                text(cr, ink, name, label_w + cell_w * (column as f64 + 0.5), 17.0, 11.0, Align::Center, true, 0.8);
            }
            for (row_index, row) in draw_rows.iter().enumerate() {
                let y = header + row_index as f64 * cell_h;
                set_color(cr, &row.color, 1.0);
                cr.arc(10.0, y + cell_h / 2.0, 4.5, 0.0, TAU);
                let _ = cr.fill();
                text(cr, ink, &row.label, 22.0, y + 17.0, 12.0, Align::Left, false, 0.9);
                for column in 0..column_count {
                    let Some(cell) = draw_cells.get(row_index).and_then(|cells| cells.get(column)) else { continue };
                    let x = label_w + column as f64 * cell_w;
                    let intensity = cell.share / max_share;
                    ink.set(cr, 0.05);
                    rounded_rect(cr, x + 1.5, y + 1.5, cell_w - 3.0, cell_h - 3.0, 5.0);
                    let _ = cr.fill();
                    if intensity > 0.0 {
                        set_color(cr, ACCENT, 0.12 + 0.88 * intensity);
                        rounded_rect(cr, x + 1.5, y + 1.5, cell_w - 3.0, cell_h - 3.0, 5.0);
                        let _ = cr.fill();
                    }
                    if hover == Some(row_index * column_count + column) {
                        ink.set(cr, 0.7);
                        cr.set_line_width(1.5);
                        rounded_rect(cr, x + 1.5, y + 1.5, cell_w - 3.0, cell_h - 3.0, 5.0);
                        let _ = cr.stroke();
                    }
                    let on_dark = intensity > 0.55;
                    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
                    cr.set_font_size(10.5);
                    if on_dark {
                        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                    } else {
                        ink.set(cr, 0.8);
                    }
                    if let Ok(extents) = cr.text_extents(&cell.text) {
                        cr.move_to(x + cell_w / 2.0 - extents.width() / 2.0 - extents.x_bearing(), y + 17.0);
                        let _ = cr.show_text(&cell.text);
                    }
                }
            }
        },
        move |x, y, w, _| {
            let cell_w = ((w - label_w - 4.0) / column_count as f64).max(20.0);
            if x < label_w || y < header {
                return None;
            }
            let column = ((x - label_w) / cell_w).floor() as usize;
            let row = ((y - header) / cell_h).floor() as usize;
            let cell = cells.get(row)?.get(column)?;
            Some((row * column_count + column, cell.tooltip.clone()))
        },
    )
}

// ------------------------------------------------------------------ radar ----

pub struct RadarPoint {
    pub label: String,
    /// 0..=100.
    pub value: f64,
    pub tooltip: String,
}

fn radar_geometry(w: f64, h: f64) -> (f64, f64, f64) {
    (w / 2.0, h / 2.0 + 4.0, (w.min(h) / 2.0 - 46.0).max(30.0))
}

fn radar_vertex(index: usize, count: usize, value: f64, cx: f64, cy: f64, radius: f64) -> (f64, f64) {
    let angle = -PI / 2.0 + TAU * index as f64 / count as f64;
    let distance = radius * value.clamp(0.0, 100.0) / 100.0;
    (cx + distance * angle.cos(), cy + distance * angle.sin())
}

pub fn radar(points: Vec<RadarPoint>, description: &str) -> gtk::DrawingArea {
    let points = Rc::new(points);
    let count = points.len();
    let (draw_points, hit_points) = (points.clone(), points);
    interactive(
        description,
        260,
        280,
        move |cr, w, h, ink, hover| {
            let (cx, cy, radius) = radar_geometry(w, h);
            cr.set_line_width(1.0);
            for ring in 1..=4 {
                let value = f64::from(ring) * 25.0;
                for index in 0..=count {
                    let (x, y) = radar_vertex(index % count, count, value, cx, cy, radius);
                    if index == 0 { cr.move_to(x, y) } else { cr.line_to(x, y) }
                }
                ink.set(cr, 0.12);
                let _ = cr.stroke();
            }
            for index in 0..count {
                let (x, y) = radar_vertex(index, count, 100.0, cx, cy, radius);
                ink.set(cr, 0.12);
                cr.move_to(cx, cy);
                cr.line_to(x, y);
                let _ = cr.stroke();
                let (lx, ly) = radar_vertex(index, count, 100.0, cx, cy, radius + 14.0);
                let align = if (lx - cx).abs() < 8.0 {
                    Align::Center
                } else if lx > cx {
                    Align::Left
                } else {
                    Align::Right
                };
                let label = fit(cr, &draw_points[index].label, 11.0, (w / 2.0 - 8.0).max(60.0));
                text(cr, ink, &label, lx, ly + 4.0, 11.0, align, hover == Some(index), 0.9);
            }
            for index in 0..=count {
                let (x, y) = radar_vertex(index % count, count, draw_points[index % count].value, cx, cy, radius);
                if index == 0 { cr.move_to(x, y) } else { cr.line_to(x, y) }
            }
            set_color(cr, ACCENT, 0.25);
            let _ = cr.fill_preserve();
            set_color(cr, ACCENT, 1.0);
            cr.set_line_width(2.2);
            let _ = cr.stroke();
            for (index, point) in draw_points.iter().enumerate() {
                let (x, y) = radar_vertex(index, count, point.value, cx, cy, radius);
                set_color(cr, ACCENT, 1.0);
                cr.arc(x, y, if hover == Some(index) { 6.0 } else { 4.0 }, 0.0, TAU);
                let _ = cr.fill();
            }
        },
        move |x, y, w, h| {
            let (cx, cy, radius) = radar_geometry(w, h);
            let nearest = (0..count).min_by(|a, b| {
                let da = {
                    let (px, py) = radar_vertex(*a, count, 100.0, cx, cy, radius + 10.0);
                    (px - x).hypot(py - y)
                };
                let db = {
                    let (px, py) = radar_vertex(*b, count, 100.0, cx, cy, radius + 10.0);
                    (px - x).hypot(py - y)
                };
                da.total_cmp(&db)
            })?;
            let (px, py) = radar_vertex(nearest, count, 100.0, cx, cy, radius + 10.0);
            ((px - x).hypot(py - y) < 70.0).then(|| (nearest, hit_points[nearest].tooltip.clone()))
        },
    )
}
