use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::app::{APP_ID, APP_NAME, paths};
use crate::engine::registry::EngineRegistry;
use crate::history::HistoryStore;
use crate::settings::SettingsStore;
use crate::ui::board::{BoardFrame, BoardWidget};
use crate::ui::controller::{Controller, Ui};
use crate::ui::dialogs;
use crate::ui::sound::SoundPlayer;
use crate::ui::widgets::{AnalysisPanel, GameOverCard, MoveList, PlayerBar, ReviewPanel};

const MIN_SIDEBAR_WIDTH: i32 = 300;
const MAX_SIDEBAR_WIDTH: i32 = 900;

pub struct MainWindow {
    window: adw::ApplicationWindow,
    controller: Rc<Controller>,
}

fn icon_button(icon: &str, tooltip: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.set_action_name(Some(action));
    button
}

fn labeled_button(icon: &str, label: &str) -> gtk::Button {
    let content = adw::ButtonContent::builder().icon_name(icon).label(label).build();
    let button = gtk::Button::builder().child(&content).build();
    button.add_css_class("action-button");
    button
}

fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let game = gio::Menu::new();
    game.append(Some("_New Game…"), Some("win.new-game"));
    game.append(Some("_Restart Game"), Some("win.restart"));
    game.append(Some("Load Position from _FEN…"), Some("win.load-fen"));
    menu.append_section(None, &game);
    let files = gio::Menu::new();
    files.append(Some("_Open PGN…"), Some("win.import-pgn"));
    files.append(Some("_Save PGN…"), Some("win.export-pgn"));
    files.append(Some("Copy P_GN"), Some("win.copy-pgn"));
    files.append(Some("Copy F_EN"), Some("win.copy-fen"));
    menu.append_section(None, &files);
    let review = gio::Menu::new();
    review.append(Some("Game _Review"), Some("win.review"));
    review.append(Some("Game _History"), Some("win.history"));
    review.append(Some("_Statistics & Analytics"), Some("win.statistics"));
    menu.append_section(None, &review);
    let view = gio::Menu::new();
    view.append(Some("_Flip Board"), Some("win.flip"));
    view.append(Some("_Fullscreen"), Some("win.fullscreen"));
    view.append(Some("Show _Sidebar"), Some("win.toggle-sidebar"));
    menu.append_section(None, &view);
    let app = gio::Menu::new();
    app.append(Some("_Engines…"), Some("win.engines"));
    app.append(Some("_Preferences"), Some("win.preferences"));
    app.append(Some("_Keyboard Shortcuts"), Some("win.shortcuts"));
    app.append(Some("_About KnightGambit"), Some("win.about"));
    menu.append_section(None, &app);
    menu
}

impl MainWindow {
    pub fn new(
        application: &adw::Application,
        settings: SettingsStore,
        registry: Rc<RefCell<EngineRegistry>>,
        history: HistoryStore,
    ) -> Self {
        let initial = settings.get();
        let window = adw::ApplicationWindow::builder()
            .application(application)
            .title(APP_NAME)
            .icon_name(APP_ID)
            .default_width(initial.window_width.max(720))
            .default_height(initial.window_height.max(560))
            .width_request(360)
            .height_request(480)
            .build();
        if initial.window_maximized {
            window.maximize();
        }

        let title = adw::WindowTitle::new(APP_NAME, "Think. Play. Master.");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let new_game = labeled_button("list-add-symbolic", "New Game");
        new_game.set_action_name(Some("win.new-game"));
        new_game.add_css_class("suggested-action");
        new_game.set_tooltip_text(Some("New game (Ctrl+N)"));
        header.pack_start(&new_game);
        header.pack_start(&icon_button("object-flip-vertical-symbolic", "Flip board (Ctrl+F)", "win.flip"));
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&build_menu())
            .tooltip_text("Main menu")
            .primary(true)
            .build();
        header.pack_end(&menu_button);
        header.pack_end(&icon_button("view-fullscreen-symbolic", "Fullscreen (F11)", "win.fullscreen"));
        let sidebar_toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .tooltip_text("Show sidebar (F9)")
            .build();
        header.pack_end(&sidebar_toggle);
        header.pack_end(&icon_button("kg-engine-symbolic", "Engines (Ctrl+E)", "win.engines"));
        let history_button = icon_button("kg-history-symbolic", "Game history (Ctrl+H)", "win.history");
        let statistics_button = icon_button("kg-stats-symbolic", "Statistics & analytics (Ctrl+I)", "win.statistics");
        header.pack_end(&history_button);
        header.pack_end(&statistics_button);

