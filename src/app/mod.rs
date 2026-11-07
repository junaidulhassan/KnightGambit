pub mod paths;

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::engine::registry::EngineRegistry;
use crate::settings::SettingsStore;
use crate::ui::window::MainWindow;

pub const APP_ID: &str = "io.github.knightgambit.KnightGambit";
pub const APP_NAME: &str = "KnightGambit";
pub const APP_TAGLINE: &str = "Think. Play. Master.";

pub fn prepare_display() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../ui/style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let theme = gtk::IconTheme::for_display(&display);
        if let Some(directory) = paths::development_icons_dir() {
            theme.add_search_path(directory);
        }
    }
}

pub fn run() -> glib::ExitCode {
    glib::set_application_name(APP_NAME);
    glib::set_prgname(Some(APP_ID));
    let application = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let state: Rc<RefCell<Option<MainWindow>>> = Rc::new(RefCell::new(None));

    application.connect_startup(|_| {
        prepare_display();
        gtk::Window::set_default_icon_name(APP_ID);
    });

    let activate_state = state.clone();
    application.connect_activate(move |application| {
        present_window(application, &activate_state, None);
    });

    let open_state = state.clone();
    application.connect_open(move |application, files, _| {
        present_window(application, &open_state, files.first().cloned());
    });

    application.run()
}

fn present_window(
    application: &adw::Application,
    state: &Rc<RefCell<Option<MainWindow>>>,
    file: Option<gio::File>,
) {
    if let Some(window) = state.borrow().as_ref() {
        window.present();
        if let Some(file) = file {
            window.open_file(&file);
        }
        return;
    }
    let settings = SettingsStore::open(paths::settings_file());
    let registry = Rc::new(RefCell::new(EngineRegistry::load(
        paths::engines_file(),
        paths::engines_dir(),
    )));
    let window = MainWindow::new(application, settings, registry);
    window.present();
    if let Some(file) = file {
        window.open_file(&file);
    }
    state.replace(Some(window));
}
