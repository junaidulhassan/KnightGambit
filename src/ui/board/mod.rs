mod animation;
mod checkmate_animation;
mod frame;
mod geometry;
mod imp;

pub use frame::BoardFrame;
pub use imp::{AnimationHint, BoardAppearance};

use std::rc::Rc;

use gtk::glib;
use gtk::subclass::prelude::*;
use gtk::prelude::*;
use shakmaty::{Chess, Role, Square};

glib::wrapper! {
    pub struct BoardWidget(ObjectSubclass<imp::BoardWidget>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for BoardWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BoardWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_move_requested(&self, callback: impl Fn(Square, Square, Option<Role>) + 'static) {
        self.imp().on_move.replace(Some(Rc::new(callback)));
    }

    pub fn connect_premove(&self, callback: impl Fn(Square, Square, Option<Role>) + 'static) {
        self.imp().on_premove.replace(Some(Rc::new(callback)));
    }

    pub fn set_premove_color(&self, color: Option<shakmaty::Color>) {
        let mut state = self.imp().state.borrow_mut();
        state.premove_color = color;
        if color.is_none() {
            state.premove = None;
        }
        drop(state);
        self.queue_draw();
    }

    pub fn take_premove(&self) -> Option<(Square, Square, Option<Role>)> {
        let premove = self.imp().state.borrow_mut().premove.take();
        if premove.is_some() {
            self.queue_draw();
        }
        premove
    }

    pub fn set_review_marker(&self, marker: Option<(Square, gtk::gdk::Texture)>) {
        self.imp().set_marker(marker, None);
    }

    /// Like [`Self::set_review_marker`], but the move's squares take `color`
    /// and `label` pops up briefly before the icon appears.
    pub fn set_accented_marker(&self, marker: Option<(Square, gtk::gdk::Texture)>, label: &str, color: gtk::gdk::RGBA) {
        self.imp().set_marker(marker, Some(imp::MarkerAccent { label: Some(label.to_owned()), color }));
    }

    /// Like [`Self::set_review_marker`], but the move's squares take `color`.
    pub fn set_tinted_marker(&self, marker: Option<(Square, gtk::gdk::Texture)>, color: gtk::gdk::RGBA) {
        self.imp().set_marker(marker, Some(imp::MarkerAccent { label: None, color }));
    }

    pub fn set_arrow(&self, arrow: Option<(Square, Square)>) {
        self.imp().state.borrow_mut().arrow = arrow;
        self.queue_draw();
    }

    pub fn set_appearance(&self, appearance: BoardAppearance) {
        let changed = {
            let mut state = self.imp().state.borrow_mut();
            let changed = state.appearance != appearance;
            state.appearance = appearance;
            changed
        };
        if changed {
            self.queue_draw();
        }
    }

    pub fn set_position(&self, position: &Chess, last_move: Option<(Square, Square)>, hint: Option<AnimationHint>) {
        self.imp().set_position(position, last_move, hint);
    }

    pub fn set_status(&self, check: Option<Square>, mate: Option<Square>) {
        let start_pulse = {
            let mut state = self.imp().state.borrow_mut();
            let fresh_mate = mate.is_some() && state.mate != mate;
            state.check = check;
            state.mate = mate;
            if mate.is_none() {
                state.checkmate = None;
            }
            fresh_mate
        };
        if let Some(loser) = mate.filter(|_| start_pulse) {
            self.imp().start_checkmate(loser);
        }
        self.queue_draw();
    }

    pub fn set_movable(&self, white: bool, black: bool) {
        let mut state = self.imp().state.borrow_mut();
        state.movable = [white, black];
        if !white && !black {
            state.selected = None;
            state.targets.clear();
            state.drag = None;
            state.promotion = None;
        }
        drop(state);
        self.queue_draw();
    }

    pub fn set_flipped(&self, flipped: bool) {
        self.imp().state.borrow_mut().flipped = flipped;
        self.imp().cancel_interaction();
        self.queue_draw();
    }

    /// Removes all hand-drawn arrows and square marks.
    pub fn clear_drawings(&self) {
        self.imp().clear_drawings();
    }

    pub fn imp_device_scale(&self) -> f64 {
        self.imp().device_scale()
    }

    pub fn is_flipped(&self) -> bool {
        self.imp().state.borrow().flipped
    }

    pub fn cancel_interaction(&self) {
        self.imp().cancel_interaction();
    }
}
