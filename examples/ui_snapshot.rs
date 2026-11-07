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

fn run_step(window: &MainWindow, shadow: &RefCell<Game>, step: &str) {
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
        let window = Rc::new(MainWindow::new(application, settings, registry));
        window.present();
        let steps: Vec<String> = script.split(';').map(str::to_owned).collect();
        let shadow = Rc::new(RefCell::new(Game::new()));
        let mut delay = 1500u64;
        for step in steps {
            let window = window.clone();
            let shadow = shadow.clone();
            let wait = step.strip_prefix("wait:").and_then(|ms| ms.parse::<u64>().ok());
            if let Some(ms) = wait {
                delay += ms;
                continue;
            }
            glib::timeout_add_local_once(Duration::from_millis(delay), move || run_step(&window, &shadow, &step));
            delay += 700;
        }
    });
    application.run_with_args::<&str>(&[])
}
