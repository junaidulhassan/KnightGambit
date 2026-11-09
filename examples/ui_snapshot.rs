use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use knightgambit::app::{paths, prepare_display};
use knightgambit::chess::Game;
use knightgambit::engine::registry::EngineRegistry;
use knightgambit::settings::SettingsStore;
use knightgambit::ui::window::MainWindow;

/// Fills the history with synthetic games (random legal play after a book
/// opening) so the history and statistics screens can be inspected.
fn seed(store: &knightgambit::history::HistoryStore, count: usize) {
    use knightgambit::chess::PgnHeaders;
    use knightgambit::history::{
        GameContext, GameReviewSummary, MoveReviewRow, build_record, unix_now,
    };
    use rand::seq::IndexedRandom;
    use rand::{Rng, SeedableRng};
    use shakmaty::{Color, Position};

    let mut rng = rand::rngs::StdRng::seed_from_u64(7);
    let levels = [("Beginner", Some(1350)), ("Intermediate", Some(1800)), ("Advanced", Some(2300)), ("Maximum Strength", None)];
    let classes = [
        ("Brilliant", 1), ("Great", 3), ("Best", 22), ("Excellent", 18), ("Good", 20), ("Book", 8),
        ("Forced", 3), ("Inaccuracy", 9), ("Mistake", 5), ("Miss", 2), ("Blunder", 4),
    ];
    let now = unix_now();
    for index in 0..count {
        let (_, line) = knightgambit::review::OPENING_LINES.choose(&mut rng).unwrap();
        let mut game = Game::new();
        for san in line.split_whitespace() {
            let _ = game.play_san(san);
        }
        let target = rng.random_range(30..110);
        while !game.is_over() && game.ply_count() < target {
            let moves = game.current().legal_moves();
            let Some(mv) = moves.choose(&mut rng) else { break };
            let _ = game.play(*mv);
        }
        let user = if rng.random_bool(0.5) { Color::White } else { Color::Black };
        // Make the user a little better than random so the charts have a shape.
        if !game.is_over() {
            let loser = if rng.random_bool(0.4 + index as f64 / (count as f64 * 5.0)) { !user } else { user };
            game.resign(loser);
        }
        let (level, elo) = levels[rng.random_range(0..levels.len())];
        let context = GameContext {
            uid: format!("seed-{index}"),
            user_color: user,
            opponent_name: "Stockfish 19".to_owned(),
            opponent_level: level.to_owned(),
            opponent_elo: elo,
            duration_secs: rng.random_range(120..2400),
            played_at: now - ((count - index) as i64) * 86_400 / 2 - rng.random_range(0..40_000),
        };
        let headers = PgnHeaders::new(if user == Color::White { "You" } else { "Stockfish 19" }, if user == Color::White { "Stockfish 19" } else { "You" });
        let Some(record) = build_record(&game, &headers, &context) else { continue };
        let Ok(Some(id)) = store.insert_game(&record) else { continue };
        if rng.random_bool(0.6) {
            let total: i32 = classes.iter().map(|(_, weight)| weight).sum();
            let rows: Vec<MoveReviewRow> = game
                .moves()
                .iter()
                .enumerate()
                .map(|(ply, played)| {
                    let mut pick = rng.random_range(0..total);
                    let class = classes
                        .iter()
                        .find(|(_, weight)| {
                            pick -= weight;
                            pick < 0
                        })
                        .map_or("Good", |(name, _)| name);
                    MoveReviewRow {
                        game_id: id,
                        ply: ply as i64 + 1,
                        mover: played.mover,
                        class: class.to_owned(),
                        loss: 1.0,
                        accuracy: rng.random_range(55.0..100.0),
                        san: played.san.clone(),
                        best_san: Some("Nf3".to_owned()),
                    }
                })
                .collect();
            let summary = GameReviewSummary {
                engine: "Stockfish 19".to_owned(),
                reviewed_at: now,
                movetime_ms: 500,
                white_accuracy: rng.random_range(60.0..95.0),
                black_accuracy: rng.random_range(60.0..95.0),
            };
            let _ = store.save_review(id, &summary, &rows);
        }
    }
}

