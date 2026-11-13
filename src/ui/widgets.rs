use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};
use shakmaty::{Color, Role};

use crate::assets::PieceStyle;
use crate::chess::{NotationStyle, PlayedMove, format_san};
use crate::review::{GameReview, MoveClass, MoveReview};
use crate::ui::art::{piece_picture, review_icon_picture, small_piece_texture};

pub enum AvatarKind<'a> {
    Human(Color),
    Engine { name: &'a str, logo: Option<&'static [u8]> },
}

pub fn engine_logo_texture(logo: &'static [u8]) -> Option<gdk::Texture> {
    gdk::Texture::from_bytes(&glib::Bytes::from_static(logo)).ok()
}

pub struct PlayerBar {
    pub root: gtk::Box,
    avatar: adw::Avatar,
    name: gtk::Label,
    detail: gtk::Label,
    captured: gtk::Box,
    advantage: gtk::Label,
    clock: gtk::Label,
    thinking: gtk::Spinner,
}

impl PlayerBar {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        root.add_css_class("player-bar");
        let avatar = adw::Avatar::new(40, None, true);
        avatar.add_css_class("player-avatar");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let name = gtk::Label::new(None);
        name.add_css_class("player-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let thinking = gtk::Spinner::new();
        thinking.set_visible(false);
        thinking.set_tooltip_text(Some("Thinking"));
        let detail = gtk::Label::new(None);
        detail.add_css_class("player-detail");
        detail.add_css_class("dim-label");
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name_row.append(&name);
        name_row.append(&detail);
        name_row.append(&thinking);
        let captured_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let captured = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        captured.add_css_class("captured-pieces");
        let advantage = gtk::Label::new(None);
        advantage.add_css_class("material-advantage");
        captured_row.append(&captured);
        captured_row.append(&advantage);
        captured_row.set_size_request(-1, 20);
        text.append(&name_row);
        text.append(&captured_row);
        let clock = gtk::Label::new(None);
        clock.add_css_class("clock");
        clock.set_valign(gtk::Align::Center);
        clock.set_width_chars(7);
        root.append(&avatar);
        root.append(&text);
        root.append(&clock);
        Self {
            root,
            avatar,
            name,
            detail,
            captured,
            advantage,
            clock,
            thinking,
        }
    }

    pub fn set_player(&self, name: &str, detail: &str, avatar: AvatarKind, style: PieceStyle) {
        self.name.set_label(name);
        self.detail.set_label(detail);
        self.detail.set_visible(!detail.is_empty());
        let scale = self.root.scale_factor().max(1);
        match avatar {
            AvatarKind::Human(color) => {
                self.avatar.set_text(Some(name));
                let texture = small_piece_texture(style, Role::King, color, (40 * scale) as u32);
                self.avatar.set_custom_image(texture.as_ref());
                self.avatar.remove_css_class("engine-avatar");
            }
            AvatarKind::Engine { name: engine_name, logo } => {
                self.avatar.set_text(Some(engine_name));
                let texture = logo.and_then(engine_logo_texture);
                self.avatar.set_custom_image(texture.as_ref());
                self.avatar.add_css_class("engine-avatar");
            }
        }
    }

    pub fn set_captured(&self, roles: &[Role], captured_color: Color, advantage: i32, style: PieceStyle) {
        while let Some(child) = self.captured.first_child() {
            self.captured.remove(&child);
        }
        let scale = self.root.scale_factor().max(1);
        let layer = gtk::Fixed::new();
        let mut previous: Option<Role> = None;
        let mut x = 0.0;
        for role in roles {
            if let Some(last) = previous {
                x += if last == *role { 9.0 } else { 20.0 };
            }
            let picture = piece_picture(style, *role, captured_color, 20, scale);
            layer.put(&picture, x, 0.0);
            previous = Some(*role);
        }
        if previous.is_some() {
            layer.set_size_request(x as i32 + 20, 20);
            self.captured.append(&layer);
        }
        if advantage > 0 {
            self.advantage.set_label(&format!("+{advantage}"));
            self.advantage.set_visible(true);
        } else {
            self.advantage.set_visible(false);
        }
    }

    pub fn set_clock(&self, text: Option<&str>, active: bool, low: bool) {
        match text {
            Some(text) => {
                self.clock.set_visible(true);
                self.clock.set_label(text);
                set_class(&self.clock, "active", active);
                set_class(&self.clock, "low", low);
            }
            None => self.clock.set_visible(false),
        }
    }

