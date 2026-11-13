use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::app::{APP_ID, APP_NAME, APP_TAGLINE};
use crate::ui::controller::Controller;

const SHORTCUTS: [(&str, &[(&str, &str)]); 4] = [
    (
        "Game",
        &[
            ("New game", "<Control>n"),
            ("Restart game", "<Control>r"),
            ("Take back", "<Control>z"),
            ("Load position from FEN", "<Control>l"),
            ("Game review", "<Control><Shift>r"),
        ],
    ),
    (
        "Navigation",
        &[
            ("Previous move", "Left"),
            ("Next move", "Right"),
            ("First position", "Home"),
            ("Current position", "End"),
            ("Cancel selection", "Escape"),
        ],
    ),
    (
        "Files",
        &[
            ("Open PGN", "<Control>o"),
            ("Save PGN", "<Control>s"),
            ("Copy FEN", "<Control><Shift>c"),
            ("Copy PGN", "<Control><Alt>c"),
        ],
    ),
    (
        "Window",
        &[
            ("Flip board", "<Control>f"),
            ("Fullscreen", "F11"),
            ("Toggle sidebar", "F9"),
            ("Engines", "<Control>e"),
            ("Game history", "<Control>h"),
            ("Statistics & analytics", "<Control>i"),
            ("Preferences", "<Control>comma"),
            ("Keyboard shortcuts", "<Control>question"),
            ("Quit", "<Control>q"),
        ],
    ),
];

pub fn present_shortcuts(controller: &Rc<Controller>) {
    let dialog = adw::Dialog::builder()
        .title("Keyboard Shortcuts")
        .content_width(460)
        .content_height(640)
        .build();
    let page = adw::PreferencesPage::new();
    for (title, entries) in SHORTCUTS {
        let group = adw::PreferencesGroup::builder().title(title).build();
        for (label, accelerator) in entries {
            let row = adw::ActionRow::builder().title(*label).build();
            let shortcut = gtk::ShortcutLabel::new(accelerator);
            shortcut.set_valign(gtk::Align::Center);
            row.add_suffix(&shortcut);
            group.add(&row);
        }
        page.add(&group);
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(&controller.ui().window));
}

pub fn present_about(controller: &Rc<Controller>) {
    let about = adw::AboutDialog::builder()
        .application_name(APP_NAME)
        .application_icon(APP_ID)
        .developer_name("Junaid Ul Hassan")
        .developers(["Junaid Ul Hassan"])
        .designers(["Junaid Ul Hassan"])
        .copyright("© 2026 Junaid Ul Hassan")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(format!(
            "{APP_TAGLINE}\n\nA premium offline chess application for GNOME with original Staunton-inspired pieces, complete chess rules and genuine open-source UCI engines."
        ))
        .license_type(gtk::License::Gpl30)
        .website("https://github.com/knightgambit/KnightGambit")
        .issue_url("https://github.com/knightgambit/KnightGambit/issues")
        .build();
    about.add_credit_section(
        Some("Chess Engines"),
        &[
            "Stockfish — The Stockfish developers (GPL-3.0) https://stockfishchess.org",
            "Berserk — Jay Honnold (GPL-3.0) https://github.com/jhonnold/berserk",
            "Ethereal — Andrew Grant (GPL-3.0) https://github.com/AndyGrant/Ethereal",
            "Koivisto — Kim Kåhre, Finn Eggers (GPL-3.0) https://github.com/Luecx/Koivisto",
            "Leela Chess Zero — The LCZero Authors (GPL-3.0) https://lczero.org",
        ],
    );
    about.add_legal_section(
        "shakmaty and pgn-reader",
        Some("© Niklas Fiekas"),
        gtk::License::Gpl30,
        None,
    );
    about.add_legal_section(
        "resvg",
        Some("© The resvg Authors"),
        gtk::License::Apache20,
        None,
    );
    about.add_legal_section(
        "gtk4-rs and libadwaita-rs",
        Some("© The gtk-rs Project Developers"),
        gtk::License::MitX11,
        None,
    );
    about.add_legal_section(
        "Classic Staunton chess pieces",
        Some("© Colin M.L. Burnett"),
        gtk::License::Gpl20,
        None,
    );
    about.add_legal_section(
        "Engine logos",
        None,
        gtk::License::Custom,
        Some("The Ethereal and Koivisto logos are reproduced from their official GPL-3.0 source repositories to identify those engines. Stockfish, Berserk and Leela Chess Zero are shown with neutral monograms because no logo is distributed in their source releases. All engine names are the property of their respective authors."),
    );
    about.present(Some(&controller.ui().window));
}

