use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};
use shakmaty::{Board, Chess, Color, Move, Piece, Position, Role, Square};

use super::animation::{MoveAnimation, Pulse, SlidingPiece, ease_out_back, ease_out_cubic};
use super::geometry::BoardGeometry;
use crate::assets::{PieceStyle, leather_texture_rgba, render_piece_rgba};
use crate::chess::{Game, is_promotion_square, premove_destinations};
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
            style: PieceStyle::ForestWood,
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
    pub mate_pulse: Option<Pulse>,
    pub flipped: bool,
    pub appearance: BoardAppearance,
    pub suppress_next_slide: bool,
    pub premove_color: Option<Color>,
    pub premove: Option<(Square, Square, Option<Role>)>,
    pub marker: Option<(Square, gdk::Texture)>,
    pub arrow: Option<(Square, Square)>,
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

        let secondary = gtk::GestureClick::new();
        secondary.set_button(gdk::BUTTON_SECONDARY);
        let weak = widget.downgrade();
        secondary.connect_pressed(move |_, _, _, _| {
            if let Some(widget) = weak.upgrade() {
                widget.imp().cancel_interaction();
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
                && widget.imp().has_pending_interaction()
            {
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
        self.draw_highlights(snapshot, &geometry, &state, now);
        self.draw_coordinates(snapshot, &geometry, &state);
        self.draw_hints(snapshot, &geometry, &state, now);
        self.draw_pieces(snapshot, &geometry, &state, now);
        self.draw_arrow(snapshot, &geometry, &state);
        self.draw_marker(snapshot, &geometry, &state);
        self.draw_mate_badge(snapshot, &geometry, &state, now);
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

    fn draw_highlights(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState, now: i64) {
        let highlight = gdk::RGBA::new(1.0, 1.0, 53.0 / 255.0, 0.5);
        if state.appearance.highlight_last
            && let Some((from, to)) = state.last_move
        {
            snapshot.append_color(&highlight, &geometry.square_rect(from));
            snapshot.append_color(&highlight, &geometry.square_rect(to));
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
        if let Some(square) = state.mate {
            let rect = geometry.square_rect(square);
            let progress = state.mate_pulse.map(|pulse| pulse.progress(now)).unwrap_or(1.0);
            let alpha = 0.35 + 0.5 * ease_out_cubic(progress);
            snapshot.append_color(&gdk::RGBA::new(0.86, 0.12, 0.12, alpha as f32), &rect);
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
            self.append_piece(snapshot, piece, &geometry.square_rect(square), 1.0);
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

    fn draw_arrow(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let Some((from, to)) = state.arrow else { return };
        if from == to {
            return;
        }
        let a = geometry.square_rect(from);
        let b = geometry.square_rect(to);
        let (ax, ay) = (a.x() + a.width() / 2.0, a.y() + a.height() / 2.0);
        let (bx, by) = (b.x() + b.width() / 2.0, b.y() + b.height() / 2.0);
        let (dx, dy) = (bx - ax, by - ay);
        let length = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = (dx / length, dy / length);
        let (px, py) = (-uy, ux);
        let square = geometry.square;
        let shaft = square * 0.17;
        let head_width = square * 0.46;
        let head_length = (square * 0.42).min(length * 0.6);
        let start = (ax + ux * square * 0.22, ay + uy * square * 0.22);
        let tip = (bx - ux * square * 0.06, by - uy * square * 0.06);
        let neck = (tip.0 - ux * head_length, tip.1 - uy * head_length);
        let builder = gsk::PathBuilder::new();
        builder.move_to(start.0 + px * shaft / 2.0, start.1 + py * shaft / 2.0);
        builder.line_to(neck.0 + px * shaft / 2.0, neck.1 + py * shaft / 2.0);
        builder.line_to(neck.0 + px * head_width / 2.0, neck.1 + py * head_width / 2.0);
        builder.line_to(tip.0, tip.1);
        builder.line_to(neck.0 - px * head_width / 2.0, neck.1 - py * head_width / 2.0);
        builder.line_to(neck.0 - px * shaft / 2.0, neck.1 - py * shaft / 2.0);
        builder.line_to(start.0 - px * shaft / 2.0, start.1 - py * shaft / 2.0);
        builder.close();
        let path = builder.to_path();
        snapshot.append_fill(&path, gsk::FillRule::Winding, &gdk::RGBA::new(0.25, 0.69, 0.96, 0.92));
    }

    fn draw_marker(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState) {
        let Some((square, texture)) = state.marker.as_ref() else { return };
        let rect = geometry.square_rect(*square);
        let size = geometry.square * 0.38;
        let board = geometry.board_rect();
        let x = (rect.x() + rect.width() - size * 0.72).min(board.x() + board.width() - size);
        let y = (rect.y() - size * 0.28).max(board.y());
        snapshot.append_texture(texture, &graphene::Rect::new(x, y, size, size));
    }

    fn draw_mate_badge(&self, snapshot: &gtk::Snapshot, geometry: &BoardGeometry, state: &BoardState, now: i64) {
        let Some(square) = state.mate else { return };
        let rect = geometry.square_rect(square);
        let progress = state.mate_pulse.map(|pulse| pulse.progress(now)).unwrap_or(1.0);
        let pop = ease_out_back(progress) as f32;
        if pop <= 0.01 {
            return;
        }
        let diameter = geometry.square * 0.42 * pop;
        let center_x = rect.x() + rect.width() * 0.86;
        let center_y = rect.y() + rect.height() * 0.14;
        let badge = graphene::Rect::new(center_x - diameter / 2.0, center_y - diameter / 2.0, diameter, diameter);
        let rounded = gsk::RoundedRect::from_rect(badge, diameter / 2.0);
        snapshot.append_outset_shadow(
            &rounded,
            &gdk::RGBA::new(0.0, 0.0, 0.0, 0.35),
            0.0,
            diameter * 0.06,
            0.0,
            diameter * 0.18,
        );
        snapshot.push_rounded_clip(&rounded);
        snapshot.append_color(&gdk::RGBA::new(0.80, 0.10, 0.10, 1.0), &badge);
        snapshot.pop();
        snapshot.append_border(
            &rounded,
            &[(diameter * 0.07).max(1.0); 4],
            &[gdk::RGBA::new(1.0, 1.0, 1.0, 0.95); 4],
        );
        let layout = self.coordinate_layout("#", diameter * 0.62);
        let mut description = layout.font_description().unwrap_or_default();
        description.set_weight(pango::Weight::Heavy);
        layout.set_font_description(Some(&description));
        let (text_width, text_height) = layout.pixel_size();
        self.draw_text(
            snapshot,
            &layout,
            center_x - text_width as f32 / 2.0,
            center_y - text_height as f32 / 2.0,
            gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
        );
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
            if state.mate_pulse.is_some_and(|pulse| pulse.is_finished(now)) {
                state.mate_pulse = None;
            }
            let active = state.animation.is_some() || state.select_pulse.is_some() || state.mate_pulse.is_some();
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

    pub fn start_mate_pulse(&self) {
        let now = self.now();
        self.state.borrow_mut().mate_pulse = Some(Pulse::new(now, 900));
        self.ensure_ticking();
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