        let board = BoardWidget::new();
        let top_bar = PlayerBar::new();
        let bottom_bar = PlayerBar::new();
        let frame = BoardFrame::new(&top_bar.root, &board, &bottom_bar.root);
        frame.set_margin_top(12);
        frame.set_margin_bottom(12);
        frame.set_margin_start(16);
        frame.set_margin_end(16);
        let game_over = GameOverCard::new();
        let board_overlay = gtk::Overlay::new();
        board_overlay.set_child(Some(&frame));
        board_overlay.add_overlay(&game_over.root);
        let exit_fullscreen = gtk::Button::from_icon_name("view-restore-symbolic");
        exit_fullscreen.add_css_class("osd");
        exit_fullscreen.add_css_class("circular");
        exit_fullscreen.set_tooltip_text(Some("Leave fullscreen (F11)"));
        exit_fullscreen.set_halign(gtk::Align::End);
        exit_fullscreen.set_valign(gtk::Align::Start);
        exit_fullscreen.set_margin_top(12);
        exit_fullscreen.set_margin_end(12);
        exit_fullscreen.set_visible(false);
        exit_fullscreen.set_action_name(Some("win.fullscreen"));
        board_overlay.add_overlay(&exit_fullscreen);
        board_overlay.add_css_class("board-area");

        let status_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
        status_icon.add_css_class("status-icon");
        let status_title = gtk::Label::new(Some("White to move"));
        status_title.add_css_class("heading");
        status_title.set_xalign(0.0);
        let status_detail = gtk::Label::new(None);
        status_detail.add_css_class("dim-label");
        status_detail.set_xalign(0.0);
        status_detail.set_wrap(true);
        let status_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        status_text.append(&status_title);
        status_text.append(&status_detail);
        status_text.set_hexpand(true);
        let status_card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status_card.add_css_class("card");
        status_card.add_css_class("status-card");
        status_card.append(&status_icon);
        status_card.append(&status_text);

        let analysis = AnalysisPanel::new();
        let review = ReviewPanel::new();
        review.hide();
        let review_button = labeled_button("kg-review-symbolic", "Game Review");
        review_button.set_tooltip_text(Some("Analyze every move with the engine (Ctrl+Shift+R)"));
        let move_list = MoveList::new();
        let moves_frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        moves_frame.add_css_class("card");
        moves_frame.add_css_class("moves-card");
        let moves_header = gtk::Label::new(Some("Moves"));
        moves_header.add_css_class("heading");
        moves_header.set_xalign(0.0);
        moves_header.add_css_class("card-header");
        moves_frame.append(&moves_header);
        moves_frame.append(&move_list.root);