pub fn present_fen_loader(controller: &Rc<Controller>) {
    let dialog = adw::AlertDialog::new(
        Some("Load Position"),
        Some("Paste a position in Forsyth–Edwards Notation. The game will start from it with your current game settings."),
    );
    let entry = gtk::Entry::builder()
        .placeholder_text("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
        .activates_default(true)
        .build();
    let error = gtk::Label::new(None);
    error.add_css_class("error");
    error.add_css_class("caption");
    error.set_wrap(true);
    error.set_visible(false);
    let extra = gtk::Box::new(gtk::Orientation::Vertical, 6);
    extra.append(&entry);
    extra.append(&error);
    dialog.set_extra_child(Some(&extra));
    dialog.add_responses(&[("cancel", "Cancel"), ("load", "Load")]);
    dialog.set_response_appearance("load", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("load"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("load", false);
    {
        let dialog = dialog.clone();
        let error = error.clone();
        entry.connect_changed(move |entry| {
            let text = entry.text();
            match crate::chess::Game::from_fen(&text) {
                Ok(_) => {
                    error.set_visible(false);
                    dialog.set_response_enabled("load", true);
                }
                Err(problem) => {
                    error.set_label(&problem.to_string());
                    error.set_visible(!text.trim().is_empty());
                    dialog.set_response_enabled("load", false);
                }
            }
        });
    }
    let window = controller.ui().window.clone();
    let controller = controller.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "load"
            && let Err(problem) = controller.load_fen(&entry.text())
        {
            controller.toast(&problem);
        }
    });
    dialog.present(Some(&window));
}

fn pgn_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("PGN games"));
    filter.add_pattern("*.pgn");
    filter.add_pattern("*.PGN");
    filter.add_mime_type("application/x-chess-pgn");
    let all = gtk::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters.append(&all);
    filters
}

pub fn import_pgn(controller: &Rc<Controller>) {
    let chooser = gtk::FileDialog::builder()
        .title("Open PGN")
        .filters(&pgn_filters())
        .modal(true)
        .build();
    let controller = controller.clone();
    let window = controller.ui().window.clone();
    chooser.open(Some(&window), gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else { return };
        let controller = controller.clone();
        file.load_contents_async(gio::Cancellable::NONE, move |result| match result {
            Ok((bytes, _)) => controller.load_pgn(&String::from_utf8_lossy(&bytes)),
            Err(error) => controller.toast(&format!("Could not read the file: {error}")),
        });
    });
}

pub fn export_pgn(controller: &Rc<Controller>) {
    let name = glib::DateTime::now_local()
        .ok()
        .and_then(|now| now.format("knightgambit-%Y%m%d-%H%M.pgn").ok())
        .map(|text| text.to_string())
        .unwrap_or_else(|| "knightgambit.pgn".to_owned());
    let chooser = gtk::FileDialog::builder()
        .title("Save PGN")
        .initial_name(name.as_str())
        .filters(&pgn_filters())
        .modal(true)
        .build();
    let controller = controller.clone();
    let window = controller.ui().window.clone();
    chooser.save(Some(&window), gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else { return };
        let text = controller.pgn_text();
        let controller = controller.clone();
        file.replace_contents_async(
            text.into_bytes(),
            None,
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
            gio::Cancellable::NONE,
            move |result| match result {
                Ok(_) => controller.toast("Game saved"),
                Err((_, error)) => controller.toast(&format!("Could not save: {error}")),
            },
        );
    });
}
