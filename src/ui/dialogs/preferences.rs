use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};
use shakmaty::Color;

use crate::assets::PieceStyle;
use crate::chess::NotationStyle;
use crate::settings::{
    AnimationSpeed, BOARD_PRESETS, CoordinateMode, Difficulty, Settings, SettingsStore, ThemePreference,
    parse_hex_color,
};
use crate::ui::art::{ROLES, piece_picture};
use crate::ui::controller::Controller;

fn combo(title: &str, subtitle: Option<&str>, items: &[&str], selected: usize) -> adw::ComboRow {
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(items))
        .build();
    if let Some(subtitle) = subtitle {
        row.set_subtitle(subtitle);
    }
    row.set_selected(selected as u32);
    row
}

fn switch(title: &str, subtitle: Option<&str>, active: bool) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder().title(title).active(active).build();
    if let Some(subtitle) = subtitle {
        row.set_subtitle(subtitle);
    }
    row
}

fn to_gdk(hex: &str) -> gdk::RGBA {
    let color = parse_hex_color(hex).unwrap_or(crate::settings::Rgba::rgb(128, 128, 128));
    gdk::RGBA::new(color.red, color.green, color.blue, 1.0)
}

fn to_hex(color: &gdk::RGBA) -> String {
    crate::settings::Rgba::new(color.red(), color.green(), color.blue(), 1.0).to_hex()
}

fn swatch(light: &str, dark: &str) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(56);
    area.set_content_height(56);
    let light = to_gdk(light);
    let dark = to_gdk(dark);
    area.set_draw_func(move |_, cr, width, height| {
        let cell_w = width as f64 / 4.0;
        let cell_h = height as f64 / 4.0;
        for row in 0..4 {
            for column in 0..4 {
                let color = if (row + column) % 2 == 0 { &light } else { &dark };
                cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 1.0);
                cr.rectangle(column as f64 * cell_w, row as f64 * cell_h, cell_w + 0.5, cell_h + 0.5);
                let _ = cr.fill();
            }
        }
    });
    area
}

fn piece_preview(style: PieceStyle, scale: i32) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    container.add_css_class("piece-preview");
    for color in [Color::White, Color::Black] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.set_halign(gtk::Align::Center);
        for role in ROLES {
            row.append(&piece_picture(style, role, color, 44, scale));
        }
        container.append(&row);
    }
    container
}

fn bind_switch(row: &adw::SwitchRow, store: &SettingsStore, apply: fn(&mut Settings, bool)) {
    let store = store.clone();
    row.connect_active_notify(move |row| {
        let active = row.is_active();
        store.update(|settings| apply(settings, active));
    });
}

