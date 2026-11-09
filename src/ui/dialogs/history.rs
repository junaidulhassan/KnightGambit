use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use shakmaty::{Color, Position};

use crate::chess::{CapturedPieces, Game, material_balance, parse_pgn};
use crate::history::{
    GameFilter, GameRecord, Outcome, SortKey, StoredReview, color_label, opening_families, opponent_levels, select,
};
use crate::review::MoveClass;
use crate::ui::art::review_icon_texture;
use crate::ui::board::{AnimationHint, BoardFrame, BoardWidget};
use crate::ui::controller::{Controller, appearance_from};
use crate::ui::widgets::{AvatarKind, MoveList, PlayerBar};

const PAGE_SIZE: usize = 150;

pub fn format_date(timestamp: i64) -> String {
    glib::DateTime::from_unix_local(timestamp)
        .ok()
        .and_then(|date| date.format("%d %b %Y, %H:%M").ok())
        .map(|text| text.to_string())
        .unwrap_or_default()
}

pub fn format_short_date(timestamp: i64) -> String {
    glib::DateTime::from_unix_local(timestamp)
        .ok()
        .and_then(|date| date.format("%d %b").ok())
        .map(|text| text.to_string())
        .unwrap_or_default()
}

pub fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60)
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

pub fn class_of(label: &str) -> Option<MoveClass> {
    MoveClass::ALL.iter().copied().find(|class| class.label() == label)
}

fn outcome_css(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Win => "win",
        Outcome::Loss => "loss",
        Outcome::Draw => "draw",
    }
}

fn outcome_letter(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Win => "W",
        Outcome::Loss => "L",
        Outcome::Draw => "D",
    }
}

fn dropdown(items: &[&str]) -> gtk::DropDown {
    let dropdown = gtk::DropDown::new(Some(gtk::StringList::new(items)), gtk::Expression::NONE);
    dropdown.set_hexpand(true);
    dropdown
}

fn labeled(label: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let caption = gtk::Label::new(Some(label));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    caption.set_xalign(0.0);
    row.append(&caption);
    row.append(widget);
    row
}

struct Loaded {
    record: GameRecord,
    game: Game,
    review: Option<StoredReview>,
}

struct HistoryView {
    controller: Rc<Controller>,
    dialog: adw::Dialog,
    split: adw::NavigationSplitView,
    games: RefCell<Vec<GameRecord>>,
    visible: RefCell<Vec<GameRecord>>,
    filter: RefCell<GameFilter>,
    sort: Cell<(SortKey, bool)>,
    shown: Cell<usize>,
    suppress: Cell<bool>,
    list: gtk::ListBox,
    list_stack: gtk::Stack,
    empty_page: adw::StatusPage,
    count: gtk::Label,
    search: gtk::SearchEntry,
    outcome: gtk::DropDown,
    color: gtk::DropDown,
    opponent: gtk::DropDown,
    opening: gtk::DropDown,
    range: gtk::DropDown,
    from: gtk::Entry,
    to: gtk::Entry,
    sort_key: gtk::DropDown,
    sort_order: gtk::ToggleButton,
    filter_button: gtk::MenuButton,
    opponent_options: RefCell<Vec<String>>,
    opening_options: RefCell<Vec<String>>,
    detail_stack: gtk::Stack,
    board: BoardWidget,
    top_bar: PlayerBar,
    bottom_bar: PlayerBar,
    move_list: Rc<MoveList>,
    info: gtk::Box,
    badge: gtk::Label,
    first: gtk::Button,
    previous: gtk::Button,
    next: gtk::Button,
    last: gtk::Button,
    play: gtk::ToggleButton,
    review_button: adw::ButtonContent,
    open_button: gtk::Button,
    review_action: gtk::Button,
    export_button: gtk::Button,
    copy_button: gtk::Button,
    loaded: RefCell<Option<Loaded>>,
    ply: Cell<usize>,
    autoplay: RefCell<Option<glib::SourceId>>,
    selected_id: Cell<Option<i64>>,
}

pub fn present(controller: &Rc<Controller>) {
    let view = HistoryView::build(controller);
    // Keep the view alive while the dialog is open; closing releases it.
    let keeper = Rc::new(RefCell::new(Some(view.clone())));
    view.dialog.connect_closed(move |_| {
        keeper.borrow_mut().take();
    });
    view.dialog.present(Some(&controller.ui().window));
    view.reload(true);
}