        let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        navigation.add_css_class("linked");
        navigation.set_homogeneous(true);
        let first = gtk::Button::from_icon_name("media-skip-backward-symbolic");
        first.set_tooltip_text(Some("First position (Home)"));
        let previous = gtk::Button::from_icon_name("go-previous-symbolic");
        previous.set_tooltip_text(Some("Previous move (Left)"));
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        next.set_tooltip_text(Some("Next move (Right)"));
        let last = gtk::Button::from_icon_name("media-skip-forward-symbolic");
        last.set_tooltip_text(Some("Current position (End)"));
        for button in [&first, &previous, &next, &last] {
            navigation.append(button);
        }

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_homogeneous(true);
        let takeback = labeled_button("edit-undo-symbolic", "Undo");
        takeback.set_tooltip_text(Some("Take back (Ctrl+Z)"));
        let offer_draw = labeled_button("kg-draw-symbolic", "Draw");
        offer_draw.set_tooltip_text(Some("Offer a draw"));
        let resign = labeled_button("kg-flag-symbolic", "Resign");
        resign.add_css_class("destructive-action");
        resign.set_tooltip_text(Some("Resign the game"));
        actions.append(&takeback);
        actions.append(&offer_draw);
        actions.append(&resign);
        let claim_draw = labeled_button("kg-draw-symbolic", "Claim Draw");
        claim_draw.add_css_class("suggested-action");
        claim_draw.set_visible(false);

        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 10);
        sidebar.add_css_class("game-sidebar");
        sidebar.set_margin_top(12);
        sidebar.set_margin_bottom(12);
        sidebar.set_margin_start(12);
        sidebar.set_margin_end(12);
        let scroll_content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        scroll_content.append(&status_card);
        scroll_content.append(&analysis.root);
        scroll_content.append(&review.root);
        scroll_content.append(&moves_frame);
        moves_frame.set_vexpand(true);
        let sidebar_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&scroll_content)
            .build();
        sidebar.append(&sidebar_scroll);
        sidebar.append(&navigation);
        sidebar.append(&claim_draw);
        sidebar.append(&review_button);
        sidebar.append(&actions);

        let sidebar_width = initial.sidebar_width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH) as f64;
        let handle = gtk::Box::new(gtk::Orientation::Vertical, 0);
        handle.add_css_class("sidebar-handle");
        handle.set_width_request(9);
        handle.set_cursor_from_name(Some("col-resize"));
        handle.set_tooltip_text(Some("Drag to resize the sidebar"));
        let grip = gtk::Box::new(gtk::Orientation::Vertical, 0);
        grip.add_css_class("sidebar-grip");
        grip.set_valign(gtk::Align::Center);
        grip.set_halign(gtk::Align::Center);
        grip.set_size_request(3, 48);
        grip.set_vexpand(true);
        handle.append(&grip);
        let sidebar_container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sidebar_container.append(&handle);
        sidebar.set_hexpand(true);
        sidebar_container.append(&sidebar);
        let split = adw::OverlaySplitView::builder()
            .sidebar_position(gtk::PackType::End)
            .min_sidebar_width(sidebar_width)
            .max_sidebar_width(sidebar_width)
            .sidebar_width_fraction(1.0)
            .content(&board_overlay)
            .sidebar(&sidebar_container)
            .build();
        let drag = gtk::GestureDrag::new();
        let start_width = std::rc::Rc::new(std::cell::Cell::new(sidebar_width));
        {
            let split = split.clone();
            let start_width = start_width.clone();
            drag.connect_drag_begin(move |_, _, _| {
                start_width.set(split.max_sidebar_width());
            });
        }
        {
            let split = split.clone();
            let start_width = start_width.clone();
            let window = window.clone();
            drag.connect_drag_update(move |_, offset_x, _| {
                let limit = (window.width() as f64 - 360.0).max(MIN_SIDEBAR_WIDTH as f64);
                let width = (start_width.get() - offset_x)
                    .clamp(MIN_SIDEBAR_WIDTH as f64, (MAX_SIDEBAR_WIDTH as f64).min(limit));
                split.set_min_sidebar_width(width);
                split.set_max_sidebar_width(width);
            });
        }
        {
            let split = split.clone();
            let settings = settings.clone();
            drag.connect_drag_end(move |_, _, _| {
                let width = split.max_sidebar_width().round() as i32;
                settings.update_silently(|settings| settings.sidebar_width = width);
            });
        }
        handle.add_controller(drag);
        split
            .bind_property("show-sidebar", &sidebar_toggle, "active")
            .bidirectional()
            .sync_create()
            .build();

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&split));
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&toolbar));
        window.set_content(Some(&toasts));

        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            880.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        breakpoint.add_setter(&history_button, "visible", Some(&false.to_value()));
        breakpoint.add_setter(&statistics_button, "visible", Some(&false.to_value()));
        window.add_breakpoint(breakpoint);

        let ui = Ui {
            window: window.clone(),
            toasts,
            title,
            board: board.clone(),
            top_bar,
            bottom_bar,
            move_list,
            analysis,
            status_title,
            status_detail,
            status_icon,
            resign,
            offer_draw,
            claim_draw,
            takeback,
            first,
            previous,
            next,
            last,
            game_over,
            review,
            review_button,
        };
        let sounds = SoundPlayer::new(&paths::sounds_dir());
        let controller = Controller::new(ui, settings.clone(), registry, sounds, history);

        let main = Self { window, controller };
        main.install_actions(&split, &toolbar, &exit_fullscreen);
        main.install_navigation_keys();
        main.install_close_handler();
        main.start_initial_game();
        main
    }

    fn start_initial_game(&self) {
        let controller = self.controller.clone();
        let mut config = controller.settings().with(|s| s.last_game.clone());
        let registry = controller.registry();
        let detected = registry.borrow().detect_system_engines();
        if registry.borrow().records().is_empty() && !detected.is_empty() {
            let mut registry = registry.borrow_mut();
            let threads = controller.settings().with(|s| EngineRegistry::default_threads(s.max_threads));
            for engine in detected {
                registry.upsert(crate::engine::registry::EngineRecord {
                    id: engine.definition.id.to_owned(),
                    catalog_id: Some(engine.definition.id.to_owned()),
                    display_name: engine.definition.name.to_owned(),
                    executable: engine.executable,
                    source: crate::engine::registry::EngineSource::System,
                    threads,
                    hash_mb: 128,
                    ..Default::default()
                });
            }
            let _ = registry.save();
        }
        let has_engine = !registry.borrow().available().is_empty();
        if config.mode == crate::settings::GameMode::VersusEngine && !has_engine {
            config.mode = crate::settings::GameMode::Practice;
            config.clock_enabled = false;
            controller.start_game(config);
            let window = self.window.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                let toast = adw::Toast::builder()
                    .title("No chess engine installed yet — install Stockfish to play the computer")
                    .button_label("Engines")
                    .action_name("win.engines")
                    .timeout(8)
                    .build();
                if let Some(overlay) = window.content().and_downcast::<adw::ToastOverlay>() {
                    overlay.add_toast(toast);
                }
            });
        } else {
            config.start_fen.clear();
            controller.start_game(config);
        }
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    pub fn controller(&self) -> Rc<Controller> {
        self.controller.clone()
    }

    pub fn open_file(&self, file: &gio::File) {
        let controller = self.controller.clone();
        file.load_contents_async(gio::Cancellable::NONE, move |result| match result {
            Ok((bytes, _)) => controller.load_pgn(&String::from_utf8_lossy(&bytes)),
            Err(error) => controller.toast(&format!("Could not open file: {error}")),
        });
    }

    fn add_action(&self, name: &str, handler: impl Fn(&Rc<Controller>) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let controller = self.controller.clone();
        action.connect_activate(move |_, _| handler(&controller));
        self.window.add_action(&action);
    }

    fn install_actions(&self, split: &adw::OverlaySplitView, toolbar: &adw::ToolbarView, exit_fullscreen: &gtk::Button) {
        self.add_action("new-game", dialogs::new_game::present);
        self.add_action("restart", |controller| controller.restart());
        self.add_action("flip", |controller| controller.flip_board());
        self.add_action("undo", |controller| controller.takeback());
        self.add_action("resign", |controller| controller.confirm_resign());
        self.add_action("offer-draw", |controller| controller.offer_draw());
        self.add_action("claim-draw", |controller| controller.claim_draw());
        self.add_action("review", |controller| controller.start_review());
        self.add_action("first", |controller| controller.go_to_ply(0));
        self.add_action("previous", |controller| controller.step(-1));
        self.add_action("next", |controller| controller.step(1));
        self.add_action("last", |controller| controller.go_to_live());
        self.add_action("preferences", dialogs::preferences::present);
        self.add_action("engines", dialogs::engines::present);
        self.add_action("history", dialogs::history::present);
        self.add_action("statistics", dialogs::statistics::present);
        self.add_action("shortcuts", dialogs::misc::present_shortcuts);
        self.add_action("about", dialogs::misc::present_about);
        self.add_action("load-fen", dialogs::misc::present_fen_loader);
        self.add_action("copy-fen", |controller| {
            let fen = controller.fen_text();
            controller.copy_to_clipboard(&fen, "FEN");
        });
        self.add_action("copy-pgn", |controller| {
            let pgn = controller.pgn_text();
            controller.copy_to_clipboard(&pgn, "PGN");
        });
        self.add_action("import-pgn", dialogs::misc::import_pgn);
        self.add_action("export-pgn", dialogs::misc::export_pgn);

        let toggle_sidebar = gio::SimpleAction::new("toggle-sidebar", None);
        let split_ref = split.clone();
        toggle_sidebar.connect_activate(move |_, _| {
            split_ref.set_show_sidebar(!split_ref.shows_sidebar());
        });
        self.window.add_action(&toggle_sidebar);

        let fullscreen = gio::SimpleAction::new("fullscreen", None);
        let window = self.window.clone();
        fullscreen.connect_activate(move |_, _| {
            if window.is_fullscreen() {
                window.unfullscreen();
            } else {
                window.fullscreen();
            }
        });
        self.window.add_action(&fullscreen);
        let toolbar = toolbar.clone();
        let exit_fullscreen = exit_fullscreen.clone();
        self.window.connect_fullscreened_notify(move |window| {
            let fullscreen = window.is_fullscreen();
            toolbar.set_reveal_top_bars(!fullscreen);
            exit_fullscreen.set_visible(fullscreen);
        });

        if let Some(application) = self.window.application() {
            let accels: [(&str, &[&str]); 19] = [
                ("win.review", &["<Control><Shift>r"]),
                ("win.new-game", &["<Control>n"]),
                ("win.restart", &["<Control>r"]),
                ("win.flip", &["<Control>f"]),
                ("win.undo", &["<Control>z"]),
                ("win.fullscreen", &["F11"]),
                ("win.toggle-sidebar", &["F9"]),
                ("win.preferences", &["<Control>comma"]),
                ("win.engines", &["<Control>e"]),
                ("win.history", &["<Control>h"]),
                ("win.statistics", &["<Control>i"]),
                ("win.shortcuts", &["<Control>question"]),
                ("win.load-fen", &["<Control>l"]),
                ("win.copy-fen", &["<Control><Shift>c"]),
                ("win.copy-pgn", &["<Control><Alt>c"]),
                ("win.import-pgn", &["<Control>o"]),
                ("win.export-pgn", &["<Control>s"]),
                ("window.close", &["<Control>w"]),
                ("app.quit", &["<Control>q"]),
            ];
            for (action, keys) in accels {
                application.set_accels_for_action(action, keys);
            }
            let quit = gio::SimpleAction::new("quit", None);
            let window = self.window.clone();
            quit.connect_activate(move |_, _| window.close());
            application.add_action(&quit);
        }
    }

    fn install_navigation_keys(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Bubble);
        let controller = self.controller.clone();
        let window = self.window.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if window.visible_dialog().is_some()
                || modifiers.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK)
            {
                return glib::Propagation::Proceed;
            }
            match key {
                gdk::Key::Left | gdk::Key::KP_Left => controller.step(-1),
                gdk::Key::Right | gdk::Key::KP_Right => controller.step(1),
                gdk::Key::Home | gdk::Key::KP_Home | gdk::Key::Up => controller.go_to_ply(0),
                gdk::Key::End | gdk::Key::KP_End | gdk::Key::Down => controller.go_to_live(),
                gdk::Key::Escape if window.is_fullscreen() => window.unfullscreen(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        self.window.add_controller(keys);
    }

    fn install_close_handler(&self) {
        let controller = self.controller.clone();
        self.window.connect_close_request(move |window| {
            let maximized = window.is_maximized();
            let (width, height) = window.default_size();
            controller.settings().update_silently(|settings| {
                settings.window_maximized = maximized;
                if !maximized && width > 0 && height > 0 {
                    settings.window_width = width;
                    settings.window_height = height;
                }
            });
            controller.shutdown();
            glib::Propagation::Proceed
        });
    }
}
