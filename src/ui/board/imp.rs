use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};
use shakmaty::{Board, Chess, Color, Move, Piece, Position, Role, Square};

use super::animation::{MoveAnimation, Pulse, SlidingPiece, ease_out_back, ease_out_cubic};
use super::checkmate_animation::{CheckmateAnimation, CheckmateConfig, ease_in_cubic};
use super::draw_animation::{DrawAnimation, DrawConfig};
use super::geometry::BoardGeometry;
use crate::assets::{PieceStyle, leather_texture_rgba, render_piece_rgba};
use crate::chess::{DrawColor, Drawings, Game, UserArrow, is_promotion_square, knight_elbow, premove_destinations};
use crate::settings::{CoordinateMode, Rgba};
use crate::ui::art::{ROLES, texture_from_rgba};

pub type MoveCallback = Rc<dyn Fn(Square, Square, Option<Role>)>;

pub const PROMOTION_ORDER: [Role; 4] = [Role::Queen, Role::Knight, Role::Rook, Role::Bishop];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardAppearance {
    pub light: Rgba,
    pub dark: Rgba,
    pub texture: bool,
    pub style: PieceStyle,
    pub coordinates: CoordinateMode,
    pub show_legal: bool,
    pub highlight_last: bool,
    pub animation_ms: u64,
    pub auto_queen: bool,
    pub art_revision: u64,
}