pub fn present(controller: &Rc<Controller>) {
    let store = controller.settings().clone();
    let settings = store.get();
    let scale = controller.ui().window.scale_factor();
    let dialog = adw::PreferencesDialog::new();
    dialog.set_search_enabled(true);

    let appearance = adw::PreferencesPage::builder()
        .title("Appearance")
        .icon_name("preferences-desktop-appearance-symbolic")
        .build();
    let theme_group = adw::PreferencesGroup::builder().title("Theme").build();
    let theme_row = combo(
        "Color scheme",
        Some("Follow GNOME or force a light or dark interface"),
        &["Follow System", "Light", "Dark"],
        match settings.theme {
            ThemePreference::System => 0,
            ThemePreference::Light => 1,
            ThemePreference::Dark => 2,
        },
    );
    {
        let store = store.clone();
        theme_row.connect_selected_notify(move |row| {
            let theme = match row.selected() {
                1 => ThemePreference::Light,
                2 => ThemePreference::Dark,
                _ => ThemePreference::System,
            };
            store.update(|settings| settings.theme = theme);
        });
    }
    theme_group.add(&theme_row);
    appearance.add(&theme_group);

    let pieces_group = adw::PreferencesGroup::builder().title("Pieces").build();
    let style_names: Vec<&str> = PieceStyle::ALL.iter().map(|style| style.label()).collect();
    let style_row = combo(
        "Piece set",
        Some("Staunton-inspired vector pieces, sharp at every size"),
        &style_names,
        PieceStyle::ALL.iter().position(|s| *s == settings.piece_style).unwrap_or(0),
    );
    pieces_group.add(&style_row);
    let folder_row = adw::ActionRow::builder()
        .title("Custom piece folder")
        .subtitle(
            settings
                .custom_piece_dir
                .clone()
                .unwrap_or_else(|| "Folder of SVG files named wK.svg, bQ.svg, wN.svg … (12 files)".to_owned()),
        )
        .build();
    let folder_button = gtk::Button::with_label("Choose…");
    folder_button.set_valign(gtk::Align::Center);
    folder_row.add_suffix(&folder_button);
    folder_row.set_visible(settings.piece_style == PieceStyle::Custom);
    pieces_group.add(&folder_row);
    let preview_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview_holder.set_margin_top(12);
    preview_holder.append(&piece_preview(settings.piece_style, scale));
    pieces_group.add(&preview_holder);
    {
        let store = store.clone();
        let preview_holder = preview_holder.clone();
        let folder_row = folder_row.clone();
        style_row.connect_selected_notify(move |row| {
            let style = PieceStyle::ALL[(row.selected() as usize).min(PieceStyle::ALL.len() - 1)];
            store.update(|settings| settings.piece_style = style);
            folder_row.set_visible(style == PieceStyle::Custom);
            while let Some(child) = preview_holder.first_child() {
                preview_holder.remove(&child);
            }
            preview_holder.append(&piece_preview(style, scale));
        });
    }
    {
        let store = store.clone();
        let preview_holder = preview_holder.clone();
        let window = controller.ui().window.clone();
        let folder_row = folder_row.clone();
        folder_button.connect_clicked(move |_| {
            let chooser = gtk::FileDialog::builder().title("Choose Piece Folder").modal(true).build();
            let store = store.clone();
            let preview_holder = preview_holder.clone();
            let folder_row = folder_row.clone();
            chooser.select_folder(Some(&window), gtk::gio::Cancellable::NONE, move |result| {
                let Ok(folder) = result else { return };
                let Some(path) = folder.path() else { return };
                let found = crate::assets::load_custom_pieces(Some(&path));
                let text = path.display().to_string();
                folder_row.set_subtitle(&format!("{text} — {found} of 12 pieces found"));
                store.update(|settings| settings.custom_piece_dir = Some(text.clone()));
                while let Some(child) = preview_holder.first_child() {
                    preview_holder.remove(&child);
                }
                preview_holder.append(&piece_preview(PieceStyle::Custom, scale));
            });
        });
    }
    appearance.add(&pieces_group);
    dialog.add(&appearance);

    let board_page = adw::PreferencesPage::builder()
        .title("Board")
        .icon_name("view-grid-symbolic")
        .build();
    let presets_group = adw::PreferencesGroup::builder()
        .title("Board Colors")
        .description("Choose a preset or pick your own square colors")
        .build();
    let presets = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(5)
        .min_children_per_line(2)
        .row_spacing(8)
        .column_spacing(8)
        .homogeneous(true)
        .build();
    let light_button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::builder().with_alpha(false).build()));
    light_button.set_rgba(&to_gdk(&settings.light_square));
    light_button.set_valign(gtk::Align::Center);
    let dark_button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::builder().with_alpha(false).build()));
    dark_button.set_rgba(&to_gdk(&settings.dark_square));
    dark_button.set_valign(gtk::Align::Center);
    let mut preset_toggles: Vec<gtk::ToggleButton> = Vec::new();
    for preset in BOARD_PRESETS {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        let area = swatch(preset.light, preset.dark);
        area.set_halign(gtk::Align::Center);
        area.add_css_class("board-swatch");
        let label = gtk::Label::new(Some(preset.name));
        label.add_css_class("caption");
        label.set_wrap(true);
        label.set_justify(gtk::Justification::Center);
        label.set_max_width_chars(12);
        content.append(&area);
        content.append(&label);
        let toggle = gtk::ToggleButton::builder().child(&content).build();
        toggle.add_css_class("choice-card");
        if let Some(first) = preset_toggles.first() {
            toggle.set_group(Some(first));
        }
        toggle.set_active(settings.board_preset == preset.id);
        let store = store.clone();
        let light_button = light_button.clone();
        let dark_button = dark_button.clone();
        toggle.connect_toggled(move |toggle| {
            if !toggle.is_active() {
                return;
            }
            store.update(|settings| {
                settings.board_preset = preset.id.to_owned();
                settings.light_square = preset.light.to_owned();
                settings.dark_square = preset.dark.to_owned();
            });
            light_button.set_rgba(&to_gdk(preset.light));
            dark_button.set_rgba(&to_gdk(preset.dark));
        });
        presets.insert(&toggle, -1);
        preset_toggles.push(toggle);
    }
    presets_group.add(&presets);
    let custom_group = adw::PreferencesGroup::new();
    let light_row = adw::ActionRow::builder().title("Light squares").build();
    light_row.add_suffix(&light_button);
    light_row.set_activatable_widget(Some(&light_button));
    let dark_row = adw::ActionRow::builder().title("Dark squares").build();
    dark_row.add_suffix(&dark_button);
    dark_row.set_activatable_widget(Some(&dark_button));
    custom_group.add(&light_row);
    custom_group.add(&dark_row);
    for (button, is_light) in [(&light_button, true), (&dark_button, false)] {
        let store = store.clone();
        button.connect_rgba_notify(move |button| {
            let hex = to_hex(&button.rgba());
            store.update(|settings| {
                let current = if is_light { &settings.light_square } else { &settings.dark_square };
                if current.eq_ignore_ascii_case(&hex) {
                    return;
                }
                if is_light {
                    settings.light_square = hex.clone();
                } else {
                    settings.dark_square = hex.clone();
                }
                let matches_preset = BOARD_PRESETS.iter().any(|preset| {
                    preset.light.eq_ignore_ascii_case(&settings.light_square)
                        && preset.dark.eq_ignore_ascii_case(&settings.dark_square)
                });
                if !matches_preset {
                    settings.board_preset = "custom".to_owned();
                }
            });
        });
    }
    let texture_row = switch("Wood grain texture", Some("Subtle natural surface texture on the squares"), settings.board_texture);
    bind_switch(&texture_row, &store, |s, v| s.board_texture = v);
    custom_group.add(&texture_row);
    board_page.add(&presets_group);
    board_page.add(&custom_group);

    let display_group = adw::PreferencesGroup::builder().title("Board Display").build();
    let coordinate_names: Vec<&str> = CoordinateMode::ALL.iter().map(|mode| mode.label()).collect();
    let coordinates_row = combo(
        "Coordinates",
        None,
        &coordinate_names,
        CoordinateMode::ALL.iter().position(|m| *m == settings.coordinates).unwrap_or(0),
    );
    {
        let store = store.clone();
        coordinates_row.connect_selected_notify(move |row| {
            let mode = CoordinateMode::ALL[(row.selected() as usize).min(2)];
            store.update(|settings| settings.coordinates = mode);
        });
    }
    let legal_row = switch("Show legal moves", Some("Dots and rings on reachable squares"), settings.show_legal_moves);
    bind_switch(&legal_row, &store, |s, v| s.show_legal_moves = v);
    let last_row = switch("Highlight last move", None, settings.highlight_last_move);
    bind_switch(&last_row, &store, |s, v| s.highlight_last_move = v);
    display_group.add(&coordinates_row);
    display_group.add(&legal_row);
    display_group.add(&last_row);
    board_page.add(&display_group);
    dialog.add(&board_page);

    let gameplay = adw::PreferencesPage::builder()
        .title("Gameplay")
        .icon_name("applications-games-symbolic")
        .build();
    let motion_group = adw::PreferencesGroup::builder().title("Moves").build();
    let speed_names: Vec<&str> = AnimationSpeed::ALL.iter().map(|s| s.label()).collect();
    let speed_row = combo(
        "Animation speed",
        Some("Turn animations off or make them quicker"),
        &speed_names,
        AnimationSpeed::ALL.iter().position(|s| *s == settings.animation_speed).unwrap_or(2),
    );
    {
        let store = store.clone();
        speed_row.connect_selected_notify(move |row| {
            let speed = AnimationSpeed::ALL[(row.selected() as usize).min(3)];
            store.update(|settings| settings.animation_speed = speed);
        });
    }
    let queen_row = switch("Always promote to queen", Some("Skip the promotion picker"), settings.auto_queen);
    bind_switch(&queen_row, &store, |s, v| s.auto_queen = v);
    let premove_row = switch(
        "Premoves",
        Some("Queue your next move while the engine is thinking"),
        settings.premoves,
    );
    bind_switch(&premove_row, &store, |s, v| s.premoves = v);
    let notation_names: Vec<&str> = NotationStyle::ALL.iter().map(|n| n.label()).collect();
    let notation_row = combo(
        "Move notation",
        None,
        &notation_names,
        NotationStyle::ALL.iter().position(|n| *n == settings.notation).unwrap_or(0),
    );
    {
        let store = store.clone();
        notation_row.connect_selected_notify(move |row| {
            let notation = NotationStyle::ALL[(row.selected() as usize).min(2)];
            store.update(|settings| settings.notation = notation);
        });
    }
    motion_group.add(&speed_row);
    motion_group.add(&queen_row);
    motion_group.add(&premove_row);
    motion_group.add(&notation_row);
    gameplay.add(&motion_group);

    let sound_group = adw::PreferencesGroup::builder().title("Sound").build();
    let sound_row = switch("Sound effects", Some("Wooden move, capture, check and game sounds"), settings.sound_enabled);
    bind_switch(&sound_row, &store, |s, v| s.sound_enabled = v);
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
    volume.set_value(settings.sound_volume);
    volume.set_hexpand(true);
    volume.set_width_request(180);
    volume.set_valign(gtk::Align::Center);
    let volume_row = adw::ActionRow::builder().title("Volume").build();
    volume_row.add_suffix(&volume);
    {
        let store = store.clone();
        volume.connect_value_changed(move |scale| {
            let value = (scale.value() * 100.0).round() / 100.0;
            store.update(|settings| settings.sound_volume = value);
        });
    }
    sound_row
        .bind_property("active", &volume_row, "sensitive")
        .sync_create()
        .build();
    sound_group.add(&sound_row);
    sound_group.add(&volume_row);
    gameplay.add(&sound_group);

    let clock_group = adw::PreferencesGroup::builder().title("Clock").build();
    let tenths_row = switch("Show tenths of a second", Some("When less than 20 seconds remain"), settings.clock_tenths);
    bind_switch(&tenths_row, &store, |s, v| s.clock_tenths = v);
    let warning_row = adw::SpinRow::with_range(0.0, 120.0, 5.0);
    warning_row.set_title("Low time warning");
    warning_row.set_subtitle("Seconds remaining when the clock turns red (0 disables)");
    warning_row.set_value(settings.low_time_warning_seconds as f64);
    {
        let store = store.clone();
        warning_row.connect_value_notify(move |row| {
            let value = row.value().round() as u64;
            store.update(|settings| settings.low_time_warning_seconds = value);
        });
    }
    clock_group.add(&tenths_row);
    clock_group.add(&warning_row);
    gameplay.add(&clock_group);
    dialog.add(&gameplay);

    let engines_page = adw::PreferencesPage::builder()
        .title("Engines")
        .icon_name("kg-engine-symbolic")
        .build();
    let defaults_group = adw::PreferencesGroup::builder().title("Defaults").build();
    let available = controller.registry().borrow().available();
    let names: Vec<String> = available.iter().map(|record| record.version_label()).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let default_row = combo(
        "Default engine",
        None,
        &name_refs,
        settings
            .default_engine
            .as_ref()
            .and_then(|id| available.iter().position(|record| &record.id == id))
            .unwrap_or(0),
    );
    default_row.set_sensitive(!available.is_empty());
    {
        let store = store.clone();
        let ids: Vec<String> = available.iter().map(|record| record.id.clone()).collect();
        default_row.connect_selected_notify(move |row| {
            if let Some(id) = ids.get(row.selected() as usize).cloned() {
                store.update(|settings| {
                    settings.default_engine = Some(id.clone());
                    settings.last_game.engine_id = Some(id);
                });
            }
        });
    }
    let difficulty_names: Vec<&str> = Difficulty::ALL.iter().map(|d| d.label()).collect();
    let difficulty_row = combo(
        "Default difficulty",
        None,
        &difficulty_names,
        Difficulty::ALL
            .iter()
            .position(|d| *d == settings.default_strength.difficulty)
            .unwrap_or(1),
    );
    {
        let store = store.clone();
        difficulty_row.connect_selected_notify(move |row| {
            let difficulty = Difficulty::ALL[(row.selected() as usize).min(4)];
            store.update(|settings| {
                settings.default_strength.difficulty = difficulty;
                settings.last_game.strength.difficulty = difficulty;
            });
        });
    }
    let analysis_row = switch(
        "Show engine analysis",
        Some("Evaluation and best line during practice and game review"),
        settings.show_analysis,
    );
    bind_switch(&analysis_row, &store, |s, v| s.show_analysis = v);
    defaults_group.add(&default_row);
    defaults_group.add(&difficulty_row);
    defaults_group.add(&analysis_row);
    let eval_bar_row = switch(
        "Show evaluation bar",
        Some("A bar beside the board that follows the engine's evaluation of every position you set up"),
        settings.show_eval_bar,
    );
    bind_switch(&eval_bar_row, &store, |s, v| s.show_eval_bar = v);
    defaults_group.add(&eval_bar_row);
    let feedback_row = switch(
        "Judge my moves as I play (practice mode)",
        Some("Stockfish rates each move as brilliant, best, inaccuracy, mistake, blunder and so on right after you play it"),
        settings.live_feedback,
    );
    bind_switch(&feedback_row, &store, |s, v| s.live_feedback = v);
    defaults_group.add(&feedback_row);
    let depth_names: Vec<&str> = crate::settings::ReviewDepth::ALL.iter().map(|d| d.label()).collect();
    let depth_row = combo(
        "Game review depth",
        Some("Engine time spent on each position during Game Review"),
        &depth_names,
        crate::settings::ReviewDepth::ALL
            .iter()
            .position(|d| *d == settings.review_depth)
            .unwrap_or(1),
    );
    {
        let store = store.clone();
        depth_row.connect_selected_notify(move |row| {
            let depth = crate::settings::ReviewDepth::ALL[(row.selected() as usize).min(2)];
            store.update(|settings| settings.review_depth = depth);
        });
    }
    defaults_group.add(&depth_row);
    engines_page.add(&defaults_group);

    let resources_group = adw::PreferencesGroup::builder()
        .title("Resource Limits")
        .description("Upper bounds applied to every engine, whatever its own configuration says")
        .build();
    let total_threads = Settings::total_threads();
    let threads_row = adw::SpinRow::with_range(1.0, total_threads as f64, 1.0);
    threads_row.set_title("CPU thread limit");
    threads_row.set_subtitle(&format!("This computer has {total_threads} logical processors"));
    threads_row.set_value(settings.max_threads as f64);
    {
        let store = store.clone();
        threads_row.connect_value_notify(move |row| {
            let value = row.value().round() as u32;
            store.update(|settings| settings.max_threads = value.max(1));
        });
    }
    let total_memory = Settings::total_memory_mb();
    let memory_cap = (total_memory / 2).clamp(64, 32_768);
    let memory_row = adw::SpinRow::with_range(16.0, memory_cap as f64, 16.0);
    memory_row.set_title("Engine memory limit (MB)");
    memory_row.set_subtitle(&format!("Hash table size cap — {} MB of RAM installed", total_memory));
    memory_row.set_value(settings.max_hash_mb.min(memory_cap) as f64);
    {
        let store = store.clone();
        memory_row.connect_value_notify(move |row| {
            let value = row.value().round() as u32;
            store.update(|settings| settings.max_hash_mb = value.max(16));
        });
    }
    resources_group.add(&threads_row);
    resources_group.add(&memory_row);
    engines_page.add(&resources_group);
    let manage_group = adw::PreferencesGroup::new();
    let manage_row = adw::ActionRow::builder()
        .title("Manage Engines")
        .subtitle("Install, configure and remove chess engines")
        .activatable(true)
        .build();
    manage_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    {
        let controller = controller.clone();
        let dialog_ref = dialog.clone();
        manage_row.connect_activated(move |_| {
            dialog_ref.close();
            let controller = controller.clone();
            glib::idle_add_local_once(move || super::engines::present(&controller));
        });
    }
    manage_group.add(&manage_row);
    engines_page.add(&manage_group);
    dialog.add(&engines_page);

    let about_page = adw::PreferencesPage::builder()
        .name("about")
        .title("About")
        .icon_name("help-about-symbolic")
        .build();
    let credits = adw::PreferencesGroup::builder().title("Credits").build();
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 6);
    identity.set_margin_bottom(12);
    let logo = gtk::Image::from_icon_name(crate::app::APP_ID);
    logo.set_pixel_size(96);
    let name = gtk::Label::new(Some(crate::app::APP_NAME));
    name.add_css_class("title-1");
    let tagline = gtk::Label::new(Some(crate::app::APP_TAGLINE));
    tagline.add_css_class("dim-label");
    let version = gtk::Label::new(Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    version.add_css_class("caption");
    version.add_css_class("dim-label");
    identity.append(&logo);
    identity.append(&name);
    identity.append(&tagline);
    identity.append(&version);
    let identity_group = adw::PreferencesGroup::new();
    identity_group.add(&identity);
    about_page.add(&identity_group);
    let developer = adw::ActionRow::builder()
        .title("Developer")
        .subtitle("Junaid Ul Hassan")
        .build();
    developer.add_prefix(&gtk::Image::from_icon_name("avatar-default-symbolic"));
    credits.add(&developer);
    let license = adw::ActionRow::builder()
        .title("License")
        .subtitle("GNU General Public License v3.0 or later")
        .build();
    credits.add(&license);
    about_page.add(&credits);
    dialog.add(&about_page);

    dialog.present(Some(&controller.ui().window));
}