impl HistoryView {
    fn build(controller: &Rc<Controller>) -> Rc<Self> {
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search openings, opponents, results…")
            .hexpand(true)
            .build();
        let outcome = dropdown(&["Any result", "Wins", "Losses", "Draws"]);
        let color = dropdown(&["Either color", "White", "Black"]);
        let opponent = dropdown(&["Any opponent"]);
        let opening = dropdown(&["Any opening"]);
        let range = dropdown(&["Any time", "Today", "Last 7 days", "Last 30 days", "Last 90 days", "Last 12 months"]);
        let from = gtk::Entry::builder().placeholder_text("YYYY-MM-DD").max_length(10).build();
        let to = gtk::Entry::builder().placeholder_text("YYYY-MM-DD").max_length(10).build();
        let labels: Vec<&str> = SortKey::ALL.iter().map(|key| key.label()).collect();
        let sort_key = dropdown(&labels);
        let sort_order = gtk::ToggleButton::builder()
            .icon_name("view-sort-descending-symbolic")
            .active(true)
            .tooltip_text("Newest / highest first")
            .valign(gtk::Align::End)
            .build();

        let filters = gtk::Box::new(gtk::Orientation::Vertical, 10);
        filters.set_margin_top(12);
        filters.set_margin_bottom(12);
        filters.set_margin_start(12);
        filters.set_margin_end(12);
        filters.set_width_request(300);
        filters.append(&labeled("Result", &outcome));
        filters.append(&labeled("Playing color", &color));
        filters.append(&labeled("Opponent level", &opponent));
        filters.append(&labeled("Opening", &opening));
        filters.append(&labeled("Date range", &range));
        let custom = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        custom.set_homogeneous(true);
        custom.append(&labeled("From", &from));
        custom.append(&labeled("To", &to));
        filters.append(&custom);
        let sort_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        sort_row.append(&labeled("Sort by", &sort_key));
        sort_row.append(&sort_order);
        filters.append(&sort_row);
        let reset = gtk::Button::with_label("Reset Filters");
        reset.add_css_class("flat");
        filters.append(&reset);
        let popover = gtk::Popover::new();
        popover.set_child(Some(&filters));
        let filter_button = gtk::MenuButton::builder()
            .icon_name("kg-filter-symbolic")
            .tooltip_text("Filter and sort")
            .popover(&popover)
            .build();

        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        search_row.set_margin_start(10);
        search_row.set_margin_end(10);
        search_row.set_margin_top(8);
        search_row.append(&search);
        search_row.append(&filter_button);
        let count = gtk::Label::new(None);
        count.add_css_class("caption");
        count.add_css_class("dim-label");
        count.set_xalign(0.0);
        count.set_margin_start(14);
        count.set_margin_top(4);
        count.set_margin_bottom(4);

        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk::SelectionMode::Single);
        let list_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        let empty_page = adw::StatusPage::builder()
            .icon_name("kg-history-symbolic")
            .title("No games yet")
            .description("Games you finish against the computer are saved here automatically.")
            .build();
        let list_stack = gtk::Stack::new();
        list_stack.add_named(&list_scroll, Some("list"));
        list_stack.add_named(&empty_page, Some("empty"));
        list_stack.set_vexpand(true);

        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.append(&search_row);
        sidebar_box.append(&count);
        sidebar_box.append(&list_stack);
        let sidebar_toolbar = adw::ToolbarView::new();
        sidebar_toolbar.add_top_bar(&adw::HeaderBar::builder().show_end_title_buttons(false).build());
        sidebar_toolbar.set_content(Some(&sidebar_box));
        let sidebar_page = adw::NavigationPage::new(&sidebar_toolbar, "Game History");

        // ---- detail ----
        let board = BoardWidget::new();
        board.set_movable(false, false);
        let top_bar = PlayerBar::new();
        let bottom_bar = PlayerBar::new();
        top_bar.set_clock(None, false, false);
        bottom_bar.set_clock(None, false, false);
        let frame = BoardFrame::new(&top_bar.root, &board, &bottom_bar.root);
        frame.set_margin_top(6);
        frame.set_margin_bottom(6);
        frame.set_margin_start(12);
        frame.set_margin_end(12);
        let badge = gtk::Label::new(None);
        badge.set_xalign(0.5);
        badge.set_wrap(true);
        badge.set_justify(gtk::Justification::Center);
        badge.add_css_class("heading");

        let first = gtk::Button::from_icon_name("media-skip-backward-symbolic");
        first.set_tooltip_text(Some("First position (Home)"));
        let previous = gtk::Button::from_icon_name("go-previous-symbolic");
        previous.set_tooltip_text(Some("Previous move (Left)"));
        let play = gtk::ToggleButton::builder().icon_name("media-playback-start-symbolic").tooltip_text("Play the game (Space)").build();
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        next.set_tooltip_text(Some("Next move (Right)"));
        let last = gtk::Button::from_icon_name("media-skip-forward-symbolic");
        last.set_tooltip_text(Some("Final position (End)"));
        let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        navigation.add_css_class("linked");
        navigation.set_halign(gtk::Align::Center);
        for button in [first.upcast_ref::<gtk::Widget>(), previous.upcast_ref(), play.upcast_ref(), next.upcast_ref(), last.upcast_ref()] {
            navigation.append(button);
        }
        let flip = gtk::Button::from_icon_name("object-flip-vertical-symbolic");
        flip.set_tooltip_text(Some("Flip board"));
        flip.add_css_class("flat");
        let nav_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        nav_row.set_halign(gtk::Align::Center);
        nav_row.append(&navigation);
        nav_row.append(&flip);

        let board_column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        board_column.set_hexpand(true);
        board_column.append(&frame);
        board_column.append(&badge);
        board_column.append(&nav_row);
        board_column.set_margin_bottom(10);

        let move_list = MoveList::new();
        let moves_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        moves_card.add_css_class("card");
        moves_card.set_height_request(180);
        moves_card.set_vexpand(true);
        let moves_title = gtk::Label::new(Some("Moves"));
        moves_title.add_css_class("heading");
        moves_title.add_css_class("card-header");
        moves_title.set_xalign(0.0);
        moves_card.append(&moves_title);
        moves_card.append(&move_list.root);

        let info = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let open_button = gtk::Button::with_label("Open on Board");
        open_button.set_tooltip_text(Some("Load this game into the main window"));
        let review_content = adw::ButtonContent::builder().icon_name("kg-review-symbolic").label("Review with Stockfish").build();
        let review_action = gtk::Button::builder().child(&review_content).build();
        review_action.add_css_class("suggested-action");
        let export_button = gtk::Button::with_label("Save PGN…");
        let copy_button = gtk::Button::with_label("Copy PGN");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_homogeneous(true);
        buttons.append(&export_button);
        buttons.append(&copy_button);

        let side = gtk::Box::new(gtk::Orientation::Vertical, 10);
        side.set_width_request(310);
        side.set_margin_top(6);
        side.set_margin_bottom(12);
        side.set_margin_end(12);
        side.append(&info);
        side.append(&moves_card);
        side.append(&review_action);
        side.append(&open_button);
        side.append(&buttons);