impl Default for BoardAppearance {
    fn default() -> Self {
        Self {
            light: Rgba::rgb(0xec, 0xec, 0xe9),
            dark: Rgba::rgb(0x36, 0x69, 0x4d),
            texture: true,
            style: PieceStyle::Staunton,
            coordinates: CoordinateMode::Inside,
            show_legal: true,
            highlight_last: true,
            animation_ms: 190,
            auto_queen: false,
            art_revision: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragState {
    pub from: Square,
    pub start: (f64, f64),
    pub pointer: (f64, f64),
    pub active: bool,
    pub was_selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionPicker {
    pub from: Square,
    pub to: Square,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationHint {
    Forward(Move),
    Backward(Move),
}

/// Extra emphasis for a standout move: its squares take the class color and
/// a small label pops up before the class icon appears.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerAccent {
    pub label: Option<String>,
    pub color: gdk::RGBA,
}

/// How long the label stays fully visible, and how long it takes to fade
/// into the class icon.
const ACCENT_HOLD_MS: f64 = 1000.0;
const ACCENT_FADE_MS: f64 = 300.0;
const ACCENT_POP_MS: f64 = 160.0;

#[derive(Default)]
pub struct BoardState {
    pub position: Chess,
    pub last_move: Option<(Square, Square)>,
    pub check: Option<Square>,
    pub mate: Option<Square>,
    pub movable: [bool; 2],
    pub selected: Option<Square>,
    pub targets: Vec<Move>,
    pub drag: Option<DragState>,
    pub hover: Option<Square>,
    pub promotion: Option<PromotionPicker>,
    pub animation: Option<MoveAnimation>,
    pub select_pulse: Option<Pulse>,
    pub checkmate: Option<CheckmateAnimation>,
    pub draw: Option<DrawAnimation>,
    pub flipped: bool,
    pub appearance: BoardAppearance,
    pub suppress_next_slide: bool,
    pub premove_color: Option<Color>,
    pub premove: Option<(Square, Square, Option<Role>)>,
    pub marker: Option<(Square, gdk::Texture)>,
    pub accent: Option<MarkerAccent>,
    pub accent_pulse: Option<Pulse>,
    pub arrow: Option<(Square, Square)>,
    pub drawings: Drawings,
    /// Arrow being drawn with the right mouse button: start, current square, color.
    pub draw_preview: Option<(Square, Square, DrawColor)>,
}


#[derive(Default)]
pub struct ArtCache {
    pub key: Option<ArtKey>,
    pub textures: HashMap<(Role, Color), gdk::Texture>,
    pub in_flight: Option<ArtKey>,
    pub wanted: Option<ArtKey>,
}

pub type ArtKey = (u32, PieceStyle, u64);

#[derive(Default)]
pub struct BoardWidget {
    pub state: RefCell<BoardState>,
    pub art: RefCell<ArtCache>,
    pub leather: RefCell<Option<gdk::Texture>>,
    pub tick: RefCell<Option<gtk::TickCallbackId>>,
    pub on_move: RefCell<Option<MoveCallback>>,
    pub on_premove: RefCell<Option<MoveCallback>>,
    pub geometry: Cell<Option<BoardGeometry>>,
    pub arrow_start: Cell<Option<Square>>,
    pub arrow_origin: Cell<(f64, f64)>,
    pub checkmate_frame: Cell<Option<i64>>,
}

#[glib::object_subclass]
impl ObjectSubclass for BoardWidget {
    const NAME: &'static str = "KgBoardWidget";
    type Type = super::BoardWidget;
    type ParentType = gtk::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_css_name("chessboard");
        klass.set_accessible_role(gtk::AccessibleRole::Img);
    }
}

fn rgba(color: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(color.red, color.green, color.blue, color.alpha)
}

fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

impl ObjectImpl for BoardWidget {
    fn constructed(&self) {
        self.parent_constructed();
        let widget = self.obj();
        widget.set_focusable(true);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.update_property(&[gtk::accessible::Property::Label("Chess board")]);

        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let weak = widget.downgrade();
        drag.connect_drag_begin(move |gesture, x, y| {
            if let Some(widget) = weak.upgrade() {
                widget.grab_focus();
                if !widget.imp().press(x, y) {
                    gesture.set_state(gtk::EventSequenceState::Denied);
                }
            }
        });
        let weak = widget.downgrade();
        drag.connect_drag_update(move |gesture, dx, dy| {
            if let (Some(widget), Some((sx, sy))) = (weak.upgrade(), gesture.start_point()) {
                widget.imp().drag_to(sx + dx, sy + dy);
            }
        });
        let weak = widget.downgrade();
        drag.connect_drag_end(move |gesture, dx, dy| {
            if let (Some(widget), Some((sx, sy))) = (weak.upgrade(), gesture.start_point()) {
                widget.imp().release(sx + dx, sy + dy);
            }
        });
        widget.add_controller(drag);

        let secondary = gtk::GestureDrag::new();
        secondary.set_button(gdk::BUTTON_SECONDARY);
        let weak = widget.downgrade();
        secondary.connect_drag_begin(move |gesture, x, y| {
            if let Some(widget) = weak.upgrade() {
                widget.grab_focus();
                widget.imp().begin_drawing(x, y, gesture.current_event_state());
            }
        });
        let weak = widget.downgrade();
        secondary.connect_drag_update(move |gesture, dx, dy| {
            if let Some(widget) = weak.upgrade() {
                let (sx, sy) = widget.imp().arrow_origin.get();
                widget.imp().update_drawing(sx + dx, sy + dy, gesture.current_event_state());
            }
        });
        let weak = widget.downgrade();
        secondary.connect_drag_end(move |gesture, dx, dy| {
            if let Some(widget) = weak.upgrade() {
                let (sx, sy) = widget.imp().arrow_origin.get();
                widget.imp().finish_drawing(sx + dx, sy + dy, gesture.current_event_state());
            }
        });
        widget.add_controller(secondary);

        let motion = gtk::EventControllerMotion::new();
        let weak = widget.downgrade();
        motion.connect_motion(move |_, x, y| {
            if let Some(widget) = weak.upgrade() {
                widget.imp().update_cursor(x, y);
            }
        });
        widget.add_controller(motion);

        let keys = gtk::EventControllerKey::new();
        let weak = widget.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(widget) = weak.upgrade()
                && (widget.imp().has_pending_interaction() || widget.imp().has_drawings())
            {
                widget.imp().clear_drawings();
                widget.imp().cancel_interaction();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        widget.add_controller(keys);

        let (sender, receiver) = async_channel::bounded::<Vec<u8>>(1);
        std::thread::spawn(move || {
            if let Some(data) = leather_texture_rgba(1024, 0x6b6e_6967_6874) {
                let _ = sender.send_blocking(data);
            }
        });
        let weak = widget.downgrade();
        glib::spawn_future_local(async move {
            if let Ok(data) = receiver.recv().await
                && let Some(widget) = weak.upgrade()
            {
                widget
                    .imp()
                    .leather
                    .replace(Some(texture_from_rgba(data, 1024, 1024)));
                widget.queue_draw();
            }
        });
    }

    fn dispose(&self) {
        if let Some(tick) = self.tick.take() {
            tick.remove();
        }
        self.on_move.replace(None);
        self.on_premove.replace(None);
    }
}

impl WidgetImpl for BoardWidget {
    fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
        (200, 720, -1, -1)
    }

    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let widget = self.obj();
        let width = widget.width() as f32;
        let height = widget.height() as f32;
        if width < 16.0 || height < 16.0 {
            return;
        }
        let state = self.state.borrow();
        let frame_ratio = match state.appearance.coordinates {
            CoordinateMode::Border => 0.045,
            _ => 0.012,
        };
        let geometry = BoardGeometry::compute(width, height, state.flipped, frame_ratio);
        self.geometry.set(Some(geometry));
        let scale = self.device_scale();
        let wanted_pixels = (geometry.square as f64 * scale).ceil() as u32;
        drop(state);
        self.ensure_art(wanted_pixels);
        let state = self.state.borrow();
        let now = widget.frame_clock().map(|clock| clock.frame_time()).unwrap_or(0);
        self.draw_frame(snapshot, &geometry, &state);
        self.draw_squares(snapshot, &geometry, &state);
        self.draw_highlights(snapshot, &geometry, &state);
        self.draw_marks(snapshot, &geometry, &state);
        self.draw_coordinates(snapshot, &geometry, &state);
        self.draw_hints(snapshot, &geometry, &state, now);
        self.draw_pieces(snapshot, &geometry, &state, now);
        self.draw_arrows(snapshot, &geometry, &state);
        self.draw_marker(snapshot, &geometry, &state, now);
        self.draw_checkmate_overlay(snapshot, &geometry, &state);
        self.draw_draw_overlay(snapshot, &geometry, &state);
        self.draw_promotion(snapshot, &geometry, &state);
    }
}

impl BoardWidget {
    pub fn device_scale(&self) -> f64 {
        let widget = self.obj();
        widget
            .native()
            .and_then(|native| native.surface())
            .map(|surface| surface.scale())
            .unwrap_or_else(|| widget.scale_factor() as f64)
            .max(1.0)
    }

    fn ensure_art(&self, pixels: u32) {
        let pixels = pixels.clamp(16, 1024);
        let (style, revision) = {
            let state = self.state.borrow();
            (state.appearance.style, state.appearance.art_revision)
        };
        let key: ArtKey = (pixels, style, revision);
        let mut art = self.art.borrow_mut();
        if art.key == Some(key) {
            return;
        }
        if art.textures.is_empty() {
            let mut textures = HashMap::new();
            for color in [Color::White, Color::Black] {
                for role in ROLES {
                    if let Some(data) = render_piece_rgba(role, color, style, pixels) {
                        textures.insert((role, color), texture_from_rgba(data, pixels, pixels));
                    }
                }
            }
            art.textures = textures;
            art.key = Some(key);
            return;
        }
        art.wanted = Some(key);
        if art.in_flight.is_none() {
            art.in_flight = Some(key);
            drop(art);
            self.spawn_art_render(key);
        }
    }

    fn spawn_art_render(&self, key: ArtKey) {
        let (pixels, style, _) = key;
        let (sender, receiver) = async_channel::bounded::<Vec<(Role, Color, Vec<u8>)>>(1);
        std::thread::spawn(move || {
            let mut rendered = Vec::with_capacity(12);
            for color in [Color::White, Color::Black] {
                for role in ROLES {
                    if let Some(data) = render_piece_rgba(role, color, style, pixels) {
                        rendered.push((role, color, data));
                    }
                }
            }
            let _ = sender.send_blocking(rendered);
        });
        let weak = self.obj().downgrade();
        glib::spawn_future_local(async move {
            let Ok(rendered) = receiver.recv().await else { return };
            let Some(widget) = weak.upgrade() else { return };
            let imp = widget.imp();
            let next = {
                let mut art = imp.art.borrow_mut();
                art.textures = rendered
                    .into_iter()
                    .map(|(role, color, data)| ((role, color), texture_from_rgba(data, pixels, pixels)))
                    .collect();
                art.key = Some(key);
                art.in_flight = None;
                match art.wanted {
                    Some(wanted) if wanted != key => {
                        art.in_flight = Some(wanted);
                        Some(wanted)
                    }
                    _ => None,
                }
            };
            if let Some(next) = next {
                imp.spawn_art_render(next);
            }
            widget.queue_draw();
        });
    }

    fn piece_texture(&self, piece: Piece) -> Option<gdk::Texture> {
        self.art.borrow().textures.get(&(piece.role, piece.color)).cloned()
    }

    fn draw_frame(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let outer = geometry.outer_rect();
        let radius = (geometry.square * 0.08).clamp(3.0, 10.0) + geometry.frame * 0.4;
        let rounded = gsk::RoundedRect::from_rect(outer, radius);
        snapshot.append_outset_shadow(
            &rounded,
            &gdk::RGBA::new(0.0, 0.0, 0.0, 0.28),
            0.0,
            geometry.square * 0.05,
            0.0,
            geometry.square * 0.22,
        );
        let frame_color = state.appearance.dark.mix(Rgba::rgb(0x12, 0x14, 0x12), 0.55);
        snapshot.push_rounded_clip(&rounded);
        snapshot.append_color(&rgba(frame_color), &outer);
        if geometry.frame > 2.0 {
            let inner_glow = state.appearance.dark.mix(Rgba::rgb(255, 255, 255), 0.08);
            snapshot.append_border(
                &rounded,
                &[1.0; 4],
                &[rgba(inner_glow.with_alpha(0.5)); 4],
            );
        }
        snapshot.pop();
    }

    fn draw_squares(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let board_rect = geometry.board_rect();
        let light = rgba(state.appearance.light);
        let dark = rgba(state.appearance.dark);
        snapshot.append_color(&light, &board_rect);
        for row in 0..8u32 {
            for column in 0..8u32 {
                if (row + column) % 2 == 1 {
                    snapshot.append_color(&dark, &geometry.cell_rect(column as f32, row as f32));
                }
            }
        }
        if !state.appearance.texture {
            return;
        }
        let Some(leather) = self.leather.borrow().clone() else {
            return;
        };
        snapshot.append_texture(&leather, &board_rect);
        let attenuate = dark.with_alpha(0.55);
        for row in 0..8u32 {
            for column in 0..8u32 {
                if (row + column) % 2 == 1 {
                    snapshot.append_color(&attenuate, &geometry.cell_rect(column as f32, row as f32));
                }
            }
        }
    }

    fn draw_highlights(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let highlight = gdk::RGBA::new(1.0, 1.0, 53.0 / 255.0, 0.5);
        let accent = state
            .marker
            .as_ref()
            .zip(state.accent.as_ref())
            .map(|((square, _), accent)| (*square, accent.color.with_alpha(0.62)));
        let accent_covers_move =
            accent.is_some_and(|(square, _)| state.last_move.is_some_and(|(from, to)| from == square || to == square));
        if state.appearance.highlight_last
            && let Some((from, to)) = state.last_move
        {
            let color = match accent {
                Some((_, color)) if accent_covers_move => color,
                _ => highlight,
            };
            snapshot.append_color(&color, &geometry.square_rect(from));
            snapshot.append_color(&color, &geometry.square_rect(to));
        } else if let Some((square, color)) = accent {
            snapshot.append_color(&color, &geometry.square_rect(square));
        }
        if let Some((from, to, _)) = state.premove {
            let premove = gdk::RGBA::new(0.86, 0.30, 0.24, 0.55);
            snapshot.append_color(&premove, &geometry.square_rect(from));
            snapshot.append_color(&premove, &geometry.square_rect(to));
        }
        if let Some(selected) = state.selected {
            let color = if Self::is_premove_piece(state, selected) {
                gdk::RGBA::new(0.86, 0.30, 0.24, 0.45)
            } else {
                highlight
            };
            snapshot.append_color(&color, &geometry.square_rect(selected));
        }
        if let Some(square) = state.check.filter(|_| state.mate.is_none()) {
            let rect = geometry.square_rect(square);
            let center = graphene::Point::new(rect.x() + rect.width() / 2.0, rect.y() + rect.height() / 2.0);
            let stops = [
                gsk::ColorStop::new(0.0, gdk::RGBA::new(1.0, 0.0, 0.0, 1.0)),
                gsk::ColorStop::new(0.25, gdk::RGBA::new(0.91, 0.0, 0.0, 1.0)),
                gsk::ColorStop::new(0.89, gdk::RGBA::new(0.66, 0.0, 0.0, 0.0)),
                gsk::ColorStop::new(1.0, gdk::RGBA::new(0.62, 0.0, 0.0, 0.0)),
            ];
            snapshot.append_radial_gradient(
                &rect,
                &center,
                rect.width() * 0.62,
                rect.height() * 0.62,
                0.0,
                1.0,
                &stops,
            );
        }
        if let Some(draw) = state.draw.as_ref() {
            let tint = gdk::RGBA::new(0.33, 0.31, 0.29, 0.72 * draw.overlay() as f32);
            for king in draw.kings() {
                snapshot.append_color(&tint, &geometry.square_rect(king));
            }
        }
        if let Some(checkmate) = state.checkmate.as_ref() {
            let red = gdk::RGBA::new(0.878, 0.294, 0.294, checkmate.red_alpha() as f32);
            let green = gdk::RGBA::new(0.369, 0.620, 0.310, checkmate.green_alpha() as f32);
            snapshot.append_color(&red, &geometry.square_rect(checkmate.loser()));
            snapshot.append_color(&green, &geometry.square_rect(checkmate.winner()));
        }
        if let Some(drag) = state.drag.filter(|drag| drag.active)
            && let Some(hover) = state.hover
            && hover != drag.from
        {
            let rect = geometry.square_rect(hover);
            let width = (geometry.square * 0.05).max(2.0);
            snapshot.append_border(
                &gsk::RoundedRect::from_rect(rect, 0.0),
                &[width; 4],
                &[gdk::RGBA::new(1.0, 1.0, 1.0, 0.65); 4],
            );
        }
    }

    fn coordinate_layout(&self, text: &str, size: f32) -> pango::Layout {
        let layout = self.obj().create_pango_layout(Some(text));
        let mut description = pango::FontDescription::new();
        description.set_family("Ubuntu, Cantarell, Inter, sans-serif");
        description.set_weight(pango::Weight::Semibold);
        description.set_absolute_size(size as f64 * pango::SCALE as f64);
        layout.set_font_description(Some(&description));
        layout
    }

    fn draw_text(&self, snapshot: &gtk::Snapshot, layout: &pango::Layout, x: f32, y: f32, color: gdk::RGBA) {
        snapshot.save();
        snapshot.translate(&graphene::Point::new(x, y));
        snapshot.append_layout(layout, &color);
        snapshot.restore();
    }

    fn draw_coordinates(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let mode = state.appearance.coordinates;
        if mode == CoordinateMode::Hidden || geometry.square < 22.0 {
            return;
        }
        let light = rgba(state.appearance.light);
        let dark = rgba(state.appearance.dark);
        match mode {
            CoordinateMode::Inside => {
                let size = geometry.square * 0.19;
                let pad = geometry.square * 0.06;
                for row in 0..8u32 {
                    let square = geometry.display_square(0, row);
                    let layout = self.coordinate_layout(&square.rank().char().to_string(), size);
                    let rect = geometry.cell_rect(0.0, row as f32);
                    let color = if square.is_light() { dark } else { light };
                    self.draw_text(snapshot, &layout, rect.x() + pad, rect.y() + pad * 0.5, color);
                }
                for column in 0..8u32 {
                    let square = geometry.display_square(column, 7);
                    let layout = self.coordinate_layout(&square.file().char().to_string(), size);
                    let (text_width, text_height) = layout.pixel_size();
                    let rect = geometry.cell_rect(column as f32, 7.0);
                    let color = if square.is_light() { dark } else { light };
                    self.draw_text(
                        snapshot,
                        &layout,
                        rect.x() + rect.width() - text_width as f32 - pad,
                        rect.y() + rect.height() - text_height as f32 - pad * 0.3,
                        color,
                    );
                }
            }
            CoordinateMode::Border => {
                let size = (geometry.frame * 0.62).max(8.0);
                let color = rgba(state.appearance.light.with_alpha(0.92));
                for row in 0..8u32 {
                    let square = geometry.display_square(0, row);
                    let layout = self.coordinate_layout(&square.rank().char().to_string(), size);
                    let (text_width, text_height) = layout.pixel_size();
                    let rect = geometry.cell_rect(0.0, row as f32);
                    self.draw_text(
                        snapshot,
                        &layout,
                        geometry.origin_x - geometry.frame / 2.0 - text_width as f32 / 2.0,
                        rect.y() + (rect.height() - text_height as f32) / 2.0,
                        color,
                    );
                }
                for column in 0..8u32 {
                    let square = geometry.display_square(column, 7);
                    let layout = self.coordinate_layout(&square.file().char().to_string(), size);
                    let (text_width, text_height) = layout.pixel_size();
                    let rect = geometry.cell_rect(column as f32, 7.0);
                    self.draw_text(
                        snapshot,
                        &layout,
                        rect.x() + (rect.width() - text_width as f32) / 2.0,
                        geometry.origin_y + geometry.board_size() + (geometry.frame - text_height as f32) / 2.0,
                        color,
                    );
                }
            }
            CoordinateMode::Hidden => {}
        }
    }

    fn draw_hints(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState, now: i64) {
        if !state.appearance.show_legal || state.selected.is_none() || state.promotion.is_some() {
            return;
        }
        let fade = state
            .select_pulse
            .map(|pulse| ease_out_cubic(pulse.progress(now)))
            .unwrap_or(1.0) as f32;
        let board = state.position.board();
        let mut seen = Vec::with_capacity(state.targets.len());
        for mv in &state.targets {
            let target = Game::destination_of(mv);
            if seen.contains(&target) {
                continue;
            }
            seen.push(target);
            let rect = geometry.square_rect(target);
            let occupied = board.piece_at(target).is_some() || matches!(mv, Move::EnPassant { .. });
            let hovered = state.hover == Some(target) && state.drag.is_some_and(|d| d.active);
            if hovered {
                continue;
            }
            if occupied {
                let inset = geometry.square * 0.02;
                let ring = graphene::Rect::new(
                    rect.x() + inset,
                    rect.y() + inset,
                    rect.width() - inset * 2.0,
                    rect.height() - inset * 2.0,
                );
                let rounded = gsk::RoundedRect::from_rect(ring, ring.width() / 2.0);
                let width = geometry.square * 0.085;
                snapshot.append_border(
                    &rounded,
                    &[width; 4],
                    &[gdk::RGBA::new(0.0, 0.0, 0.0, 0.17 * fade); 4],
                );
            } else {
                let radius = geometry.square * 0.16 * (0.6 + 0.4 * fade);
                let center_x = rect.x() + rect.width() / 2.0;
                let center_y = rect.y() + rect.height() / 2.0;
                let dot = graphene::Rect::new(center_x - radius, center_y - radius, radius * 2.0, radius * 2.0);
                snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(dot, radius));
                snapshot.append_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.17 * fade), &dot);
                snapshot.pop();
            }
        }
    }

    fn append_piece(&self, snapshot: &gtk::Snapshot, piece: Piece, rect: &graphene::Rect, scale: f32) {
        let Some(texture) = self.piece_texture(piece) else {
            return;
        };
        if (scale - 1.0).abs() < f32::EPSILON {
            snapshot.append_texture(&texture, rect);
        } else {
            let width = rect.width() * scale;
            let height = rect.height() * scale;
            let scaled = graphene::Rect::new(
                rect.x() + (rect.width() - width) / 2.0,
                rect.y() + (rect.height() - height) / 2.0,
                width,
                height,
            );
            snapshot.append_texture(&texture, &scaled);
        }
    }

    fn draw_pieces(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState, now: i64) {
        let board: &Board = state.position.board();
        let animation = state.animation.as_ref().filter(|animation| !animation.is_finished(now));
        let hidden_targets: Vec<Square> = animation
            .map(|animation| animation.sliding.iter().map(|slide| slide.to).collect())
            .unwrap_or_default();
        let dragged = state.drag.filter(|drag| drag.active).map(|drag| drag.from);
        if let Some(animation) = animation {
            let progress = animation.progress(now);
            for (piece, square) in &animation.fading {
                let rect = geometry.square_rect(*square);
                snapshot.push_opacity(1.0 - ease_out_cubic(progress));
                self.append_piece(snapshot, *piece, &rect, 1.0 - 0.25 * progress as f32);
                snapshot.pop();
            }
        }
        for (square, piece) in board.iter() {
            if hidden_targets.contains(&square) || dragged == Some(square) {
                continue;
            }
            if let Some(picker) = state.promotion
                && picker.from == square
            {
                continue;
            }
            let scale = state
                .checkmate
                .as_ref()
                .filter(|checkmate| checkmate.loser() == square)
                .map_or(1.0, |checkmate| checkmate.king_scale() as f32);
            let dim = state
                .draw
                .as_ref()
                .filter(|draw| draw.kings().contains(&square))
                .map_or(0.0, |draw| draw.overlay());
            if dim > 0.0 {
                snapshot.push_opacity(1.0 - 0.6 * dim);
                self.append_piece(snapshot, piece, &geometry.square_rect(square), scale);
                snapshot.pop();
            } else {
                self.append_piece(snapshot, piece, &geometry.square_rect(square), scale);
            }
        }
        if let Some(animation) = animation {
            let eased = ease_out_cubic(animation.progress(now)) as f32;
            for slide in &animation.sliding {
                let (from_column, from_row) = geometry.cell_of(slide.from);
                let (to_column, to_row) = geometry.cell_of(slide.to);
                let column = from_column + (to_column - from_column) * eased;
                let row = from_row + (to_row - from_row) * eased;
                self.append_piece(snapshot, slide.piece, &geometry.cell_rect(column, row), 1.0);
            }
        }
        if let Some(drag) = state.drag.filter(|drag| drag.active)
            && let Some(piece) = board.piece_at(drag.from)
        {
            let size = geometry.square;
            let rect = graphene::Rect::new(
                drag.pointer.0 as f32 - size / 2.0,
                drag.pointer.1 as f32 - size / 2.0,
                size,
                size,
            );
            self.append_piece(snapshot, piece, &rect, 1.1);
        }
    }

    fn draw_arrows(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        if let Some((from, to)) = state.arrow {
            self.draw_arrow_shape(snapshot, geometry, from, to, gdk::RGBA::new(0.0, 0.72, 0.94, 0.8));
        }
        for arrow in &state.drawings.arrows {
            let (r, g, b, a) = arrow.color.rgba();
            self.draw_arrow_shape(snapshot, geometry, arrow.from, arrow.to, gdk::RGBA::new(r, g, b, a));
        }
        if let Some((from, to, color)) = state.draw_preview
            && from != to
        {
            let (r, g, b, a) = color.rgba();
            self.draw_arrow_shape(snapshot, geometry, from, to, gdk::RGBA::new(r, g, b, a * 0.6));
        }
    }

    /// Draws an arrow between two squares as one flat shape: a square-cut
    /// tail that starts clear of the piece, a straight shaft and a triangular
    /// head whose tip sits on the centre of the target square. Knight moves
    /// are drawn as an "L": the long leg first, then a sharp turn.
    fn draw_arrow_shape(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, from: Square, to: Square, color: gdk::RGBA) {
        if from == to {
            return;
        }
        let center = |square: Square| {
            let rect = geometry.square_rect(square);
            (rect.x() + rect.width() / 2.0, rect.y() + rect.height() / 2.0)
        };
        let unit = |from: (f32, f32), to: (f32, f32)| {
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let length = (dx * dx + dy * dy).sqrt().max(1.0);
            (dx / length, dy / length)
        };
        let square = geometry.square;
        let half_shaft = square * 0.11;
        let half_head = square * 0.26;
        let head_length = square * 0.36;
        let tail_gap = square * 0.36;

        let a = center(from);
        let tip = center(to);
        let corner = knight_elbow(from, to).map(center);
        // Direction of the first leg and of the leg that carries the head.
        let first = unit(a, corner.unwrap_or(tip));
        let last = unit(corner.unwrap_or(a), tip);
        let start = (a.0 + first.0 * tail_gap, a.1 + first.1 * tail_gap);
        let neck = (tip.0 - last.0 * head_length, tip.1 - last.1 * head_length);
        let first_normal = (-first.1, first.0);
        let last_normal = (-last.1, last.0);
        let offset = |point: (f32, f32), normal: (f32, f32), distance: f32| {
            (point.0 + normal.0 * distance, point.1 + normal.1 * distance)
        };
        // Where the two shaft edges meet at the turn of an "L".
        let miter = corner.map(|corner| {
            let dot = first_normal.0 * last_normal.0 + first_normal.1 * last_normal.1;
            let scale = half_shaft / (1.0 + dot).max(0.2);
            let shift = ((first_normal.0 + last_normal.0) * scale, (first_normal.1 + last_normal.1) * scale);
            ((corner.0 + shift.0, corner.1 + shift.1), (corner.0 - shift.0, corner.1 - shift.1))
        });

        let mut outline = vec![offset(start, first_normal, half_shaft)];
        outline.extend(miter.map(|(left, _)| left));
        outline.push(offset(neck, last_normal, half_shaft));
        outline.push(offset(neck, last_normal, half_head));
        outline.push(tip);
        outline.push(offset(neck, last_normal, -half_head));
        outline.push(offset(neck, last_normal, -half_shaft));
        outline.extend(miter.map(|(_, right)| right));
        outline.push(offset(start, first_normal, -half_shaft));

        let builder = gsk::PathBuilder::new();
        builder.move_to(outline[0].0, outline[0].1);
        for point in &outline[1..] {
            builder.line_to(point.0, point.1);
        }
        builder.close();
        snapshot.append_fill(&builder.to_path(), gsk::FillRule::Winding, &color);
    }

    fn draw_marks(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        for (square, color) in &state.drawings.marks {
            let rect = geometry.square_rect(*square);
            let inset = geometry.square * 0.05;
            let ring = graphene::Rect::new(
                rect.x() + inset,
                rect.y() + inset,
                rect.width() - inset * 2.0,
                rect.height() - inset * 2.0,
            );
            let rounded = gsk::RoundedRect::from_rect(ring, ring.width() / 2.0);
            let (r, g, b, a) = color.rgba();
            let tint = gdk::RGBA::new(r, g, b, a);
            snapshot.append_border(&rounded, &[(geometry.square * 0.07).max(2.0); 4], &[tint; 4]);
        }
    }

    fn draw_marker(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState, now: i64) {
        let Some((square, texture)) = state.marker.as_ref() else { return };
        let rect = geometry.square_rect(*square);
        let size = geometry.square * 0.38;
        let board = geometry.board_rect();
        let x = (rect.x() + rect.width() - size * 0.72).min(board.x() + board.width() - size);
        let y = (rect.y() - size * 0.28).max(board.y());
        let icon = graphene::Rect::new(x, y, size, size);
        let popup = state
            .accent
            .as_ref()
            .and_then(|accent| accent.label.as_deref().map(|label| (accent, label)))
            .zip(state.accent_pulse);
        let Some(((accent, label), pulse)) = popup else {
            snapshot.append_texture(texture, &icon);
            return;
        };
        let elapsed = (now - pulse.start_us) as f64 / 1000.0;
        let fade = ((elapsed - ACCENT_HOLD_MS) / ACCENT_FADE_MS).clamp(0.0, 1.0);
        if fade > 0.0 {
            let grown = size * ease_out_back(fade) as f32;
            let inset = (size - grown) / 2.0;
            snapshot.push_opacity(fade);
            snapshot.append_texture(texture, &graphene::Rect::new(x + inset, y + inset, grown, grown));
            snapshot.pop();
        }
        if fade >= 1.0 {
            return;
        }
        let pop = (ease_out_back(elapsed / ACCENT_POP_MS) * (1.0 - ease_in_cubic(fade))) as f32;
        if pop <= 0.01 {
            return;
        }
        let layout = self.coordinate_layout(&format!("{label}!"), geometry.square * 0.24);
        let mut description = layout.font_description().unwrap_or_default();
        description.set_weight(pango::Weight::Bold);
        layout.set_font_description(Some(&description));
        let (text_width, text_height) = layout.pixel_size();
        let (text_width, text_height) = (text_width as f32, text_height as f32);
        let pad_x = geometry.square * 0.1;
        let pad_y = geometry.square * 0.045;
        let (width, height) = (text_width + pad_x * 2.0, text_height + pad_y * 2.0);
        let left = (rect.x() + rect.width() * 0.5 - width * 0.2)
            .min(board.x() + board.width() - width)
            .max(board.x());
        let top = (rect.y() - height * 0.55).max(board.y());
        let center = graphene::Point::new(left + width / 2.0, top + height / 2.0);
        snapshot.push_opacity(1.0 - fade);
        snapshot.save();
        snapshot.translate(&center);
        snapshot.scale(pop, pop);
        snapshot.translate(&graphene::Point::new(-center.x(), -center.y()));
        let bubble = graphene::Rect::new(left, top, width, height);
        let rounded = gsk::RoundedRect::from_rect(bubble, height * 0.28);
        snapshot.append_outset_shadow(
            &rounded,
            &gdk::RGBA::new(0.0, 0.0, 0.0, 0.35),
            0.0,
            height * 0.08,
            0.0,
            height * 0.28,
        );
        snapshot.push_rounded_clip(&rounded);
        snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &bubble);
        snapshot.pop();
        self.draw_text(snapshot, &layout, left + pad_x, top + pad_y, accent.color);
        snapshot.restore();
        snapshot.pop();
    }

    /// Sets the class icon for a square, optionally with an accent. The
    /// label animation only restarts when the accented move changes.
    pub fn set_marker(&self, marker: Option<(Square, gdk::Texture)>, accent: Option<MarkerAccent>) {
        // The frame clock only advances while the board is being redrawn, so
        // its time can be seconds old here and would cut the label short.
        let now = glib::monotonic_time();
        let mut state = self.state.borrow_mut();
        let accent = accent.filter(|_| marker.is_some());
        let unchanged = accent == state.accent
            && marker.as_ref().map(|(square, _)| *square) == state.marker.as_ref().map(|(square, _)| *square);
        state.marker = marker;
        if !unchanged {
            state.accent_pulse = accent
                .as_ref()
                .filter(|accent| accent.label.is_some())
                .map(|_| Pulse::new(now, (ACCENT_HOLD_MS + ACCENT_FADE_MS) as i64));
            state.accent = accent;
        }
        let animate = state.accent_pulse.is_some();
        drop(state);
        if animate {
            self.ensure_ticking();
        }
        self.obj().queue_draw();
    }

    fn draw_badge_disc(&self, snapshot: &gtk::Snapshot, center: (f32, f32), diameter: f32, fill: gdk::RGBA) {
        let disc = graphene::Rect::new(center.0 - diameter / 2.0, center.1 - diameter / 2.0, diameter, diameter);
        let rounded = gsk::RoundedRect::from_rect(disc, diameter / 2.0);
        snapshot.append_outset_shadow(
            &rounded,
            &gdk::RGBA::new(0.0, 0.0, 0.0, 0.35),
            0.0,
            diameter * 0.06,
            0.0,
            diameter * 0.18,
        );
        snapshot.push_rounded_clip(&rounded);
        snapshot.append_color(&fill, &disc);
        snapshot.pop();
        snapshot.append_border(
            &rounded,
            &[(diameter * 0.07).max(1.0); 4],
            &[gdk::RGBA::new(1.0, 1.0, 1.0, 0.95); 4],
        );
    }

    fn draw_draw_overlay(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let Some(draw) = state.draw.as_ref() else { return };
        let board = geometry.board_rect();
        let square = geometry.square;
        let white = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
        let ink = gdk::RGBA::new(0.24, 0.22, 0.21, 1.0);
        let half = |size: f32| {
            let layout = self.coordinate_layout("½", size);
            let mut description = layout.font_description().unwrap_or_default();
            description.set_weight(pango::Weight::Heavy);
            layout.set_font_description(Some(&description));
            layout
        };
        let overlay = draw.overlay();
        let grow = draw.overlay_scale() as f32;
        if overlay > 0.0 && grow > 0.01 {
            snapshot.push_opacity(overlay);
            for king in draw.kings() {
                let rect = geometry.square_rect(king);
                let center = graphene::Point::new(rect.x() + rect.width() / 2.0, rect.y() + rect.height() / 2.0);
                snapshot.save();
                snapshot.translate(&center);
                snapshot.scale(grow, grow);
                snapshot.translate(&graphene::Point::new(-center.x(), -center.y()));
                let glyph = half(square * 0.5);
                let (width, height) = glyph.pixel_size();
                let origin = (center.x() - width as f32 / 2.0, center.y() - height as f32 / 2.0 + square * 0.04);
                let outline = (square * 0.035).max(1.0);
                for (dx, dy) in [
                    (-1.0, 0.0),
                    (1.0, 0.0),
                    (0.0, -1.0),
                    (0.0, 1.0),
                    (-0.7, -0.7),
                    (0.7, -0.7),
                    (-0.7, 0.7),
                    (0.7, 0.7),
                ] {
                    self.draw_text(snapshot, &glyph, origin.0 + dx * outline, origin.1 + dy * outline, ink);
                }
                self.draw_text(snapshot, &glyph, origin.0, origin.1, white);
                let label = self.coordinate_layout("Draw", square * 0.2);
                let mut description = label.font_description().unwrap_or_default();
                description.set_weight(pango::Weight::Bold);
                label.set_font_description(Some(&description));
                let (text_width, text_height) = label.pixel_size();
                let (pill_width, pill_height) = (text_width as f32 + square * 0.24, text_height as f32 + square * 0.08);
                let pill_x = (center.x() - pill_width / 2.0).clamp(board.x(), board.x() + board.width() - pill_width);
                let pill_y = (rect.y() - pill_height * 0.55).max(board.y());
                let pill = graphene::Rect::new(pill_x, pill_y, pill_width, pill_height);
                let rounded = gsk::RoundedRect::from_rect(pill, pill_height / 2.0);
                snapshot.append_outset_shadow(
                    &rounded,
                    &gdk::RGBA::new(0.0, 0.0, 0.0, 0.3),
                    0.0,
                    pill_height * 0.08,
                    0.0,
                    pill_height * 0.3,
                );
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(&white, &pill);
                snapshot.pop();
                self.draw_text(
                    snapshot,
                    &label,
                    pill_x + (pill_width - text_width as f32) / 2.0,
                    pill_y + (pill_height - text_height as f32) / 2.0,
                    ink,
                );
                snapshot.restore();
            }
            snapshot.pop();
        }
        let full = square * 0.38;
        let diameter = full * draw.badge_scale() as f32;
        if diameter > 0.5 {
            for king in draw.kings() {
                let rect = geometry.square_rect(king);
                let center = (
                    (rect.x() + rect.width() - full * 0.22).min(board.x() + board.width() - full / 2.0),
                    (rect.y() + full * 0.22).max(board.y() + full / 2.0),
                );
                self.draw_badge_disc(snapshot, center, diameter, gdk::RGBA::new(0.36, 0.35, 0.34, 1.0));
                let glyph = half(diameter * 0.62);
                let (width, height) = glyph.pixel_size();
                self.draw_text(snapshot, &glyph, center.0 - width as f32 / 2.0, center.1 - height as f32 / 2.0, white);
            }
        }
    }

    fn draw_checkmate_overlay(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let Some(checkmate) = state.checkmate.as_ref() else { return };
        let white = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
        let board = geometry.board_rect();
        let full = geometry.square * 0.38;
        let corner = |square: Square| {
            let rect = geometry.square_rect(square);
            (
                (rect.x() + rect.width() - full * 0.22).min(board.x() + board.width() - full / 2.0),
                (rect.y() + full * 0.22).max(board.y() + full / 2.0),
            )
        };
        let diameter = full * checkmate.badge_scale() as f32;
        let badge_alpha = checkmate.badge_alpha();
        if diameter > 0.5 && badge_alpha > 0.0 {
            snapshot.push_opacity(badge_alpha);
            let marker = corner(checkmate.loser());
            self.draw_badge_disc(snapshot, marker, diameter, gdk::RGBA::new(0.84, 0.16, 0.16, 1.0));
            let hash = gsk::PathBuilder::new();
            let (reach, gap, lean) = (diameter * 0.24, diameter * 0.1, diameter * 0.05);
            for side in [-1.0f32, 1.0] {
                hash.move_to(marker.0 + side * gap + lean, marker.1 - reach);
                hash.line_to(marker.0 + side * gap - lean, marker.1 + reach);
                hash.move_to(marker.0 - reach, marker.1 + side * gap);
                hash.line_to(marker.0 + reach, marker.1 + side * gap);
            }
            let stroke = gsk::Stroke::new((diameter * 0.09).max(1.2));
            stroke.set_line_cap(gsk::LineCap::Round);
            snapshot.append_stroke(&hash.to_path(), &stroke, &white);
            let star_center = corner(checkmate.winner());
            self.draw_badge_disc(snapshot, star_center, diameter, gdk::RGBA::new(0.369, 0.620, 0.310, 1.0));
            let star = gsk::PathBuilder::new();
            for point in 0..10 {
                let radius = if point % 2 == 0 { diameter * 0.32 } else { diameter * 0.13 };
                let angle = std::f32::consts::PI * (point as f32 / 5.0 - 0.5);
                let (x, y) = (star_center.0 + radius * angle.cos(), star_center.1 + radius * angle.sin());
                if point == 0 {
                    star.move_to(x, y);
                } else {
                    star.line_to(x, y);
                }
            }
            star.close();
            snapshot.append_fill(&star.to_path(), gsk::FillRule::Winding, &white);
            snapshot.pop();
        }
    }

    pub fn promotion_cells(&self, geometry: &BoardGeometry, picker: &PromotionPicker) -> Vec<(Role, graphene::Rect)> {
        let (column, row) = geometry.cell_of(picker.to);
        let downward = row < 4.0;
        PROMOTION_ORDER
            .iter()
            .enumerate()
            .map(|(index, role)| {
                let offset = index as f32;
                let cell_row = if downward { row + offset } else { row - offset };
                (*role, geometry.cell_rect(column, cell_row))
            })
            .collect()
    }

    fn draw_promotion(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let Some(picker) = state.promotion else { return };
        snapshot.append_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.45), &geometry.board_rect());
        let cells = self.promotion_cells(geometry, &picker);
        let first = cells.first().map(|(_, rect)| *rect);
        let last = cells.last().map(|(_, rect)| *rect);
        if let (Some(first), Some(last)) = (first, last) {
            let top = first.y().min(last.y());
            let bottom = (first.y() + first.height()).max(last.y() + last.height());
            let column = graphene::Rect::new(first.x(), top, first.width(), bottom - top);
            let rounded = gsk::RoundedRect::from_rect(column, geometry.square * 0.08);
            snapshot.append_outset_shadow(
                &rounded,
                &gdk::RGBA::new(0.0, 0.0, 0.0, 0.5),
                0.0,
                geometry.square * 0.04,
                0.0,
                geometry.square * 0.25,
            );
            snapshot.push_rounded_clip(&rounded);
            snapshot.append_color(&gdk::RGBA::new(0.98, 0.97, 0.94, 1.0), &column);
            snapshot.pop();
        }
        for (role, rect) in cells {
            let hovered = state.hover.is_some_and(|hover| geometry.square_rect(hover) == rect);
            if hovered {
                snapshot.append_color(&gdk::RGBA::new(0.21, 0.41, 0.30, 0.25), &rect);
            }
            self.append_piece(
                snapshot,
                Piece {
                    color: picker.color,
                    role,
                },
                &rect,
                if hovered { 1.0 } else { 0.9 },
            );
        }
    }

    pub fn ensure_ticking(&self) {
        if self.tick.borrow().is_some() {
            return;
        }
        let widget = self.obj();
        let id = widget.add_tick_callback(|widget, clock| {
            let now = clock.frame_time();
            let imp = widget.imp();
            let mut state = imp.state.borrow_mut();
            if state.animation.as_ref().is_some_and(|animation| animation.is_finished(now)) {
                state.animation = None;
            }
            if state.select_pulse.is_some_and(|pulse| pulse.is_finished(now)) {
                state.select_pulse = None;
            }
            let elapsed_ms = imp.checkmate_frame.get().map_or(0.0, |last| (now - last) as f64 / 1000.0);
            let mut checkmate_active = false;
            if let Some(checkmate) = state.checkmate.as_mut()
                && !checkmate.is_finished()
            {
                checkmate.update(elapsed_ms);
                checkmate_active = !checkmate.is_finished();
                if !checkmate_active && !checkmate.config().persist_highlights_until_new_game {
                    state.checkmate = None;
                }
            }
            let mut draw_active = false;
            if let Some(draw) = state.draw.as_mut()
                && !draw.is_finished()
            {
                draw.update(elapsed_ms);
                draw_active = !draw.is_finished();
            }
            imp.checkmate_frame.set((checkmate_active || draw_active).then_some(now));
            if state.accent_pulse.is_some_and(|pulse| pulse.is_finished(now)) {
                state.accent_pulse = None;
            }
            let active = state.animation.is_some()
                || state.select_pulse.is_some()
                || checkmate_active
                || draw_active
                || state.accent_pulse.is_some();
            drop(state);
            widget.queue_draw();
            if active {
                glib::ControlFlow::Continue
            } else {
                imp.tick.replace(None);
                glib::ControlFlow::Break
            }
        });
        self.tick.replace(Some(id));
    }

    fn now(&self) -> i64 {
        self.obj()
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    pub fn start_checkmate(&self, loser: Square) {
        let started = {
            let mut state = self.state.borrow_mut();
            let board = state.position.board();
            let winner = state
                .last_move
                .map(|(_, to)| to)
                .filter(|to| *to != loser)
                .or_else(|| board.color_at(loser).and_then(|color| board.king_of(!color)));
            state.checkmate = winner.map(|winner| CheckmateAnimation::new(loser, winner, CheckmateConfig::default()));
            state.checkmate.is_some()
        };
        self.checkmate_frame.set(None);
        if started {
            self.ensure_ticking();
        }
    }

    pub fn set_draw(&self, drawn: bool) {
        let started = {
            let mut state = self.state.borrow_mut();
            if !drawn {
                state.draw = None;
                false
            } else if state.draw.is_some() {
                false
            } else {
                let board = state.position.board();
                let kings = board.king_of(Color::White).zip(board.king_of(Color::Black));
                state.draw = kings.map(|(white, black)| DrawAnimation::new([white, black], DrawConfig::default()));
                state.draw.is_some()
            }
        };
        if started {
            self.checkmate_frame.set(None);
            self.ensure_ticking();
        }
        self.obj().queue_draw();
    }

    fn is_premove_piece(state: &BoardState, square: Square) -> bool {
        let Some(piece) = state.position.board().piece_at(square) else {
            return false;
        };
        state.premove_color == Some(piece.color) && piece.color != state.position.turn()
    }

    fn can_move_piece(state: &BoardState, square: Square) -> bool {
        let Some(piece) = state.position.board().piece_at(square) else {
            return false;
        };
        (piece.color == state.position.turn() && state.movable[color_index(piece.color)])
            || Self::is_premove_piece(state, square)
    }

    fn select(&self, state: &mut BoardState, square: Square) {
        state.selected = Some(square);
        state.targets = if Self::is_premove_piece(state, square) {
            let role = state.position.board().piece_at(square).map(|p| p.role).unwrap_or(Role::Pawn);
            premove_destinations(state.position.board(), square)
                .into_iter()
                .map(|to| Move::Normal {
                    role,
                    from: square,
                    capture: None,
                    to,
                    promotion: None,
                })
                .collect()
        } else {
            state
                .position
                .legal_moves()
                .into_iter()
                .filter(|mv| mv.from() == Some(square))
                .collect()
        };
        state.select_pulse = Some(Pulse::new(self.now(), 140));
    }

    fn emit_premove(&self, from: Square, to: Square) {
        let promotion = {
            let mut state = self.state.borrow_mut();
            let promotion = is_promotion_square(state.position.board(), from, to).then_some(Role::Queen);
            Self::clear_selection(&mut state);
            state.drag = None;
            state.premove = Some((from, to, promotion));
            promotion
        };
        self.obj().queue_draw();
        let callback = self.on_premove.borrow().clone();
        if let Some(callback) = callback {
            callback(from, to, promotion);
        }
    }

    fn clear_selection(state: &mut BoardState) {
        state.selected = None;
        state.targets.clear();
        state.select_pulse = None;
    }

    fn target_matches(mv: &Move, target: Square) -> bool {
        Game::destination_of(mv) == target || matches!(mv, Move::Castle { rook, .. } if *rook == target)
    }

    fn try_move(&self, from: Square, to: Square, dropped: bool) -> bool {
        let premove = {
            let state = self.state.borrow();
            Self::is_premove_piece(&state, from).then(|| state.targets.iter().any(|mv| mv.to() == to))
        };
        if let Some(valid) = premove {
            if valid {
                self.emit_premove(from, to);
            }
            return valid;
        }
        let (needs_promotion, legal, color, auto_queen) = {
            let state = self.state.borrow();
            let candidates: Vec<&Move> = state
                .targets
                .iter()
                .filter(|mv| mv.from() == Some(from) && Self::target_matches(mv, to))
                .collect();
            (
                candidates.iter().any(|mv| mv.is_promotion()),
                !candidates.is_empty(),
                state.position.turn(),
                state.appearance.auto_queen,
            )
        };
        if !legal {
            return false;
        }
        if needs_promotion && !auto_queen {
            let mut state = self.state.borrow_mut();
            state.promotion = Some(PromotionPicker { from, to, color });
            state.drag = None;
            drop(state);
            self.obj().queue_draw();
            return true;
        }
        let promotion = needs_promotion.then_some(Role::Queen);
        self.emit_move(from, to, promotion, dropped);
        true
    }

    fn emit_move(&self, from: Square, to: Square, promotion: Option<Role>, dropped: bool) {
        {
            let mut state = self.state.borrow_mut();
            Self::clear_selection(&mut state);
            state.drag = None;
            state.promotion = None;
            state.suppress_next_slide = dropped;
        }
        self.obj().queue_draw();
        let callback = self.on_move.borrow().clone();
        if let Some(callback) = callback {
            callback(from, to, promotion);
        }
        self.state.borrow_mut().suppress_next_slide = false;
    }

    fn press(&self, x: f64, y: f64) -> bool {
        self.clear_drawings();
        let Some(geometry) = self.geometry.get() else {
            return false;
        };
        let picker = self.state.borrow().promotion;
        if let Some(picker) = picker {
            let chosen = self
                .promotion_cells(&geometry, &picker)
                .into_iter()
                .find(|(_, rect)| rect.contains_point(&graphene::Point::new(x as f32, y as f32)))
                .map(|(role, _)| role);
            match chosen {
                Some(role) => self.emit_move(picker.from, picker.to, Some(role), false),
                None => {
                    self.state.borrow_mut().promotion = None;
                    self.obj().queue_draw();
                }
            }
            return false;
        }
        let Some(square) = geometry.square_at(x, y) else {
            let mut state = self.state.borrow_mut();
            Self::clear_selection(&mut state);
            state.premove = None;
            drop(state);
            self.obj().queue_draw();
            return false;
        };
        let (selected, pending_move, own_piece) = {
            let state = self.state.borrow();
            let pending = state
                .selected
                .filter(|from| *from != square)
                .and_then(|_| state.targets.iter().find(|mv| Self::target_matches(mv, square)).copied());
            (state.selected, pending, Self::can_move_piece(&state, square))
        };
        if let (Some(from), Some(mv)) = (selected, pending_move)
            && (!own_piece || matches!(mv, Move::Castle { .. }))
        {
            self.try_move(from, square, false);
            return false;
        }
        let mut state = self.state.borrow_mut();
        if Self::can_move_piece(&state, square) {
            let was_selected = state.selected == Some(square);
            if !was_selected {
                self.select(&mut state, square);
            }
            state.drag = Some(DragState {
                from: square,
                start: (x, y),
                pointer: (x, y),
                active: false,
                was_selected,
            });
            drop(state);
            self.ensure_ticking();
            self.obj().queue_draw();
            true
        } else {
            Self::clear_selection(&mut state);
            state.premove = None;
            drop(state);
            self.obj().queue_draw();
            false
        }
    }

    fn drag_to(&self, x: f64, y: f64) {
        let geometry = self.geometry.get();
        let mut state = self.state.borrow_mut();
        let Some(mut drag) = state.drag else { return };
        let distance = ((x - drag.start.0).powi(2) + (y - drag.start.1).powi(2)).sqrt();
        if !drag.active && distance > 4.0 {
            drag.active = true;
            self.obj().set_cursor_from_name(Some("grabbing"));
        }
        drag.pointer = (x, y);
        state.drag = Some(drag);
        state.hover = geometry.and_then(|geometry| geometry.square_at(x, y));
        drop(state);
        self.obj().queue_draw();
    }

    fn release(&self, x: f64, y: f64) {
        let geometry = self.geometry.get();
        let drag = self.state.borrow_mut().drag.take();
        self.obj().set_cursor_from_name(None);
        let Some(drag) = drag else { return };
        if drag.active {
            let target = geometry.and_then(|geometry| geometry.square_at(x, y));
            self.state.borrow_mut().hover = None;
            match target {
                Some(target) if target != drag.from => {
                    if !self.try_move(drag.from, target, true) {
                        self.animate_return(drag, geometry);
                    }
                }
                _ => {
                    self.obj().queue_draw();
                }
            }
        } else if drag.was_selected {
            let mut state = self.state.borrow_mut();
            Self::clear_selection(&mut state);
            drop(state);
            self.obj().queue_draw();
        } else {
            self.obj().queue_draw();
        }
    }

    fn animate_return(&self, drag: DragState, geometry: Option<BoardGeometry>) {
        let Some(geometry) = geometry else { return };
        let mut state = self.state.borrow_mut();
        let Some(piece) = state.position.board().piece_at(drag.from) else {
            return;
        };
        let duration = state.appearance.animation_ms.min(160) as i64 * 1000;
        if duration == 0 {
            drop(state);
            self.obj().queue_draw();
            return;
        }
        let from_square = geometry.square_at(drag.pointer.0, drag.pointer.1).unwrap_or(drag.from);
        state.animation = Some(MoveAnimation {
            start_us: self.now(),
            duration_us: duration,
            sliding: vec![SlidingPiece {
                piece,
                from: from_square,
                to: drag.from,
            }],
            fading: Vec::new(),
        });
        drop(state);
        self.ensure_ticking();
    }

    pub fn has_drawings(&self) -> bool {
        !self.state.borrow().drawings.is_empty()
    }

    pub fn clear_drawings(&self) {
        let cleared = self.state.borrow_mut().drawings.clear();
        if cleared {
            self.obj().queue_draw();
        }
    }

    fn draw_color(modifiers: gdk::ModifierType) -> DrawColor {
        DrawColor::from_modifiers(
            modifiers.contains(gdk::ModifierType::SHIFT_MASK),
            modifiers.contains(gdk::ModifierType::CONTROL_MASK),
            modifiers.contains(gdk::ModifierType::ALT_MASK),
        )
    }

    fn begin_drawing(&self, x: f64, y: f64, modifiers: gdk::ModifierType) {
        let square = self.geometry.get().and_then(|geometry| geometry.square_at(x, y));
        self.arrow_origin.set((x, y));
        self.arrow_start.set(square);
        if let Some(square) = square {
            self.state.borrow_mut().draw_preview = Some((square, square, Self::draw_color(modifiers)));
        }
    }

    fn update_drawing(&self, x: f64, y: f64, modifiers: gdk::ModifierType) {
        let Some(start) = self.arrow_start.get() else { return };
        let current = self.geometry.get().and_then(|geometry| geometry.square_at(x, y)).unwrap_or(start);
        self.state.borrow_mut().draw_preview = Some((start, current, Self::draw_color(modifiers)));
        self.obj().queue_draw();
    }

    /// Finishes a right-button gesture: a drag toggles an arrow, a plain click
    /// toggles a square mark (or cancels a pending premove or selection).
    fn finish_drawing(&self, x: f64, y: f64, modifiers: gdk::ModifierType) {
        let start = self.arrow_start.take();
        self.state.borrow_mut().draw_preview = None;
        let Some(start) = start else {
            self.obj().queue_draw();
            return;
        };
        let end = self.geometry.get().and_then(|geometry| geometry.square_at(x, y)).unwrap_or(start);
        let color = Self::draw_color(modifiers);
        if start == end {
            if self.has_pending_interaction() {
                self.cancel_interaction();
            } else {
                self.state.borrow_mut().drawings.toggle_mark(start, color);
            }
        } else {
            self.state.borrow_mut().drawings.toggle_arrow(UserArrow { from: start, to: end, color });
        }
        self.obj().queue_draw();
    }

    pub fn cancel_interaction(&self) {
        let mut state = self.state.borrow_mut();
        Self::clear_selection(&mut state);
        state.premove = None;
        state.drag = None;
        state.promotion = None;
        state.hover = None;
        drop(state);
        self.obj().set_cursor_from_name(None);
        self.obj().queue_draw();
    }

    pub fn has_pending_interaction(&self) -> bool {
        let state = self.state.borrow();
        state.selected.is_some() || state.promotion.is_some() || state.drag.is_some() || state.premove.is_some()
    }

    fn update_cursor(&self, x: f64, y: f64) {
        let Some(geometry) = self.geometry.get() else { return };
        let mut state = self.state.borrow_mut();
        if state.drag.is_some_and(|drag| drag.active) {
            return;
        }
        let square = geometry.square_at(x, y);
        let previous_hover = state.hover;
        if state.promotion.is_some() {
            state.hover = square;
        }
        let cursor = match square {
            Some(square) if Self::can_move_piece(&state, square) => Some("grab"),
            Some(square)
                if state.selected.is_some()
                    && state.targets.iter().any(|mv| Self::target_matches(mv, square)) =>
            {
                Some("pointer")
            }
            _ if state.promotion.is_some() => Some("pointer"),
            _ => None,
        };
        let changed = previous_hover != state.hover;
        drop(state);
        self.obj().set_cursor_from_name(cursor);
        if changed {
            self.obj().queue_draw();
        }
    }

    pub fn build_animation(previous: &Chess, next: &Chess, hint: AnimationHint, start_us: i64, duration_us: i64) -> Option<MoveAnimation> {
        let mut sliding = Vec::new();
        let mut fading = Vec::new();
        match hint {
            AnimationHint::Forward(mv) => {
                let board = previous.board();
                match mv {
                    Move::Castle { king, rook } => {
                        let king_to = Game::destination_of(&mv);
                        let (_, rook_to) = Game::castling_rook_path(&mv)?;
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(king)?,
                            from: king,
                            to: king_to,
                        });
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(rook)?,
                            from: rook,
                            to: rook_to,
                        });
                    }
                    Move::EnPassant { from, to } => {
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(from)?,
                            from,
                            to,
                        });
                        let captured = Square::from_coords(to.file(), from.rank());
                        if let Some(piece) = board.piece_at(captured) {
                            fading.push((piece, captured));
                        }
                    }
                    Move::Normal { from, to, .. } => {
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(from)?,
                            from,
                            to,
                        });
                        if let Some(piece) = board.piece_at(to) {
                            fading.push((piece, to));
                        }
                    }
                    Move::Put { .. } => return None,
                }
            }
            AnimationHint::Backward(mv) => {
                let board = next.board();
                match mv {
                    Move::Castle { king, rook } => {
                        let king_to = Game::destination_of(&mv);
                        let (_, rook_to) = Game::castling_rook_path(&mv)?;
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(king)?,
                            from: king_to,
                            to: king,
                        });
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(rook)?,
                            from: rook_to,
                            to: rook,
                        });
                    }
                    Move::EnPassant { from, to } | Move::Normal { from, to, .. } => {
                        sliding.push(SlidingPiece {
                            piece: board.piece_at(from)?,
                            from: to,
                            to: from,
                        });
                    }
                    Move::Put { .. } => return None,
                }
            }
        }
        Some(MoveAnimation {
            start_us,
            duration_us,
            sliding,
            fading,
        })
    }

    pub fn set_position(&self, position: &Chess, last_move: Option<(Square, Square)>, hint: Option<AnimationHint>) {
        let now = self.now();
        let mut state = self.state.borrow_mut();
        let duration = state.appearance.animation_ms as i64 * 1000;
        let suppress = state.suppress_next_slide;
        let animation = match hint {
            Some(hint) if duration > 0 => {
                Self::build_animation(&state.position, position, hint, now, duration).and_then(|mut animation| {
                    if let (true, AnimationHint::Forward(mv)) = (suppress, hint) {
                        let castle = matches!(mv, Move::Castle { .. });
                        animation.sliding.retain(|slide| castle && slide.piece.role == Role::Rook);
                    }
                    (!animation.sliding.is_empty() || !animation.fading.is_empty()).then_some(animation)
                })
            }
            _ => None,
        };
        if state.position != *position {
            state.drawings.clear();
        }
        state.position = position.clone();
        state.last_move = last_move;
        state.animation = animation;
        Self::clear_selection(&mut state);
        state.drag = None;
        state.promotion = None;
        let animating = state.animation.is_some();
        drop(state);
        if animating {
            self.ensure_ticking();
        }
        self.obj().queue_draw();
    }
}