fn save_window(window: &adw::ApplicationWindow, path: &str) {
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let width = window.width() as f64;
    let height = window.height() as f64;
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width, height);
    let Some(node) = snapshot.to_node() else {
        eprintln!("snapshot: empty node");
        return;
    };
    let Some(renderer) = window.native().and_then(|native| native.renderer()) else {
        eprintln!("snapshot: no renderer");
        return;
    };
    let texture = renderer.render_texture(&node, None);
    match texture.save_to_png(path) {
        Ok(()) => println!("saved {path} ({}x{})", texture.width(), texture.height()),
        Err(error) => eprintln!("snapshot failed: {error}"),
    }
}

fn run_step(window: &MainWindow, shadow: &RefCell<Game>, step: &str, store: &knightgambit::history::HistoryStore) {
    let (command, argument) = step.split_once(':').unwrap_or((step, ""));
    let controller = window.controller();
    match command {
        "shot" => {
            let target = window.window().clone();
            target.present();
            target.queue_draw();
            let path = argument.to_owned();
            if let Some(clock) = target.frame_clock() {
                let done = Rc::new(std::cell::Cell::new(false));
                let handler: Rc<RefCell<Option<glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
                let id = clock.connect_after_paint({
                    let target = target.clone();
                    let done = done.clone();
                    let handler = handler.clone();
                    let path = path.clone();
                    move |clock| {
                        if done.replace(true) {
                            return;
                        }
                        let target = target.clone();
                        let path = path.clone();
                        glib::idle_add_local_once(move || save_window(&target, &path));
                        if let Some(id) = handler.borrow_mut().take() {
                            clock.disconnect(id);
                        }
                    }
                });
                handler.replace(Some(id));
                clock.request_phase(gtk::gdk::FrameClockPhase::PAINT);
            } else {
                save_window(&target, &path);
            }
        }
        "snap" => save_window(window.window(), argument),
        "moves" => {
            for san in argument.split_whitespace() {
                if let Ok(live) = Game::from_fen(&controller.fen_text()) {
                    shadow.replace(live);
                }
                let mut game = shadow.borrow_mut();
                let played = game.play_san(san).map(|played| played.mv);
                match played {
                    Ok(mv) => {
                        let from = mv.from().unwrap();
                        let to = Game::destination_of(&mv);
                        drop(game);
                        controller.submit_move(from, to, mv.promotion());
                    }
                    Err(error) => eprintln!("illegal scripted move {san}: {error:?}"),
                }
            }
        }
        "click" => {
            let board = controller.ui().board.clone();
            let square: shakmaty::Square = argument.parse().expect("square name");
            let width = board.width() as f64;
            let height = board.height() as f64;
            let available = width.min(height);
            let frame = (available * 0.012).round();
            let side = ((available - frame * 2.0) / 8.0).floor() * 8.0;
            let cell = side / 8.0;
            let outer = side + frame * 2.0;
            let origin_x = ((width - outer) / 2.0).round() + frame;
            let origin_y = ((height - outer) / 2.0).round() + frame;
            let file = u32::from(square.file()) as f64;
            let rank = u32::from(square.rank()) as f64;
            let (column, row) = if board.is_flipped() { (7.0 - file, rank) } else { (file, 7.0 - rank) };
            let x = origin_x + (column + 0.5) * cell;
            let y = origin_y + (row + 0.5) * cell;
            let controllers = board.observe_controllers();
            for index in 0..controllers.n_items() {
                if let Some(drag) = controllers.item(index).and_downcast::<gtk::GestureDrag>() {
                    drag.emit_by_name::<()>("drag-begin", &[&x, &y]);
                }
            }
        }
        "fen" => {
            if let Err(error) = controller.load_fen(argument) {
                eprintln!("fen failed: {error}");
            }
        }
        "drag-sidebar" => {
            let offset: f64 = argument.parse().unwrap_or(-200.0);
            fn find(widget: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
                if widget.has_css_class(class) {
                    return Some(widget.clone());
                }
                let mut child = widget.first_child();
                while let Some(current) = child {
                    if let Some(found) = find(&current, class) {
                        return Some(found);
                    }
                    child = current.next_sibling();
                }
                None
            }
            let root: gtk::Widget = window.window().clone().upcast();
            if let Some(handle) = find(&root, "sidebar-handle") {
                let controllers = handle.observe_controllers();
                for index in 0..controllers.n_items() {
                    if let Some(drag) = controllers.item(index).and_downcast::<gtk::GestureDrag>() {
                        drag.emit_by_name::<()>("drag-begin", &[&0.0f64, &0.0f64]);
                        drag.emit_by_name::<()>("drag-update", &[&offset, &0.0f64]);
                        drag.emit_by_name::<()>("drag-end", &[&offset, &0.0f64]);
                    }
                }
            } else {
                eprintln!("sidebar handle not found");
            }
        }
        "action" => {
            let _ = WidgetExt::activate_action(window.window(), argument, None);
        }
        "reset" => {
            shadow.replace(Game::new());
        }
        "size" => {
            let (w, h) = argument.split_once('x').unwrap();
            window.window().set_default_size(w.parse().unwrap(), h.parse().unwrap());
        }
        "page" => {
            if let Some(dialog) = window.window().visible_dialog().and_downcast::<adw::PreferencesDialog>() {
                dialog.set_visible_page_name(argument);
            }
        }
        "close-dialogs" => {
            if let Some(dialog) = window.window().visible_dialog() {
                dialog.close();
            }
        }
        "seed" => seed(store, argument.parse().unwrap_or(40)),
        "resign" => {
            controller.confirm_resign();
            if let Some(dialog) = window.window().visible_dialog().and_downcast::<adw::AlertDialog>() {
                dialog.emit_by_name_with_details::<()>("response", glib::Quark::from_str("resign"), &[&"resign"]);
                dialog.close();
            } else {
                eprintln!("resign: no confirmation dialog");
            }
        }
        "takeback" => controller.takeback(),
        "review" => controller.start_review(),
        "stats-page" => {
            fn find_stack(widget: &gtk::Widget) -> Option<adw::ViewStack> {
                if let Some(stack) = widget.downcast_ref::<adw::ViewStack>() {
                    return Some(stack.clone());
                }
                let mut child = widget.first_child();
                while let Some(current) = child {
                    if let Some(found) = find_stack(&current) {
                        return Some(found);
                    }
                    child = current.next_sibling();
                }
                None
            }
            if let Some(dialog) = window.window().visible_dialog()
                && let Some(stack) = find_stack(dialog.upcast_ref())
            {
                stack.set_visible_child_name(argument);
            }
        }
        "quit" => {
            if let Some(application) = window.window().application() {
                application.quit();
            }
        }
        _ => eprintln!("unknown step {step}"),
    }
}