        let detail_content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        detail_content.append(&board_column);
        detail_content.append(&side);
        let detail_scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&detail_content).build();
        let placeholder = adw::StatusPage::builder()
            .icon_name("kg-history-symbolic")
            .title("Select a game")
            .description("Pick a game from the list to replay it move by move.")
            .build();
        let detail_stack = gtk::Stack::new();
        detail_stack.add_named(&placeholder, Some("placeholder"));
        detail_stack.add_named(&detail_scroll, Some("detail"));

        // Header actions
        let manage = gtk::Popover::new();
        let manage_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        manage_box.set_margin_top(6);
        manage_box.set_margin_bottom(6);
        manage_box.set_margin_start(6);
        manage_box.set_margin_end(6);
        let export_all = gtk::Button::with_label("Export All Games as PGN…");
        let export_shown = gtk::Button::with_label("Export Shown Games as PGN…");
        let backup = gtk::Button::with_label("Back Up Database…");
        for button in [&export_all, &export_shown, &backup] {
            button.add_css_class("flat");
            manage_box.append(button);
        }
        manage.set_child(Some(&manage_box));
        let manage_button = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text("Export and back up")
            .popover(&manage)
            .build();
        let detail_header = adw::HeaderBar::new();
        detail_header.pack_end(&manage_button);
        let detail_toolbar = adw::ToolbarView::new();
        detail_toolbar.add_top_bar(&detail_header);
        detail_toolbar.set_content(Some(&detail_stack));
        let detail_page = adw::NavigationPage::new(&detail_toolbar, "Replay");

        let split = adw::NavigationSplitView::new();
        split.set_sidebar(Some(&sidebar_page));
        split.set_content(Some(&detail_page));
        split.set_min_sidebar_width(300.0);
        split.set_max_sidebar_width(420.0);
        split.set_sidebar_width_fraction(0.34);

        let bin = adw::BreakpointBin::new();
        bin.set_width_request(360);
        bin.set_height_request(480);
        bin.set_child(Some(&split));
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            860.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        bin.add_breakpoint(breakpoint);

        let dialog = adw::Dialog::builder()
            .title("Game History")
            .content_width(1220)
            .content_height(800)
            .child(&bin)
            .build();

        let view = Rc::new(Self {
            controller: controller.clone(),
            dialog,
            split,
            games: RefCell::new(Vec::new()),
            visible: RefCell::new(Vec::new()),
            filter: RefCell::new(GameFilter::default()),
            sort: Cell::new((SortKey::Date, true)),
            shown: Cell::new(PAGE_SIZE),
            suppress: Cell::new(false),
            list,
            list_stack,
            empty_page,
            count,
            search,
            outcome,
            color,
            opponent,
            opening,
            range,
            from,
            to,
            sort_key,
            sort_order,
            filter_button,
            opponent_options: RefCell::new(Vec::new()),
            opening_options: RefCell::new(Vec::new()),
            detail_stack,
            board,
            top_bar,
            bottom_bar,
            move_list,
            info,
            badge,
            first,
            previous,
            next,
            last,
            play,
            review_button: review_content,
            open_button,
            review_action,
            export_button,
            copy_button,
            loaded: RefCell::new(None),
            ply: Cell::new(0),
            autoplay: RefCell::new(None),
            selected_id: Cell::new(None),
        });
        view.apply_board_settings();
        view.connect(&reset, &flip, &export_all, &export_shown, &backup);
        view
    }

    fn apply_board_settings(&self) {
        self.board.set_appearance(appearance_from(&self.controller.settings().get()));
    }

    fn connect(
        self: &Rc<Self>,
        reset: &gtk::Button,
        flip: &gtk::Button,
        export_all: &gtk::Button,
        export_shown: &gtk::Button,
        backup: &gtk::Button,
    ) {
        let weak = Rc::downgrade(self);
        let refilter = move || {
            if let Some(view) = weak.upgrade() {
                view.on_filter_changed();
            }
        };
        {
            let refilter = refilter.clone();
            self.search.connect_search_changed(move |_| refilter());
        }
        for dropdown in [&self.outcome, &self.color, &self.opponent, &self.opening, &self.range, &self.sort_key] {
            let refilter = refilter.clone();
            dropdown.connect_selected_notify(move |_| refilter());
        }
        for entry in [&self.from, &self.to] {
            let refilter = refilter.clone();
            entry.connect_changed(move |_| refilter());
        }
        {
            let refilter = refilter.clone();
            self.sort_order.connect_toggled(move |button| {
                button.set_icon_name(if button.is_active() { "view-sort-descending-symbolic" } else { "view-sort-ascending-symbolic" });
                refilter();
            });
        }
        let weak = Rc::downgrade(self);
        reset.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.reset_filters();
            }
        });
        let weak = Rc::downgrade(self);
        self.list.connect_row_selected(move |_, row| {
            let (Some(view), Some(row)) = (weak.upgrade(), row) else { return };
            if view.suppress.get() {
                return;
            }
            let index = row.index();
            if index < 0 {
                return;
            }
            let record = view.visible.borrow().get(index as usize).cloned();
            if let Some(record) = record {
                view.select_game(record.id);
            }
        });

        let nav = |button: &gtk::Button, action: fn(&HistoryView)| {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(view) = weak.upgrade() {
                    view.stop_autoplay();
                    action(&view);
                }
            });
        };
        nav(&self.first, |v| v.go_to(0));
        nav(&self.previous, |v| v.step(-1));
        nav(&self.next, |v| v.step(1));
        nav(&self.last, |v| v.go_to(usize::MAX));
        let weak = Rc::downgrade(self);
        flip.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.board.set_flipped(!view.board.is_flipped());
                view.refresh_bars();
            }
        });
        let weak = Rc::downgrade(self);
        self.play.connect_toggled(move |button| {
            if let Some(view) = weak.upgrade() {
                if button.is_active() {
                    view.start_autoplay();
                } else {
                    view.stop_autoplay();
                }
            }
        });

        let weak = Rc::downgrade(self);
        self.open_button.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade()
                && let Some(id) = view.selected_id.get()
            {
                view.dialog.close();
                view.controller.confirm_open_history_game(id, false);
            }
        });
        let weak = Rc::downgrade(self);
        self.review_action.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade()
                && let Some(id) = view.selected_id.get()
            {
                view.dialog.close();
                view.controller.confirm_open_history_game(id, true);
            }
        });
        let weak = Rc::downgrade(self);
        self.copy_button.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade()
                && let Some(loaded) = view.loaded.borrow().as_ref()
            {
                view.controller.copy_to_clipboard(&loaded.record.pgn, "PGN");
            }
        });
        let weak = Rc::downgrade(self);
        self.export_button.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade()
                && let Some(id) = view.selected_id.get()
            {
                view.export_pgn(Some(vec![id]), "knightgambit-game.pgn");
            }
        });
        let weak = Rc::downgrade(self);
        export_all.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.export_pgn(None, "knightgambit-history.pgn");
            }
        });
        let weak = Rc::downgrade(self);
        export_shown.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                let ids = view.visible.borrow().iter().map(|game| game.id).collect();
                view.export_pgn(Some(ids), "knightgambit-selection.pgn");
            }
        });
        let weak = Rc::downgrade(self);
        backup.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.backup_database();
            }
        });

        // Keyboard replay.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(view) = weak.upgrade() else { return glib::Propagation::Proceed };
            if view.search.has_focus() || view.from.has_focus() || view.to.has_focus() || view.loaded.borrow().is_none() {
                return glib::Propagation::Proceed;
            }
            use gtk::gdk::Key;
            match key {
                Key::Left | Key::KP_Left => view.step(-1),
                Key::Right | Key::KP_Right => view.step(1),
                Key::Home | Key::KP_Home => view.go_to(0),
                Key::End | Key::KP_End => view.go_to(usize::MAX),
                Key::space => view.play.set_active(!view.play.is_active()),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        self.dialog.add_controller(keys);

        let weak = Rc::downgrade(self);
        self.dialog.connect_closed(move |_| {
            if let Some(view) = weak.upgrade() {
                view.stop_autoplay();
            }
        });

        // Refresh when a game is recorded or reviewed while the dialog is open.
        let weak = Rc::downgrade(self);
        self.controller.connect_history_changed(move || match weak.upgrade() {
            Some(view) => {
                if view.dialog.is_mapped() {
                    view.reload(false);
                }
                true
            }
            None => false,
        });
    }

    // ---------------------------------------------------------------- data ----

    fn reload(self: &Rc<Self>, first: bool) {
        let store = self.controller.history().clone();
        let weak: Weak<Self> = Rc::downgrade(self);
        self.controller.worker().run(
            move || store.list_games(),
            move |result| {
                let Some(view) = weak.upgrade() else { return };
                match result {
                    Ok(games) => {
                        view.games.replace(games);
                        view.refresh_options();
                        view.apply_filter(first);
                        if view.selected_id.get().is_some() && !first {
                            view.refresh_selected();
                        }
                    }
                    Err(error) => view.controller.toast(&format!("Could not load the game history: {error}")),
                }
            },
        );
    }

    fn refresh_options(&self) {
        let games = self.games.borrow();
        let levels = opponent_levels(&games);
        let openings = opening_families(&games);
        self.suppress.set(true);
        let selected_level = self.filter.borrow().opponent.clone();
        let selected_opening = self.filter.borrow().opening.clone();
        let mut level_items = vec!["Any opponent".to_owned()];
        level_items.extend(levels.iter().cloned());
        let mut opening_items = vec!["Any opening".to_owned()];
        opening_items.extend(openings.iter().cloned());
        self.opponent.set_model(Some(&gtk::StringList::new(&refs(&level_items))));
        self.opening.set_model(Some(&gtk::StringList::new(&refs(&opening_items))));
        self.opponent.set_selected(selected_level.and_then(|l| levels.iter().position(|x| *x == l)).map_or(0, |i| i as u32 + 1));
        self.opening.set_selected(selected_opening.and_then(|o| openings.iter().position(|x| *x == o)).map_or(0, |i| i as u32 + 1));
        self.opponent_options.replace(levels);
        self.opening_options.replace(openings);
        self.suppress.set(false);
    }

    fn date_bounds(&self) -> (Option<i64>, Option<i64>) {
        let parse = |entry: &gtk::Entry, end_of_day: bool| -> Option<i64> {
            let text = entry.text();
            let mut parts = text.trim().split('-').map(|part| part.parse::<i32>().ok());
            let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
            let date = glib::DateTime::new(&glib::TimeZone::local(), year, month, day, 0, 0, 0.0).ok()?;
            Some(date.to_unix() + if end_of_day { 86_399 } else { 0 })
        };
        let custom_from = parse(&self.from, false);
        let custom_to = parse(&self.to, true);
        if custom_from.is_some() || custom_to.is_some() {
            return (custom_from, custom_to);
        }
        let now = glib::DateTime::now_local().ok();
        let days = |count: i64| now.as_ref().map(|now| now.to_unix() - count * 86_400);
        let from = match self.range.selected() {
            1 => now.as_ref().and_then(|now| {
                glib::DateTime::new(&glib::TimeZone::local(), now.year(), now.month(), now.day_of_month(), 0, 0, 0.0)
                    .ok()
                    .map(|d| d.to_unix())
            }),
            2 => days(7),
            3 => days(30),
            4 => days(90),
            5 => days(365),
            _ => None,
        };
        (from, None)
    }

    fn read_filter(&self) -> GameFilter {
        let (from, to) = self.date_bounds();
        let pick = |options: &RefCell<Vec<String>>, dropdown: &gtk::DropDown| {
            dropdown.selected().checked_sub(1).and_then(|index| options.borrow().get(index as usize).cloned())
        };
        GameFilter {
            search: self.search.text().to_string(),
            outcome: match self.outcome.selected() {
                1 => Some(Outcome::Win),
                2 => Some(Outcome::Loss),
                3 => Some(Outcome::Draw),
                _ => None,
            },
            color: match self.color.selected() {
                1 => Some(Color::White),
                2 => Some(Color::Black),
                _ => None,
            },
            opponent: pick(&self.opponent_options, &self.opponent),
            opening: pick(&self.opening_options, &self.opening),
            from,
            to,
        }
    }

    fn on_filter_changed(self: &Rc<Self>) {
        if self.suppress.get() {
            return;
        }
        self.shown.set(PAGE_SIZE);
        self.apply_filter(false);
    }

    fn reset_filters(self: &Rc<Self>) {
        self.suppress.set(true);
        self.search.set_text("");
        for dropdown in [&self.outcome, &self.color, &self.opponent, &self.opening, &self.range] {
            dropdown.set_selected(0);
        }
        self.from.set_text("");
        self.to.set_text("");
        self.sort_key.set_selected(0);
        self.sort_order.set_active(true);
        self.suppress.set(false);
        self.shown.set(PAGE_SIZE);
        self.filter.replace(GameFilter::default());
        self.sort.set((SortKey::Date, true));
        self.rebuild_list(false);
    }

    fn apply_filter(self: &Rc<Self>, select_first: bool) {
        let filter = self.read_filter();
        let key = SortKey::ALL.get(self.sort_key.selected() as usize).copied().unwrap_or(SortKey::Date);
        let descending = self.sort_order.is_active();
        self.filter.replace(filter);
        self.sort.set((key, descending));
        self.rebuild_list(select_first);
    }

    fn rebuild_list(self: &Rc<Self>, select_first: bool) {
        let filter = self.filter.borrow().clone();
        let (key, descending) = self.sort.get();
        let all = self.games.borrow();
        let visible = select(&all, &filter, key, descending);
        let total = all.len();
        drop(all);
        if filter_narrows(&filter) {
            self.filter_button.add_css_class("suggested-action");
        } else {
            self.filter_button.remove_css_class("suggested-action");
        }

        self.suppress.set(true);
        self.list.remove_all();
        let shown = self.shown.get().min(visible.len());
        for game in visible.iter().take(shown) {
            self.list.append(&game_row(game));
        }
        if shown < visible.len() {
            let more = gtk::Button::with_label(&format!("Show {} more games", (visible.len() - shown).min(PAGE_SIZE)));
            more.add_css_class("flat");
            more.set_margin_top(6);
            more.set_margin_bottom(6);
            let weak = Rc::downgrade(self);
            more.connect_clicked(move |_| {
                if let Some(view) = weak.upgrade() {
                    view.shown.set(view.shown.get() + PAGE_SIZE);
                    view.rebuild_list(false);
                }
            });
            let row = gtk::ListBoxRow::builder().activatable(false).selectable(false).child(&more).build();
            self.list.append(&row);
        }
        self.visible.replace(visible);
        self.suppress.set(false);

        let visible_count = self.visible.borrow().len();
        self.count.set_label(&match (total, visible_count) {
            (0, _) => String::new(),
            (total, shown) if shown == total => format!("{total} game{}", if total == 1 { "" } else { "s" }),
            (total, shown) => format!("{shown} of {total} games"),
        });
        if total == 0 {
            self.empty_page.set_title("No games yet");
            self.empty_page.set_description(Some("Games you finish against the computer are saved here automatically."));
            self.list_stack.set_visible_child_name("empty");
        } else if visible_count == 0 {
            self.empty_page.set_title("No matching games");
            self.empty_page.set_description(Some("Try different search words or reset the filters."));
            self.list_stack.set_visible_child_name("empty");
        } else {
            self.list_stack.set_visible_child_name("list");
        }

        // Keep the open game selected if it is still listed; otherwise optionally pick the first.
        let visible = self.visible.borrow();
        let keep = self.selected_id.get().and_then(|id| visible.iter().take(self.shown.get()).position(|game| game.id == id));
        match (keep, select_first) {
            (Some(index), _) => {
                self.suppress.set(true);
                if let Some(row) = self.list.row_at_index(index as i32) {
                    self.list.select_row(Some(&row));
                }
                self.suppress.set(false);
            }
            (None, true) if !visible.is_empty() && !self.split.is_collapsed() => {
                let first = visible[0].id;
                drop(visible);
                if let Some(row) = self.list.row_at_index(0) {
                    self.suppress.set(true);
                    self.list.select_row(Some(&row));
                    self.suppress.set(false);
                }
                self.select_game(first);
            }
            _ => {}
        }
    }

    // -------------------------------------------------------------- detail ----

    fn select_game(self: &Rc<Self>, id: i64) {
        self.selected_id.set(Some(id));
        self.load_detail(id, true);
    }

    fn refresh_selected(self: &Rc<Self>) {
        if let Some(id) = self.selected_id.get() {
            self.load_detail(id, false);
        }
    }

    fn load_detail(self: &Rc<Self>, id: i64, reset_position: bool) {
        let store = self.controller.history().clone();
        let weak = Rc::downgrade(self);
        self.controller.worker().run(
            move || {
                let record = store.get_game(id)?;
                let review = store.load_review(id)?;
                Ok::<_, crate::history::HistoryError>((record, review))
            },
            move |result| {
                let Some(view) = weak.upgrade() else { return };
                if view.selected_id.get() != Some(id) {
                    return;
                }
                match result {
                    Ok((Some(record), review)) => view.show_game(record, review, reset_position),
                    Ok((None, _)) => view.controller.toast("That game is no longer in your history"),
                    Err(error) => view.controller.toast(&format!("Could not open the game: {error}")),
                }
            },
        );
    }

    fn show_game(self: &Rc<Self>, record: GameRecord, review: Option<StoredReview>, reset_position: bool) {
        let game = match parse_pgn(&record.pgn) {
            Ok(parsed) => parsed.game,
            Err(error) => {
                self.controller.toast(&format!("The stored game could not be read: {error}"));
                return;
            }
        };
        self.stop_autoplay();
        let previous_ply = self.ply.get();
        let same = self.loaded.borrow().as_ref().is_some_and(|loaded| loaded.record.id == record.id);
        self.board.set_flipped(record.user_color == Color::Black);
        self.build_info(&record, review.as_ref());
        self.review_button.set_label(if review.is_some() { "Re-review with Stockfish" } else { "Review with Stockfish" });
        let ply_count = game.ply_count();
        self.loaded.replace(Some(Loaded { record, game, review }));
        self.detail_stack.set_visible_child_name("detail");
        self.split.set_show_content(true);
        let target = if reset_position || !same { ply_count } else { previous_ply.min(ply_count) };
        self.ply.set(usize::MAX);
        self.go_to_with_hint(target, None);
    }

    fn build_info(&self, record: &GameRecord, review: Option<&StoredReview>) {
        while let Some(child) = self.info.first_child() {
            self.info.remove(&child);
        }
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.add_css_class("title-3");
        title.set_label(record.opening_name.as_deref().unwrap_or("Opening not in the book"));
        self.info.append(&title);
        let result = gtk::Label::new(None);
        result.set_xalign(0.0);
        result.add_css_class("history-result-line");
        result.add_css_class(outcome_css(record.outcome));
        result.set_label(&format!("{} · {} · {}", record.outcome.label(), record.score, record.termination));
        self.info.append(&result);

        let grid = gtk::Grid::new();
        grid.set_column_spacing(12);
        grid.set_row_spacing(4);
        let mut rows: Vec<(&str, String)> = vec![
            ("Played", format_date(record.played_at)),
            ("You played", color_label(record.user_color).to_owned()),
            (
                "Opponent",
                match record.opponent_elo {
                    Some(elo) => format!("{} · {} (≈{elo})", record.opponent_name, record.opponent_level),
                    None => format!("{} · {}", record.opponent_name, record.opponent_level),
                },
            ),
        ];
        if let Some(eco) = &record.eco {
            rows.push(("ECO", eco.clone()));
        }
        rows.push(("Moves", format!("{}", (record.ply_count + 1) / 2)));
        if record.duration_secs > 0 {
            rows.push(("Duration", format_duration(record.duration_secs)));
        }
        if let Some(control) = &record.time_control {
            rows.push(("Time control", control.clone()));
        }
        if !record.start_fen.is_empty() {
            rows.push(("Start", "Custom position".to_owned()));
        }
        for (row, (name, value)) in rows.iter().enumerate() {
            let key = gtk::Label::new(Some(name));
            key.add_css_class("dim-label");
            key.set_xalign(0.0);
            key.set_valign(gtk::Align::Start);
            let text = gtk::Label::new(Some(value));
            text.set_xalign(0.0);
            text.set_wrap(true);
            text.set_selectable(true);
            text.set_hexpand(true);
            grid.attach(&key, 0, row as i32, 1, 1);
            grid.attach(&text, 1, row as i32, 1, 1);
        }
        self.info.append(&grid);

        let heading = gtk::Label::new(Some("Engine review"));
        heading.add_css_class("heading");
        heading.set_xalign(0.0);
        heading.set_margin_top(6);
        self.info.append(&heading);
        match review {
            None => {
                let note = gtk::Label::new(Some("Not reviewed yet. Review this game with Stockfish to see the quality of every move here and in your statistics."));
                note.add_css_class("dim-label");
                note.set_wrap(true);
                note.set_xalign(0.0);
                self.info.append(&note);
            }
            Some(review) => {
                let (yours, theirs) = match record.user_color {
                    Color::White => (review.summary.white_accuracy, review.summary.black_accuracy),
                    Color::Black => (review.summary.black_accuracy, review.summary.white_accuracy),
                };
                let accuracy = gtk::Label::new(None);
                accuracy.set_xalign(0.0);
                accuracy.set_markup(&format!(
                    "Accuracy  <b>{yours:.0}%</b> you · {theirs:.0}% opponent\n<span alpha=\"65%\">{} · {:.1} s per position</span>",
                    glib::markup_escape_text(&review.summary.engine),
                    review.summary.movetime_ms as f64 / 1000.0
                ));
                accuracy.set_wrap(true);
                self.info.append(&accuracy);
                let mut counts = [0usize; crate::review::CLASS_COUNT];
                for row in review.moves.iter().filter(|row| row.mover == record.user_color) {
                    if let Some(class) = class_of(&row.class) {
                        counts[class.index()] += 1;
                    }
                }
                let flow = gtk::FlowBox::new();
                flow.set_selection_mode(gtk::SelectionMode::None);
                flow.set_max_children_per_line(4);
                flow.set_row_spacing(4);
                flow.set_column_spacing(4);
                for class in MoveClass::ALL {
                    let count = counts[class.index()];
                    if count == 0 {
                        continue;
                    }
                    let chip = gtk::Label::new(None);
                    chip.set_markup(&format!(
                        "<span foreground=\"{}\" weight=\"bold\">{count}</span> {}",
                        class.color(),
                        class.label()
                    ));
                    chip.add_css_class("class-chip");
                    flow.append(&chip);
                }
                self.info.append(&flow);
            }
        }
    }

    fn step(&self, delta: i64) {
        let target = (self.ply.get() as i64 + delta).max(0) as usize;
        self.go_to(target);
    }

    fn go_to(&self, target: usize) {
        let current = self.ply.get();
        let total = self.loaded.borrow().as_ref().map_or(0, |loaded| loaded.game.ply_count());
        let target = target.min(total);
        let hint = {
            let loaded = self.loaded.borrow();
            loaded.as_ref().and_then(|loaded| {
                if target == current + 1 {
                    loaded.game.moves().get(current).map(|played| AnimationHint::Forward(played.mv))
                } else if target + 1 == current {
                    loaded.game.moves().get(target).map(|played| AnimationHint::Backward(played.mv))
                } else {
                    None
                }
            })
        };
        self.go_to_with_hint(target, hint);
    }

    fn go_to_with_hint(&self, target: usize, hint: Option<AnimationHint>) {
        let loaded = self.loaded.borrow();
        let Some(loaded) = loaded.as_ref() else { return };
        let target = target.min(loaded.game.ply_count());
        if self.ply.get() == target {
            return;
        }
        self.ply.set(target);
        let position = loaded.game.position_at(target);
        let last = target.checked_sub(1).and_then(|index| loaded.game.moves().get(index)).map(|played| {
            (played.mv.from().unwrap_or_else(|| played.mv.to()), Game::destination_of(&played.mv))
        });
        self.board.set_position(position, last, hint);
        let check = loaded.game.checked_king(target);
        let mate = (target == loaded.game.ply_count() && position.is_checkmate()).then_some(check).flatten();
        self.board.set_status(check, mate);
        self.board.set_movable(false, false);

        // Review badge on the board and caption.
        let review_row = loaded.review.as_ref().and_then(|review| review.moves.get(target.wrapping_sub(1)));
        let class = review_row.and_then(|row| class_of(&row.class));
        match (class, loaded.game.moves().get(target.wrapping_sub(1))) {
            (Some(class), Some(played)) => {
                let square = Game::destination_of(&played.mv);
                let scale = self.board.imp_device_scale();
                let pixels = (self.board.width().max(64) as f64 / 8.0 * 0.38 * scale).ceil() as u32;
                self.board.set_review_marker(review_icon_texture(class, pixels.max(16)).map(|texture| (square, texture)));
            }
            _ => self.board.set_review_marker(None),
        }
        self.board.set_arrow(None);
        match (target, review_row) {
            (0, _) => self.badge.set_markup("<span alpha=\"60%\">Starting position</span>"),
            (_, Some(row)) => {
                let class = class_of(&row.class).unwrap_or(MoveClass::Good);
                let mut text = format!(
                    "<span foreground=\"{}\">{} {}</span>",
                    class.color(),
                    glib::markup_escape_text(&row.san),
                    class.article_phrase()
                );
                if let Some(best) = &row.best_san
                    && class.is_error()
                {
                    text.push_str(&format!("\n<span alpha=\"65%\" size=\"small\">Best was {}</span>", glib::markup_escape_text(best)));
                }
                self.badge.set_markup(&text);
            }
            (_, None) => {
                let san = loaded.game.moves().get(target - 1).map(|played| played.san.clone()).unwrap_or_default();
                self.badge.set_markup(&format!("<span alpha=\"75%\">{}</span>", glib::markup_escape_text(&san)));
            }
        }
        let annotations: Vec<Option<MoveClass>> = match &loaded.review {
            Some(review) => review.moves.iter().map(|row| class_of(&row.class)).collect(),
            None => Vec::new(),
        };
        let style = self.controller.settings().with(|s| (s.notation, s.piece_style));
        let first_number = loaded.game.position_at(0).fullmoves().get();
        let black_first = loaded.game.position_at(0).turn() == Color::Black;
        let moves = loaded.game.moves().to_vec();
        self.move_list.set_moves(&moves, first_number, black_first, target, style.0, &annotations);
        self.refresh_bars();
        self.first.set_sensitive(target > 0);
        self.previous.set_sensitive(target > 0);
        let total = moves.len();
        self.next.set_sensitive(target < total);
        self.last.set_sensitive(target < total);
        if target >= total {
            self.play.set_active(false);
        }
    }

    fn refresh_bars(&self) {
        let loaded = self.loaded.borrow();
        let Some(loaded) = loaded.as_ref() else { return };
        let style = self.controller.settings().with(|s| s.piece_style);
        let record = &loaded.record;
        let ply = self.ply.get().min(loaded.game.ply_count());
        let user = record.user_color;
        let captured = CapturedPieces::from_moves(&loaded.game.moves()[..ply]);
        let balance = material_balance(loaded.game.position_at(ply).board());
        let bottom_color = if self.board.is_flipped() { Color::Black } else { Color::White };
        let user_is_bottom = user == bottom_color;
        let set_user = |bar: &PlayerBar| bar.set_player("You", color_label(user), AvatarKind::Human(user), style);
        let set_opponent = |bar: &PlayerBar| {
            bar.set_player(
                &record.opponent_name,
                &record.opponent_level,
                AvatarKind::Engine { name: &record.opponent_name, logo: None },
                style,
            )
        };
        if user_is_bottom {
            set_user(&self.bottom_bar);
            set_opponent(&self.top_bar);
        } else {
            set_user(&self.top_bar);
            set_opponent(&self.bottom_bar);
        }
        for (bar, color) in [(&self.bottom_bar, bottom_color), (&self.top_bar, !bottom_color)] {
            let advantage = match color {
                Color::White => balance,
                Color::Black => -balance,
            };
            bar.set_captured(captured.captured_by(color), !color, advantage.max(0), style);
        }
    }

    fn start_autoplay(self: &Rc<Self>) {
        self.stop_autoplay_source();
        let total = self.loaded.borrow().as_ref().map_or(0, |loaded| loaded.game.ply_count());
        if self.ply.get() >= total {
            self.go_to(0);
        }
        self.play.set_icon_name("media-playback-pause-symbolic");
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(900), move || {
            let Some(view) = weak.upgrade() else { return glib::ControlFlow::Break };
            view.step(1);
            if view.autoplay.borrow().is_none() {
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        self.autoplay.replace(Some(source));
    }

    fn stop_autoplay_source(&self) {
        if let Some(source) = self.autoplay.take() {
            source.remove();
        }
    }

    fn stop_autoplay(&self) {
        self.stop_autoplay_source();
        self.play.set_icon_name("media-playback-start-symbolic");
        if self.play.is_active() {
            self.play.set_active(false);
        }
    }

    // ---------------------------------------------------------- files ----

    fn export_pgn(self: &Rc<Self>, ids: Option<Vec<i64>>, name: &str) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("PGN games"));
        filter.add_pattern("*.pgn");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let chooser = gtk::FileDialog::builder().title("Export PGN").initial_name(name).filters(&filters).modal(true).build();
        let view = self.clone();
        chooser.save(Some(&self.controller.ui().window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let store = view.controller.history().clone();
            let weak = Rc::downgrade(&view);
            view.controller.worker().run(
                move || store.export_pgn(ids.as_deref()),
                move |result| {
                    let Some(view) = weak.upgrade() else { return };
                    match result {
                        Ok(text) if text.trim().is_empty() => view.controller.toast("There are no games to export"),
                        Ok(text) => {
                            let count = text.matches("[Event ").count();
                            let controller = view.controller.clone();
                            file.replace_contents_async(
                                text.into_bytes(),
                                None,
                                false,
                                gio::FileCreateFlags::REPLACE_DESTINATION,
                                gio::Cancellable::NONE,
                                move |result| match result {
                                    Ok(_) => controller.toast(&format!("Exported {count} game{}", if count == 1 { "" } else { "s" })),
                                    Err((_, error)) => controller.toast(&format!("Could not save: {error}")),
                                },
                            );
                        }
                        Err(error) => view.controller.toast(&format!("Could not export: {error}")),
                    }
                },
            );
        });
    }

    fn backup_database(self: &Rc<Self>) {
        if !self.controller.history().is_persistent() {
            self.controller.toast("The game history is not stored on disk this session");
            return;
        }
        let stamp = glib::DateTime::now_local()
            .ok()
            .and_then(|now| now.format("knightgambit-history-%Y%m%d-%H%M.db").ok())
            .map(|text| text.to_string())
            .unwrap_or_else(|| "knightgambit-history.db".to_owned());
        let chooser = gtk::FileDialog::builder().title("Back Up Game History").initial_name(stamp.as_str()).modal(true).build();
        let view = self.clone();
        chooser.save(Some(&self.controller.ui().window), gio::Cancellable::NONE, move |result| {
            let Some(path) = result.ok().and_then(|file| file.path()) else { return };
            let store = view.controller.history().clone();
            let weak = Rc::downgrade(&view);
            view.controller.worker().run(
                move || store.backup_to(&path).map(|_| path),
                move |result| {
                    let Some(view) = weak.upgrade() else { return };
                    match result {
                        Ok(path) => view.controller.toast(&format!("Backup saved to {}", path.display())),
                        Err(error) => view.controller.toast(&format!("Backup failed: {error}")),
                    }
                },
            );
        });
    }
}