    pub fn set_active(&self, active: bool) {
        set_class(&self.root, "to-move", active);
    }

    pub fn set_thinking(&self, thinking: bool) {
        self.thinking.set_visible(thinking);
        self.thinking.set_spinning(thinking);
    }
}

impl Default for PlayerBar {
    fn default() -> Self {
        Self::new()
    }
}

pub fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, enabled: bool) {
    if enabled {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

type SelectCallback = Rc<dyn Fn(usize)>;

pub struct MoveList {
    pub root: gtk::ScrolledWindow,
    grid: gtk::Grid,
    placeholder: gtk::Label,
    buttons: RefCell<Vec<gtk::Button>>,
    on_select: RefCell<Option<SelectCallback>>,
    signature: RefCell<Vec<String>>,
}

impl MoveList {
    pub fn new() -> Rc<Self> {
        let grid = gtk::Grid::new();
        grid.set_column_spacing(4);
        grid.set_row_spacing(2);
        grid.add_css_class("move-grid");
        grid.set_margin_top(6);
        grid.set_margin_bottom(6);
        grid.set_margin_start(8);
        grid.set_margin_end(8);
        let placeholder = gtk::Label::new(Some("Moves will appear here"));
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_top(24);
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.append(&placeholder);
        container.append(&grid);
        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&container)
            .build();
        root.add_css_class("move-list");
        Rc::new(Self {
            root,
            grid,
            placeholder,
            buttons: RefCell::new(Vec::new()),
            on_select: RefCell::new(None),
            signature: RefCell::new(Vec::new()),
        })
    }

    pub fn connect_select(&self, callback: impl Fn(usize) + 'static) {
        self.on_select.replace(Some(Rc::new(callback)));
    }

    pub fn set_moves(
        self: &Rc<Self>,
        moves: &[PlayedMove],
        first_move_number: u32,
        black_moves_first: bool,
        current_ply: usize,
        style: NotationStyle,
        annotations: &[Option<MoveClass>],
    ) {
        let texts: Vec<String> = moves.iter().map(|played| format_san(played, style)).collect();
        let mut signature = texts.clone();
        signature.push(format!("{first_move_number}:{black_moves_first}"));
        signature.extend(annotations.iter().map(|class| format!("{class:?}")));
        if *self.signature.borrow() != signature {
            self.rebuild(&texts, first_move_number, black_moves_first, annotations);
            self.signature.replace(signature);
        }
        self.highlight(current_ply);
    }

    fn rebuild(self: &Rc<Self>, texts: &[String], first_move_number: u32, black_moves_first: bool, annotations: &[Option<MoveClass>]) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        let mut buttons = Vec::with_capacity(texts.len());
        let offset = usize::from(black_moves_first);
        for (index, text) in texts.iter().enumerate() {
            let slot = index + offset;
            let row = (slot / 2) as i32;
            let column = (slot % 2) as i32 + 1;
            if column == 1 || index == 0 {
                let number = gtk::Label::new(Some(&format!("{}.", first_move_number + row as u32)));
                number.add_css_class("move-number");
                number.add_css_class("dim-label");
                number.set_xalign(1.0);
                number.set_width_chars(4);
                self.grid.attach(&number, 0, row, 1, 1);
            }
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            let class = annotations.get(index).copied().flatten();
            match class {
                Some(class) if class.highlighted_in_list() => {
                    content.append(&review_icon_picture(class, 18, self.root.scale_factor()));
                }
                _ => {
                    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                    spacer.set_size_request(if annotations.is_empty() { 0 } else { 18 }, -1);
                    content.append(&spacer);
                }
            }
            match class {
                Some(class) if class.colored_in_list() => label.set_markup(&format!(
                    "<span foreground=\"{}\" weight=\"bold\">{}</span>",
                    class.color(),
                    glib::markup_escape_text(text)
                )),
                _ => label.set_text(text),
            }
            content.append(&label);
            let button = gtk::Button::builder().child(&content).build();
            button.add_css_class("flat");
            button.add_css_class("move-button");
            button.set_hexpand(true);
            let weak = Rc::downgrade(self);
            let ply = index + 1;
            button.connect_clicked(move |_| {
                if let Some(list) = weak.upgrade() {
                    let callback = list.on_select.borrow().clone();
                    if let Some(callback) = callback {
                        callback(ply);
                    }
                }
            });
            self.grid.attach(&button, column, row, 1, 1);
            buttons.push(button);
        }
        self.placeholder.set_visible(texts.is_empty());
        self.buttons.replace(buttons);
    }

    fn highlight(&self, current_ply: usize) {
        let buttons = self.buttons.borrow();
        for (index, button) in buttons.iter().enumerate() {
            set_class(button, "current-move", index + 1 == current_ply);
        }
        if let Some(button) = current_ply.checked_sub(1).and_then(|index| buttons.get(index)).cloned() {
            let scrolled = self.root.clone();
            glib::idle_add_local_once(move || {
                let adjustment = scrolled.vadjustment();
                if let Some(bounds) = button.compute_bounds(&scrolled) {
                    let top = bounds.y() as f64 + adjustment.value();
                    let bottom = top + bounds.height() as f64;
                    let visible_top = adjustment.value();
                    let visible_bottom = visible_top + adjustment.page_size();
                    if top < visible_top {
                        adjustment.set_value(top - 4.0);
                    } else if bottom > visible_bottom {
                        adjustment.set_value(bottom - adjustment.page_size() + 4.0);
                    }
                }
            });
        } else if current_ply == 0 {
            self.root.vadjustment().set_value(0.0);
        }
    }
}