fn main() -> glib::ExitCode {
    let script = std::env::var("KG_SCRIPT").unwrap_or_else(|_| "shot:/tmp/kg.png;quit".to_owned());
    let application = adw::Application::builder()
        .application_id("io.github.knightgambit.KnightGambit.Snapshot")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_startup(|_| prepare_display());
    application.connect_activate(move |application| {
        let settings = SettingsStore::open(paths::settings_file());
        let registry = Rc::new(RefCell::new(EngineRegistry::load(paths::engines_file(), paths::engines_dir())));
        if let Ok(path) = std::env::var("KG_MOCK_ENGINE") {
            let mut registry = registry.borrow_mut();
            registry.upsert(knightgambit::engine::registry::EngineRecord {
                id: "stockfish".to_owned(),
                catalog_id: Some("stockfish".to_owned()),
                display_name: "Stockfish".to_owned(),
                executable: path.into(),
                source: knightgambit::engine::registry::EngineSource::Custom,
                ..Default::default()
            });
        }
        let store = match std::env::var("KG_HISTORY_DB") {
            Ok(path) => knightgambit::history::HistoryStore::open(std::path::Path::new(&path)).expect("history db").0,
            Err(_) => knightgambit::history::HistoryStore::in_memory().expect("in-memory history"),
        };
        let window = Rc::new(MainWindow::new(application, settings, registry, store.clone()));
        window.present();
        let steps: Vec<String> = script.split(';').map(str::to_owned).collect();
        let shadow = Rc::new(RefCell::new(Game::new()));
        let mut delay = 1500u64;
        for step in steps {
            let window = window.clone();
            let shadow = shadow.clone();
            let store = store.clone();
            let wait = step.strip_prefix("wait:").and_then(|ms| ms.parse::<u64>().ok());
            if let Some(ms) = wait {
                delay += ms;
                continue;
            }
            glib::timeout_add_local_once(Duration::from_millis(delay), move || run_step(&window, &shadow, &step, &store));
            delay += 700;
        }
    });
    application.run_with_args::<&str>(&[])
}