fn refs(items: &[String]) -> Vec<&str> {
    items.iter().map(String::as_str).collect()
}

fn filter_narrows(filter: &GameFilter) -> bool {
    filter.outcome.is_some()
        || filter.color.is_some()
        || filter.opponent.is_some()
        || filter.opening.is_some()
        || filter.from.is_some()
        || filter.to.is_some()
}

fn game_row(game: &GameRecord) -> adw::ActionRow {
    let title = game.opening_name.clone().unwrap_or_else(|| "Opening not in the book".to_owned());
    let subtitle = format!(
        "vs {} · {} · as {} · {} moves · {}",
        game.opponent_name,
        game.opponent_level,
        color_label(game.user_color),
        (game.ply_count + 1) / 2,
        game.termination
    );
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&title))
        .subtitle(glib::markup_escape_text(&subtitle))
        .subtitle_lines(2)
        .title_lines(1)
        .build();
    let badge = gtk::Label::new(Some(outcome_letter(game.outcome)));
    badge.add_css_class("history-result");
    badge.add_css_class(outcome_css(game.outcome));
    badge.set_valign(gtk::Align::Center);
    badge.set_tooltip_text(Some(game.outcome.label()));
    row.add_prefix(&badge);
    let date = gtk::Label::new(Some(&format_date(game.played_at)));
    date.add_css_class("caption");
    date.add_css_class("dim-label");
    date.set_wrap(true);
    date.set_justify(gtk::Justification::Right);
    date.set_width_chars(10);
    date.set_max_width_chars(12);
    date.set_valign(gtk::Align::Center);
    row.add_suffix(&date);
    if game.analyzed {
        let icon = gtk::Image::from_icon_name("kg-review-symbolic");
        icon.set_tooltip_text(Some("Reviewed with Stockfish"));
        icon.add_css_class("dim-label");
        row.add_suffix(&icon);
    }
    row
}