pub struct AnalysisPanel {
    pub root: gtk::Box,
    title: gtk::Label,
    evaluation: gtk::Label,
    stats: gtk::Label,
    line: gtk::Label,
    bar: gtk::LevelBar,
    pub feedback_toggle: gtk::ToggleButton,
}

impl AnalysisPanel {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("card");
        root.add_css_class("analysis-card");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some("Engine analysis"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let evaluation = gtk::Label::new(Some("0.00"));
        evaluation.add_css_class("evaluation");
        let feedback_toggle = gtk::ToggleButton::with_label("Judge moves");
        feedback_toggle.add_css_class("flat");
        feedback_toggle.add_css_class("caption");
        feedback_toggle.set_valign(gtk::Align::Center);
        feedback_toggle.set_tooltip_text(Some(
            "Rate every move you play as brilliant, best, inaccuracy, mistake or blunder (practice mode)",
        ));
        header.append(&title);
        header.append(&feedback_toggle);
        header.append(&evaluation);
        let bar = gtk::LevelBar::for_interval(0.0, 1.0);
        bar.set_value(0.5);
        bar.add_css_class("eval-bar");
        bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_LOW));
        bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_HIGH));
        bar.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_FULL));
        let stats = gtk::Label::new(None);
        stats.add_css_class("caption");
        stats.add_css_class("dim-label");
        stats.set_xalign(0.0);
        let line = gtk::Label::new(None);
        line.add_css_class("analysis-line");
        line.set_xalign(0.0);
        line.set_wrap(true);
        line.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        line.set_lines(3);
        line.set_ellipsize(gtk::pango::EllipsizeMode::End);
        line.set_selectable(false);
        root.append(&header);
        root.append(&bar);
        root.append(&stats);
        root.append(&line);
        Self {
            root,
            title,
            evaluation,
            stats,
            line,
            bar,
            feedback_toggle,
        }
    }

    pub fn show(&self, title: &str, evaluation: &str, white_share: f64, stats: &str, line: &str) {
        self.title.set_label(title);
        self.evaluation.set_label(evaluation);
        self.bar.set_value(white_share.clamp(0.02, 0.98));
        self.stats.set_label(stats);
        self.line.set_label(line);
    }

    pub fn reset(&self, title: &str) {
        self.show(title, "—", 0.5, "Waiting for the engine…", "");
    }
}

impl Default for AnalysisPanel {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GameOverCard {
    pub root: gtk::Revealer,
    icon: gtk::Image,
    title: gtk::Label,
    subtitle: gtk::Label,
    score: gtk::Label,
    pub new_game: gtk::Button,
    pub rematch: gtk::Button,
    pub review: gtk::Button,
}

impl GameOverCard {
    pub fn new() -> Self {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
        card.add_css_class("game-over-card");
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Center);
        card.set_width_request(300);
        let icon = gtk::Image::from_icon_name("kg-trophy-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("game-over-icon");
        let title = gtk::Label::new(None);
        title.add_css_class("title-1");
        let subtitle = gtk::Label::new(None);
        subtitle.set_wrap(true);
        subtitle.set_justify(gtk::Justification::Center);
        subtitle.add_css_class("game-over-subtitle");
        let score = gtk::Label::new(None);
        score.add_css_class("game-over-score");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::Center);
        buttons.set_margin_top(6);
        let new_game = gtk::Button::with_label("New Game");
        new_game.add_css_class("suggested-action");
        new_game.add_css_class("pill");
        let rematch = gtk::Button::with_label("Rematch");
        rematch.add_css_class("pill");
        let review = gtk::Button::with_label("Game Review");
        review.add_css_class("pill");
        buttons.append(&review);
        buttons.append(&rematch);
        buttons.append(&new_game);
        card.append(&icon);
        card.append(&title);
        card.append(&subtitle);
        card.append(&score);
        card.append(&buttons);
        let root = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(260)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .child(&card)
            .reveal_child(false)
            .can_target(false)
            .build();
        Self {
            root,
            icon,
            title,
            subtitle,
            score,
            new_game,
            rematch,
            review,
        }
    }

