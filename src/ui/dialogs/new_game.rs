use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use shakmaty::{Color, Role};

use crate::chess::Game;
use crate::clock::{DelayMode, TIME_CONTROL_PRESETS, TimeControl};
use crate::engine::registry::EngineRecord;
use crate::settings::{ColorChoice, Difficulty, GameMode, NewGameSettings, StrengthSettings};
use crate::ui::art::piece_picture;
use crate::ui::controller::Controller;

fn toggle_card(icon: Option<&str>, picture: Option<gtk::Picture>, label: &str, subtitle: &str) -> gtk::ToggleButton {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    if let Some(icon) = icon {
        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(28);
        content.append(&image);
    }
    if let Some(picture) = picture {
        picture.set_halign(gtk::Align::Center);
        content.append(&picture);
    }
    let title = gtk::Label::new(Some(label));
    title.add_css_class("heading");
    content.append(&title);
    if !subtitle.is_empty() {
        let caption = gtk::Label::new(Some(subtitle));
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
        caption.set_wrap(true);
        caption.set_justify(gtk::Justification::Center);
        content.append(&caption);
    }
    let button = gtk::ToggleButton::builder().child(&content).hexpand(true).build();
    button.add_css_class("choice-card");
    button
}

fn card_row(buttons: &[&gtk::ToggleButton]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_homogeneous(true);
    for button in buttons {
        row.append(*button);
        if let Some(first) = buttons.first()
            && !std::ptr::eq(*first, *button)
        {
            button.set_group(Some(*first));
        }
    }
    row
}

struct Form {
    mode: Cell<GameMode>,
    color: Cell<ColorChoice>,
    engines: RefCell<Vec<EngineRecord>>,
    preset: Cell<Option<usize>>,
}