    pub fn present(&self, title: &str, subtitle: &str, score: &str, icon_name: &str, tone: &str) {
        self.title.set_label(title);
        self.subtitle.set_label(subtitle);
        self.score.set_label(score);
        self.icon.set_icon_name(Some(icon_name));
        if let Some(card) = self.root.child() {
            for class in ["win", "loss", "draw"] {
                card.remove_css_class(class);
            }
            card.add_css_class(tone);
        }
        self.root.set_can_target(true);
        self.root.set_reveal_child(true);
    }

    pub fn dismiss(&self) {
        self.root.set_can_target(false);
        self.root.set_reveal_child(false);
    }

    pub fn is_shown(&self) -> bool {
        self.root.reveals_child()
    }
}

impl Default for GameOverCard {
    fn default() -> Self {
        Self::new()
    }
}

type PlyCallback = Rc<dyn Fn(usize)>;
type GraphData = Rc<RefCell<(Vec<f64>, Vec<Option<MoveClass>>, usize)>>;

struct CountRow {
    widgets: Vec<gtk::Widget>,
    white: gtk::Label,
    black: gtk::Label,
}

pub struct ReviewPanel {
    pub root: gtk::Box,
    pub close: gtk::Button,
    progress_box: gtk::Box,
    progress: gtk::ProgressBar,
    progress_label: gtk::Label,
    summary: gtk::Box,
    white_name: gtk::Label,
    black_name: gtk::Label,
    white_accuracy: gtk::Label,
    black_accuracy: gtk::Label,
    white_rating: gtk::Label,
    black_rating: gtk::Label,
    opening: gtk::Label,
    rows: Vec<CountRow>,
    feedback_icon: gtk::Box,
    feedback_title: gtk::Label,
    feedback_detail: gtk::Label,
    graph: gtk::DrawingArea,
    graph_data: GraphData,
    on_seek: Rc<RefCell<Option<PlyCallback>>>,
}

fn class_rgb(class: MoveClass) -> (f64, f64, f64) {
    let value = u32::from_str_radix(class.color().trim_start_matches('#'), 16).unwrap_or(0);
    (
        ((value >> 16) & 0xff) as f64 / 255.0,
        ((value >> 8) & 0xff) as f64 / 255.0,
        (value & 0xff) as f64 / 255.0,
    )
}

fn colored_count(label: &gtk::Label, class: MoveClass, count: usize) {
    if count == 0 {
        label.set_markup(&format!("<span foreground=\"{}\" alpha=\"55%\">0</span>", class.color()));
    } else {
        label.set_markup(&format!(
            "<span foreground=\"{}\" weight=\"heavy\">{count}</span>",
            class.color()
        ));
    }
}

impl Default for ReviewPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewPanel {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("card");
        root.add_css_class("review-card");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some("Game Review"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        let engine_badge = gtk::Label::new(Some("Stockfish"));
        engine_badge.add_css_class("engine-badge");
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.add_css_class("circular");
        close.set_tooltip_text(Some("Close review"));
        header.append(&title);
        header.append(&engine_badge);
        header.append(&close);
        root.append(&header);

        let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let progress = gtk::ProgressBar::new();
        let progress_label = gtk::Label::new(None);
        progress_label.add_css_class("caption");
        progress_label.add_css_class("dim-label");
        progress_label.set_xalign(0.0);
        progress_box.append(&progress);
        progress_box.append(&progress_label);
        root.append(&progress_box);

        let summary = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let graph = gtk::DrawingArea::new();
        graph.set_content_height(64);
        graph.add_css_class("review-graph");
        summary.append(&graph);

        let opening = gtk::Label::new(None);
        opening.add_css_class("review-opening");
        opening.set_xalign(0.0);
        opening.set_wrap(true);
        summary.append(&opening);

        let accuracy_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        accuracy_row.set_homogeneous(true);
        let column = |name: &gtk::Label, value: &gtk::Label, rating: &gtk::Label, class: &str| {
            let block = gtk::Box::new(gtk::Orientation::Vertical, 2);
            block.add_css_class("accuracy-block");
            block.add_css_class(class);
            name.add_css_class("caption-heading");
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            value.add_css_class("accuracy-value");
            let caption = gtk::Label::new(Some("Accuracy"));
            caption.add_css_class("caption");
            caption.add_css_class("accuracy-caption");
            rating.add_css_class("rating-value");
            let rating_caption = gtk::Label::new(Some("Game Rating"));
            rating_caption.add_css_class("caption");
            rating_caption.add_css_class("accuracy-caption");
            block.append(name);
            block.append(value);
            block.append(&caption);
            block.append(rating);
            block.append(&rating_caption);
            block
        };
        let white_name = gtk::Label::new(Some("White"));
        let black_name = gtk::Label::new(Some("Black"));
        let white_accuracy = gtk::Label::new(Some("—"));
        let black_accuracy = gtk::Label::new(Some("—"));
        let white_rating = gtk::Label::new(Some("—"));
        let black_rating = gtk::Label::new(Some("—"));
        accuracy_row.append(&column(&white_name, &white_accuracy, &white_rating, "white-side"));
        accuracy_row.append(&column(&black_name, &black_accuracy, &black_rating, "black-side"));
        summary.append(&accuracy_row);

        let grid = gtk::Grid::new();
        grid.set_row_spacing(7);
        grid.set_column_spacing(14);
        grid.add_css_class("review-counts");
        let scale = root.scale_factor().max(1);
        let mut rows = Vec::new();
        for (row, class) in MoveClass::ALL.iter().enumerate() {
            let name = gtk::Label::new(Some(class.label()));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.add_css_class("class-name");
            let white = gtk::Label::new(Some("0"));
            white.add_css_class("count");
            white.set_xalign(0.5);
            white.set_width_chars(3);
            let icon = review_icon_picture(*class, 22, scale);
            icon.set_halign(gtk::Align::Center);
            let black = gtk::Label::new(Some("0"));
            black.add_css_class("count");
            black.set_xalign(0.5);
            black.set_width_chars(3);
            grid.attach(&name, 0, row as i32, 1, 1);
            grid.attach(&white, 1, row as i32, 1, 1);
            grid.attach(&icon, 2, row as i32, 1, 1);
            grid.attach(&black, 3, row as i32, 1, 1);
            rows.push(CountRow {
                widgets: vec![name.upcast(), icon.upcast(), white.clone().upcast(), black.clone().upcast()],
                white,
                black,
            });
        }
        summary.append(&grid);

        let feedback = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        feedback.add_css_class("review-feedback");
        let feedback_icon = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        feedback_icon.set_valign(gtk::Align::Center);
        let feedback_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let feedback_title = gtk::Label::new(None);
        feedback_title.add_css_class("heading");
        feedback_title.set_xalign(0.0);
        feedback_title.set_wrap(true);
        let feedback_detail = gtk::Label::new(None);
        feedback_detail.add_css_class("dim-label");
        feedback_detail.set_xalign(0.0);
        feedback_detail.set_wrap(true);
        feedback_text.append(&feedback_title);
        feedback_text.append(&feedback_detail);
        feedback.append(&feedback_icon);
        feedback.append(&feedback_text);
        summary.append(&feedback);
        root.append(&summary);

        let graph_data: GraphData = Rc::new(RefCell::new((Vec::new(), Vec::new(), 0)));
        let data = graph_data.clone();
        graph.set_draw_func(move |_, cr, width, height| {
            let (curve, classes, current) = &*data.borrow();
            let (w, h) = (width as f64, height as f64);
            cr.set_source_rgb(0.17, 0.165, 0.157);
            cr.rectangle(0.0, 0.0, w, h);
            let _ = cr.fill();
            if curve.len() < 2 {
                return;
            }
            let step = w / (curve.len() - 1) as f64;
            cr.move_to(0.0, h);
            for (index, value) in curve.iter().enumerate() {
                cr.line_to(index as f64 * step, h * (1.0 - value / 100.0));
            }
            cr.line_to(w, h);
            cr.close_path();
            cr.set_source_rgb(0.953, 0.925, 0.875);
            let _ = cr.fill();
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.6);
            cr.set_line_width(1.0);
            cr.move_to(0.0, h / 2.0);
            cr.line_to(w, h / 2.0);
            let _ = cr.stroke();
            for (index, class) in classes.iter().enumerate() {
                if let Some(class) = class.filter(|class| class.is_error() || matches!(class, MoveClass::Brilliant | MoveClass::Great)) {
                    let ply = index + 1;
                    let (r, g, b) = class_rgb(class);
                    let x = ply as f64 * step;
                    let y = h * (1.0 - curve.get(ply).copied().unwrap_or(50.0) / 100.0);
                    cr.set_source_rgb(r, g, b);
                    cr.arc(x, y.clamp(4.0, h - 4.0), 4.0, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
            }
            cr.set_source_rgb(0.25, 0.69, 0.96);
            cr.set_line_width(2.0);
            let x = (*current as f64 * step).clamp(1.0, w - 1.0);
            cr.move_to(x, 0.0);
            cr.line_to(x, h);
            let _ = cr.stroke();
        });
        let on_seek: Rc<RefCell<Option<PlyCallback>>> = Rc::new(RefCell::new(None));
        let click = gtk::GestureClick::new();
        {
            let data = graph_data.clone();
            let on_seek = on_seek.clone();
            let graph = graph.clone();
            click.connect_pressed(move |_, _, x, _| {
                let count = data.borrow().0.len();
                if count < 2 {
                    return;
                }
                let ply = ((x / graph.width().max(1) as f64) * (count - 1) as f64).round() as usize;
                let callback = on_seek.borrow().clone();
                if let Some(callback) = callback {
                    callback(ply.min(count - 1));
                }
            });
        }
        graph.add_controller(click);
        graph.set_cursor_from_name(Some("pointer"));

        Self {
            root,
            close,
            progress_box,
            progress,
            progress_label,
            summary,
            white_name,
            black_name,
            white_accuracy,
            black_accuracy,
            white_rating,
            black_rating,
            opening,
            rows,
            feedback_icon,
            feedback_title,
            feedback_detail,
            graph,
            graph_data,
            on_seek,
        }
    }

    pub fn connect_seek(&self, callback: impl Fn(usize) + 'static) {
        self.on_seek.replace(Some(Rc::new(callback)));
    }

    pub fn show_progress(&self, done: usize, total: usize, engine: &str) {
        self.root.set_visible(true);
        self.progress_box.set_visible(true);
        self.summary.set_visible(false);
        let fraction = if total == 0 { 0.0 } else { done as f64 / total as f64 };
        self.progress.set_fraction(fraction);
        self.progress_label
            .set_label(&format!("{engine} is analyzing position {} of {total}…", (done + 1).min(total)));
    }

    pub fn show_review(&self, review: &GameReview, white: &str, black: &str, ply: usize) {
        self.root.set_visible(true);
        self.progress_box.set_visible(false);
        self.summary.set_visible(true);
        self.white_name.set_label(white);
        self.black_name.set_label(black);
        let scored = |slot: usize| {
            MoveClass::ALL
                .iter()
                .filter(|class| class.counts_for_accuracy())
                .map(|class| review.counts[slot][class.index()])
                .sum::<usize>()
        };
        let accuracy_text = |slot: usize| {
            if scored(slot) == 0 {
                "—".to_owned()
            } else {
                format!("{:.1}", review.accuracy[slot])
            }
        };
        self.white_accuracy.set_label(&accuracy_text(0));
        self.black_accuracy.set_label(&accuracy_text(1));
        let rating = |value: Option<u32>| value.map(|elo| elo.to_string()).unwrap_or_else(|| "—".to_owned());
        self.white_rating.set_label(&rating(review.estimated_elo[0]));
        self.black_rating.set_label(&rating(review.estimated_elo[1]));
        match &review.opening {
            Some(name) => {
                self.opening.set_markup(&format!(
                    "<span foreground=\"{}\">📖</span>  {}",
                    MoveClass::Book.color(),
                    glib::markup_escape_text(name)
                ));
                self.opening.set_visible(true);
            }
            None => self.opening.set_visible(false),
        }
        for (index, row) in self.rows.iter().enumerate() {
            let class = MoveClass::ALL[index];
            let white_count = review.counts[0][index];
            let black_count = review.counts[1][index];
            colored_count(&row.white, class, white_count);
            colored_count(&row.black, class, black_count);
            let visible = class != MoveClass::Forced || white_count + black_count > 0;
            for widget in &row.widgets {
                widget.set_visible(visible);
            }
        }
        {
            let mut data = self.graph_data.borrow_mut();
            data.0 = review.white_curve.clone();
            data.1 = review.moves.iter().map(|m| Some(m.class)).collect();
            data.2 = ply;
        }
        self.graph.queue_draw();
        self.show_move(review.move_at(ply), review, ply);
    }

    fn show_move(&self, current: Option<&MoveReview>, review: &GameReview, ply: usize) {
        while let Some(child) = self.feedback_icon.first_child() {
            self.feedback_icon.remove(&child);
        }
        let scale = self.root.scale_factor().max(1);
        match current {
            Some(item) => {
                self.feedback_icon.append(&review_icon_picture(item.class, 34, scale));
                self.feedback_title.set_markup(&format!(
                    "<span foreground=\"{}\" weight=\"heavy\">{}</span> {}",
                    item.class.color(),
                    glib::markup_escape_text(&item.san),
                    item.class.article_phrase()
                ));
                let eval = review
                    .white_scores
                    .get(ply)
                    .map(|score| match score {
                        crate::engine::uci::Score::Mate(0) => "checkmate".to_owned(),
                        other => other.display(),
                    })
                    .unwrap_or_default();
                let detail = match (&item.best_san, item.played_best, item.class) {
                    (_, _, MoveClass::Book) => format!("Opening theory. Evaluation {eval}."),
                    (Some(best), false, _) => format!("Best was {best}. Evaluation {eval}. Accuracy {:.0}%", item.accuracy),
                    _ => format!("Evaluation {eval}. Accuracy {:.0}%", item.accuracy),
                };
                self.feedback_detail.set_label(&detail);
            }
            None => {
                self.feedback_title.set_label("Starting position");
                self.feedback_detail
                    .set_label("Step through the moves or click the graph to see each classification.");
            }
        }
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
    }
}


/// A vertical evaluation bar beside the board: White's share fills from White's
/// side, with the score printed on the side that is ahead.
pub struct EvalBar {
    pub root: gtk::DrawingArea,
    state: Rc<EvalBarState>,
}

struct EvalBarState {
    target: std::cell::Cell<f64>,
    shown: std::cell::Cell<f64>,
    flipped: std::cell::Cell<bool>,
    text: RefCell<String>,
    ticking: std::cell::Cell<bool>,
}

impl EvalBar {
    pub fn new() -> Self {
        let root = gtk::DrawingArea::new();
        root.set_content_width(26);
        root.set_vexpand(true);
        root.add_css_class("eval-bar-vertical");
        root.update_property(&[gtk::accessible::Property::Label("Evaluation bar")]);
        let state = Rc::new(EvalBarState {
            target: std::cell::Cell::new(0.5),
            shown: std::cell::Cell::new(0.5),
            flipped: std::cell::Cell::new(false),
            text: RefCell::new("0.0".to_owned()),
            ticking: std::cell::Cell::new(false),
        });
        let draw_state = state.clone();
        root.set_draw_func(move |_, cr, width, height| {
            let (w, h) = (f64::from(width), f64::from(height));
            let share = draw_state.shown.get().clamp(0.0, 1.0);
            let flipped = draw_state.flipped.get();
            cr.save().ok();
            let radius = 5.0;
            cr.new_sub_path();
            cr.arc(w - radius, radius, radius, -std::f64::consts::FRAC_PI_2, 0.0);
            cr.arc(w - radius, h - radius, radius, 0.0, std::f64::consts::FRAC_PI_2);
            cr.arc(radius, h - radius, radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
            cr.arc(radius, radius, radius, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
            cr.close_path();
            cr.clip();
            cr.set_source_rgb(0.18, 0.18, 0.17);
            let _ = cr.paint();
            cr.set_source_rgb(0.96, 0.95, 0.92);
            let white_height = h * share;
            if flipped {
                cr.rectangle(0.0, 0.0, w, white_height);
            } else {
                cr.rectangle(0.0, h - white_height, w, white_height);
            }
            let _ = cr.fill();
            // Midline marker.
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.6);
            cr.rectangle(0.0, h / 2.0 - 0.5, w, 1.0);
            let _ = cr.fill();
            // Score text on the leading side.
            let text = draw_state.text.borrow();
            cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
            cr.set_font_size(10.0);
            if let Ok(extents) = cr.text_extents(&text) {
                let white_leads = share >= 0.5;
                let at_bottom = white_leads != flipped;
                let y = if at_bottom { h - 6.0 } else { extents.height() + 6.0 };
                if white_leads {
                    cr.set_source_rgb(0.15, 0.15, 0.14);
                } else {
                    cr.set_source_rgb(0.95, 0.95, 0.93);
                }
                cr.move_to(w / 2.0 - extents.width() / 2.0 - extents.x_bearing(), y);
                let _ = cr.show_text(&text);
            }
            cr.restore().ok();
        });
        Self { root, state }
    }

    pub fn set_flipped(&self, flipped: bool) {
        self.state.flipped.set(flipped);
        self.root.queue_draw();
    }

    /// Shows an evaluation: White's share in 0..=1 and the label to print.
    pub fn set(&self, white_share: f64, text: &str) {
        self.state.target.set(white_share.clamp(0.0, 1.0));
        *self.state.text.borrow_mut() = text.to_owned();
        self.root.set_tooltip_text(Some(&format!("Evaluation {text} (positive favors White)")));
        self.animate();
    }

    pub fn reset(&self) {
        self.set(0.5, "0.0");
    }

    fn animate(&self) {
        if self.state.ticking.replace(true) {
            return;
        }
        let state = self.state.clone();
        let root = self.root.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let (shown, target) = (state.shown.get(), state.target.get());
            let next = shown + (target - shown) * 0.28;
            let done = (target - next).abs() < 0.002;
            state.shown.set(if done { target } else { next });
            root.queue_draw();
            if done {
                state.ticking.set(false);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}

impl Default for EvalBar {
    fn default() -> Self {
        Self::new()
    }
}

/// A short card telling how the last move was judged and what was best.
pub struct FeedbackCard {
    pub root: gtk::Box,
    icon: gtk::Image,
    title: gtk::Label,
    detail: gtk::Label,
}

impl FeedbackCard {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.add_css_class("card");
        root.add_css_class("feedback-card");
        let icon = gtk::Image::new();
        icon.set_pixel_size(32);
        icon.set_valign(gtk::Align::Center);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = gtk::Label::new(None);
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_wrap(true);
        let detail = gtk::Label::new(None);
        detail.add_css_class("caption");
        detail.add_css_class("dim-label");
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        text.append(&title);
        text.append(&detail);
        root.append(&icon);
        root.append(&text);
        root.set_visible(false);
        Self { root, icon, title, detail }
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
    }

    pub fn show_message(&self, title: &str, detail: &str) {
        self.icon.set_visible(false);
        self.title.set_markup(&glib::markup_escape_text(title));
        self.detail.set_label(detail);
        self.root.set_visible(true);
    }

    pub fn show_feedback(&self, feedback: &crate::review::MoveFeedback, scale: i32) {
        let class = feedback.class;
        if let Some(texture) = crate::ui::art::review_icon_texture(class, (32 * scale.max(1)) as u32) {
            self.icon.set_paintable(Some(&texture));
            self.icon.set_visible(true);
        }
        self.title.set_markup(&format!(
            "<span foreground=\"{}\">{} {}</span>",
            class.color(),
            glib::markup_escape_text(&feedback.san),
            class.article_phrase()
        ));
        let mut detail = format!("Evaluation {}", feedback.score_after_white.display());
        if !feedback.played_best
            && !matches!(class, MoveClass::Book | MoveClass::Forced)
            && let Some(best) = &feedback.best_san
        {
            detail.push_str(&format!(" · Best was {best} ({})", feedback.score_best_white.display()));
        }
        self.detail.set_label(&detail);
        self.root.set_visible(true);
    }
}

impl Default for FeedbackCard {
    fn default() -> Self {
        Self::new()
    }
}

/// Banner shown while a variation is being explored.
pub struct ExplorationBanner {
    pub root: gtk::Box,
    label: gtk::Label,
    pub back: gtk::Button,
}

impl ExplorationBanner {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.add_css_class("card");
        root.add_css_class("exploration-banner");
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_wrap(true);
        let back = gtk::Button::with_label("Back to Game");
        back.add_css_class("suggested-action");
        back.set_valign(gtk::Align::Center);
        root.append(&label);
        root.append(&back);
        root.set_visible(false);
        Self { root, label, back }
    }

    pub fn show(&self, from_move: usize) {
        self.label.set_markup(&format!(
            "<b>Analysis line</b>\n<span size=\"small\" alpha=\"70%\">Your moves branch from move {from_move}. The game is untouched.</span>"
        ));
        self.root.set_visible(true);
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
    }
}

impl Default for ExplorationBanner {
    fn default() -> Self {
        Self::new()
    }
}