pub fn present(controller: &Rc<Controller>) {
    let initial = controller.config();
    let style = controller.settings().with(|s| s.piece_style);
    let scale = controller.ui().window.scale_factor();
    let engines: Vec<EngineRecord> = controller.registry().borrow().available();
    let form = Rc::new(Form {
        mode: Cell::new(initial.mode),
        color: Cell::new(initial.color),
        engines: RefCell::new(engines.clone()),
        preset: Cell::new(
            TIME_CONTROL_PRESETS
                .iter()
                .position(|preset| *preset == initial.time_control),
        ),
    });

    let dialog = adw::Dialog::builder()
        .title("New Game")
        .content_width(560)
        .content_height(760)
        .build();
    let page = adw::PreferencesPage::new();

    let mode_group = adw::PreferencesGroup::builder().title("Game Mode").build();
    let versus = toggle_card(Some("computer-symbolic"), None, "Computer", "Play an engine");
    let local = toggle_card(Some("system-users-symbolic"), None, "Two Players", "Same device");
    let practice = toggle_card(Some("applications-science-symbolic"), None, "Practice", "Move both sides");
    let mode_row = card_row(&[&versus, &local, &practice]);
    mode_group.add(&mode_row);
    page.add(&mode_group);

    let color_group = adw::PreferencesGroup::builder().title("Play As").build();
    let white = toggle_card(None, Some(piece_picture(style, Role::King, Color::White, 40, scale)), "White", "");
    let random = toggle_card(Some("media-playlist-shuffle-symbolic"), None, "Random", "");
    let black = toggle_card(None, Some(piece_picture(style, Role::King, Color::Black, 40, scale)), "Black", "");
    let color_row = card_row(&[&white, &random, &black]);
    color_group.add(&color_row);
    page.add(&color_group);

    let engine_group = adw::PreferencesGroup::builder().title("Opponent").build();
    let engine_names = gtk::StringList::new(&[]);
    for record in &engines {
        engine_names.append(&record.version_label());
    }
    let engine_row = adw::ComboRow::builder()
        .title("Engine")
        .model(&engine_names)
        .build();
    if let Some(index) = initial
        .engine_id
        .as_ref()
        .and_then(|id| engines.iter().position(|record| &record.id == id))
    {
        engine_row.set_selected(index as u32);
    }
    let manage = adw::ActionRow::builder()
        .title("Manage Engines")
        .subtitle("Install Stockfish, Berserk, Ethereal, Koivisto and Leela Chess Zero")
        .activatable(true)
        .build();
    manage.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    let difficulty_names: Vec<&str> = Difficulty::ALL.iter().map(|d| d.label()).collect();
    let difficulty_row = adw::ComboRow::builder()
        .title("Difficulty")
        .model(&gtk::StringList::new(&difficulty_names))
        .build();
    difficulty_row.set_selected(
        Difficulty::ALL
            .iter()
            .position(|d| *d == initial.strength.difficulty)
            .unwrap_or(1) as u32,
    );
    let elo_row = adw::SpinRow::with_range(800.0, 3000.0, 50.0);
    elo_row.set_title("Target Elo");
    elo_row.set_subtitle("3000 means unlimited strength");
    elo_row.set_value(initial.strength.custom_elo as f64);
    let depth_row = adw::SpinRow::with_range(0.0, 60.0, 1.0);
    depth_row.set_title("Search depth limit");
    depth_row.set_subtitle("0 means no depth limit");
    depth_row.set_value(initial.strength.custom_depth as f64);
    let think_row = adw::SpinRow::with_range(0.1, 120.0, 0.5);
    think_row.set_title("Thinking time per move");
    think_row.set_subtitle("Seconds, used when no clock is running");
    think_row.set_digits(1);
    think_row.set_value(initial.strength.think_time_ms as f64 / 1000.0);
    let no_engine = adw::ActionRow::builder()
        .title("No engine installed")
        .subtitle("Install an engine to play against the computer")
        .build();
    no_engine.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    engine_group.add(&no_engine);
    engine_group.add(&engine_row);
    engine_group.add(&difficulty_row);
    engine_group.add(&elo_row);
    engine_group.add(&depth_row);
    engine_group.add(&think_row);
    engine_group.add(&manage);
    page.add(&engine_group);

    let clock_group = adw::PreferencesGroup::builder().title("Time Control").build();
    let clock_switch = adw::SwitchRow::builder()
        .title("Use chess clock")
        .active(initial.clock_enabled)
        .build();
    clock_group.add(&clock_switch);
    let presets_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    presets_box.set_margin_top(10);
    let mut preset_buttons: Vec<gtk::ToggleButton> = Vec::new();
    let custom_toggle = gtk::ToggleButton::with_label("Custom");
    custom_toggle.add_css_class("time-preset");
    let mut categories: Vec<(&str, Vec<usize>)> = Vec::new();
    for (index, preset) in TIME_CONTROL_PRESETS.iter().enumerate() {
        let label = preset.category().label();
        match categories.iter_mut().find(|(name, _)| *name == label) {
            Some((_, items)) => items.push(index),
            None => categories.push((label, vec![index])),
        }
    }
    for (category, indices) in categories {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let caption = gtk::Label::new(Some(category));
        caption.add_css_class("caption-heading");
        caption.add_css_class("dim-label");
        caption.set_width_chars(9);
        caption.set_xalign(0.0);
        row.append(&caption);
        let flow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        flow.set_homogeneous(true);
        flow.set_hexpand(true);
        for index in indices {
            let button = gtk::ToggleButton::with_label(&TIME_CONTROL_PRESETS[index].label());
            button.add_css_class("time-preset");
            button.set_group(Some(&custom_toggle));
            flow.append(&button);
            preset_buttons.push(button.clone());
            let preset_form = form.clone();
            button.connect_toggled(move |button| {
                if button.is_active() {
                    preset_form.preset.set(Some(index));
                }
            });
            if form.preset.get() == Some(index) {
                button.set_active(true);
            }
        }
        row.append(&flow);
        presets_box.append(&row);
    }
    let custom_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let custom_caption = gtk::Label::new(Some("Other"));
    custom_caption.add_css_class("caption-heading");
    custom_caption.add_css_class("dim-label");
    custom_caption.set_width_chars(9);
    custom_caption.set_xalign(0.0);
    custom_row.append(&custom_caption);
    custom_toggle.set_hexpand(true);
    custom_row.append(&custom_toggle);
    presets_box.append(&custom_row);
    if form.preset.get().is_none() {
        custom_toggle.set_active(true);
    }
    clock_group.add(&presets_box);
    let minutes_row = adw::SpinRow::with_range(0.25, 240.0, 0.5);
    minutes_row.set_title("Minutes per side");
    minutes_row.set_digits(2);
    minutes_row.set_value(initial.time_control.initial_seconds as f64 / 60.0);
    let increment_row = adw::SpinRow::with_range(0.0, 180.0, 1.0);
    increment_row.set_title("Increment per move (seconds)");
    increment_row.set_value(initial.time_control.increment_seconds as f64);
    let delay_row = adw::SpinRow::with_range(0.0, 60.0, 1.0);
    delay_row.set_title("Delay (seconds)");
    delay_row.set_value(initial.time_control.delay_seconds as f64);
    let delay_names: Vec<&str> = DelayMode::ALL.iter().map(|mode| mode.label()).collect();
    let delay_mode_row = adw::ComboRow::builder()
        .title("Delay type")
        .model(&gtk::StringList::new(&delay_names))
        .build();
    delay_mode_row.set_selected(
        DelayMode::ALL
            .iter()
            .position(|mode| *mode == initial.time_control.delay_mode)
            .unwrap_or(0) as u32,
    );
    let custom_group = adw::PreferencesGroup::new();
    custom_group.add(&minutes_row);
    custom_group.add(&increment_row);
    custom_group.add(&delay_mode_row);
    custom_group.add(&delay_row);
    page.add(&clock_group);
    page.add(&custom_group);

    let position_group = adw::PreferencesGroup::builder()
        .title("Starting Position")
        .description("Leave empty for the standard starting position")
        .build();
    let fen_row = adw::EntryRow::builder().title("FEN").build();
    fen_row.set_text(&initial.start_fen);
    position_group.add(&fen_row);
    page.add(&position_group);

    let start_group = adw::PreferencesGroup::new();
    let start = gtk::Button::with_label("Start Game");
    start.add_css_class("suggested-action");
    start.add_css_class("pill");
    start.set_halign(gtk::Align::Center);
    start.set_margin_top(6);
    start_group.add(&start);
    page.add(&start_group);

    let header = adw::HeaderBar::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));

    let refresh: Rc<dyn Fn()> = {
        let form = form.clone();
        let color_group = color_group.clone();
        let engine_group = engine_group.clone();
        let clock_group = clock_group.clone();
        let custom_group = custom_group.clone();
        let engine_row = engine_row.clone();
        let difficulty_row = difficulty_row.clone();
        let elo_row = elo_row.clone();
        let depth_row = depth_row.clone();
        let think_row = think_row.clone();
        let no_engine = no_engine.clone();
        let clock_switch = clock_switch.clone();
        let presets_box = presets_box.clone();
        let custom_toggle = custom_toggle.clone();
        let start = start.clone();
        let delay_row = delay_row.clone();
        let delay_mode_row = delay_mode_row.clone();
        Rc::new(move || {
            let mode = form.mode.get();
            let has_engines = !form.engines.borrow().is_empty();
            color_group.set_visible(mode == GameMode::VersusEngine);
            engine_group.set_visible(mode != GameMode::TwoPlayers);
            engine_group.set_title(if mode == GameMode::Practice { "Analysis Engine" } else { "Opponent" });
            engine_group.set_description(if mode == GameMode::Practice {
                Some("Optional live evaluation while you practice")
            } else {
                None
            });
            no_engine.set_visible(!has_engines);
            engine_row.set_visible(has_engines);
            let playing = mode == GameMode::VersusEngine && has_engines;
            difficulty_row.set_visible(playing);
            let difficulty = Difficulty::ALL[difficulty_row.selected().min(4) as usize];
            elo_row.set_visible(playing && difficulty == Difficulty::Custom);
            depth_row.set_visible(playing && difficulty == Difficulty::Custom);
            think_row.set_visible(playing && matches!(difficulty, Difficulty::Custom | Difficulty::Maximum));
            clock_group.set_visible(mode != GameMode::Practice);
            presets_box.set_sensitive(clock_switch.is_active());
            custom_group.set_visible(mode != GameMode::Practice && clock_switch.is_active() && custom_toggle.is_active());
            delay_row.set_sensitive(delay_mode_row.selected() != 0);
            start.set_sensitive(mode != GameMode::VersusEngine || has_engines);
        })
    };

    for (button, mode) in [
        (&versus, GameMode::VersusEngine),
        (&local, GameMode::TwoPlayers),
        (&practice, GameMode::Practice),
    ] {
        button.set_active(form.mode.get() == mode);
        let form = form.clone();
        let refresh = refresh.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                form.mode.set(mode);
                refresh();
            }
        });
    }
    for (button, choice) in [
        (&white, ColorChoice::White),
        (&random, ColorChoice::Random),
        (&black, ColorChoice::Black),
    ] {
        button.set_active(form.color.get() == choice);
        let form = form.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                form.color.set(choice);
            }
        });
    }
    {
        let refresh = refresh.clone();
        difficulty_row.connect_selected_notify(move |_| refresh());
    }
    {
        let refresh = refresh.clone();
        clock_switch.connect_active_notify(move |_| refresh());
    }
    {
        let refresh = refresh.clone();
        let form = form.clone();
        custom_toggle.connect_toggled(move |button| {
            if button.is_active() {
                form.preset.set(None);
            }
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        delay_mode_row.connect_selected_notify(move |_| refresh());
    }
    {
        let fen_row_ref = fen_row.clone();
        fen_row.connect_changed(move |_| {
            let text = fen_row_ref.text();
            let valid = text.trim().is_empty() || Game::from_fen(&text).is_ok();
            if valid {
                fen_row_ref.remove_css_class("error");
            } else {
                fen_row_ref.add_css_class("error");
            }
        });
    }
    {
        let controller = controller.clone();
        let dialog_ref = dialog.clone();
        manage.connect_activated(move |_| {
            dialog_ref.close();
            super::engines::present(&controller);
        });
    }
    {
        let controller = controller.clone();
        let dialog_ref = dialog.clone();
        let form = form.clone();
        start.connect_clicked(move |_| {
            let fen = fen_row.text().trim().to_owned();
            if !fen.is_empty()
                && let Err(error) = Game::from_fen(&fen)
            {
                controller.toast(&error.to_string());
                return;
            }
            let difficulty = Difficulty::ALL[difficulty_row.selected().min(4) as usize];
            let time_control = match form.preset.get() {
                Some(index) => TIME_CONTROL_PRESETS[index],
                None => TimeControl {
                    initial_seconds: (minutes_row.value() * 60.0).round().max(5.0) as u64,
                    increment_seconds: increment_row.value().round() as u64,
                    delay_seconds: delay_row.value().round() as u64,
                    delay_mode: DelayMode::ALL[delay_mode_row.selected().min(2) as usize],
                },
            };
            let engine_id = form
                .engines
                .borrow()
                .get(engine_row.selected() as usize)
                .map(|record| record.id.clone());
            let config = NewGameSettings {
                mode: form.mode.get(),
                color: form.color.get(),
                engine_id,
                strength: StrengthSettings {
                    difficulty,
                    custom_elo: elo_row.value().round() as u32,
                    custom_depth: depth_row.value().round() as u32,
                    think_time_ms: (think_row.value() * 1000.0).round() as u64,
                },
                clock_enabled: clock_switch.is_active(),
                time_control,
                start_fen: fen,
            };
            dialog_ref.close();
            controller.start_game(config);
        });
    }
    refresh();
    dialog.present(Some(&controller.ui().window));
}
