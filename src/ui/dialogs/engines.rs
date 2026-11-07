use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::app::paths;
use crate::engine::catalog::{CATALOG, EngineDefinition, InstallRecipe, LC0_NETWORKS_PAGE};
use crate::engine::installer::{
    InstallHandle, InstallProgress, apt_command, download_network, format_bytes, install_engine,
    missing_prerequisites, select_variant,
};
use crate::engine::probe::probe_in_background;
use crate::engine::registry::{EngineRecord, EngineRegistry, EngineSource};
use crate::engine::uci::{UciOption, UciOptionKind};
use crate::ui::controller::Controller;
use crate::ui::widgets::engine_logo_texture;

struct Manager {
    controller: Rc<Controller>,
    dialog: adw::Dialog,
    navigation: adw::NavigationView,
    content: gtk::Box,
    installs: RefCell<Vec<(String, Rc<InstallHandle>)>>,
}

fn avatar_for(definition: Option<&EngineDefinition>, name: &str, size: i32) -> adw::Avatar {
    let avatar = adw::Avatar::new(size, Some(name), true);
    if let Some(texture) = definition.and_then(|d| d.logo).and_then(engine_logo_texture) {
        avatar.set_custom_image(Some(&texture));
    }
    avatar.add_css_class("engine-avatar");
    avatar
}

fn download_size(definition: &EngineDefinition) -> String {
    match definition.recipe {
        InstallRecipe::Binary { variants } => select_variant(variants)
            .map(|variant| format!("{} download", format_bytes(variant.size)))
            .unwrap_or_else(|| "Not available for this CPU".to_owned()),
        InstallRecipe::Source { .. } => "Compiles in a few minutes".to_owned(),
    }
}

impl Manager {
    fn registry(&self) -> Rc<RefCell<EngineRegistry>> {
        self.controller.registry()
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        let intro = adw::PreferencesGroup::builder()
            .description("Every engine runs fully offline as its own process. Binaries come only from the official upstream releases and are verified with SHA-256 checksums before use.")
            .build();
        self.content.append(&intro);
        let detected = self.registry().borrow().detect_system_engines();
        for definition in CATALOG.iter() {
            self.content.append(&self.catalog_group(definition, &detected));
        }
        self.content.append(&self.custom_group());
    }

    fn catalog_group(
        self: &Rc<Self>,
        definition: &'static EngineDefinition,
        detected: &[crate::engine::registry::DetectedEngine],
    ) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        let record = self.registry().borrow().find_by_catalog(definition.id).cloned();
        let row = adw::ActionRow::builder()
            .title(format!("{} {}", definition.name, definition.version))
            .subtitle(definition.description)
            .subtitle_lines(3)
            .build();
        row.add_css_class("engine-row");
        row.add_prefix(&avatar_for(Some(definition), definition.name, 48));
        let status = gtk::Label::new(None);
        status.add_css_class("caption");
        status.set_xalign(0.0);
        status.set_wrap(true);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_valign(gtk::Align::Center);
        let system_copy = detected.iter().find(|d| d.definition.id == definition.id);
        match &record {
            Some(record) => {
                let state = if !record.is_runnable() {
                    status.add_css_class("error");
                    format!("Executable missing: {}", record.executable.display())
                } else if record.needs_weights() {
                    status.add_css_class("warning");
                    "Installed — choose or download a network weights file to play".to_owned()
                } else if !record.enabled {
                    "Installed · disabled".to_owned()
                } else {
                    status.add_css_class("success");
                    format!("Ready · {}", record.source.label())
                };
                status.set_label(&state);
                let configure = gtk::Button::from_icon_name("emblem-system-symbolic");
                configure.set_tooltip_text(Some("Configure"));
                configure.add_css_class("flat");
                let manager = self.clone();
                let id = record.id.clone();
                configure.connect_clicked(move |_| manager.open_details(&id));
                actions.append(&configure);
            }
            None => {
                let install = gtk::Button::with_label("Install");
                install.add_css_class("suggested-action");
                install.add_css_class("pill");
                status.set_label(&format!("Not installed · {} · {}", definition.install_summary(), download_size(definition)));
                if let Some(system) = system_copy {
                    let use_system = gtk::Button::with_label("Use System Copy");
                    use_system.add_css_class("pill");
                    use_system.set_tooltip_text(Some(&system.executable.display().to_string()));
                    let manager = self.clone();
                    let executable = system.executable.clone();
                    use_system.connect_clicked(move |_| manager.adopt(definition, executable.clone(), EngineSource::System));
                    actions.append(&use_system);
                    status.set_label(&format!("Found on this system at {}", system.executable.display()));
                }
                let manager = self.clone();
                let progress_group = group.clone();
                install.connect_clicked(move |button| {
                    button.set_sensitive(false);
                    manager.install(definition, &progress_group, button);
                });
                actions.append(&install);
            }
        }
        row.add_suffix(&actions);
        group.add(&row);
        let status_row = adw::ActionRow::new();
        status_row.set_child(Some(&{
            let holder = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            holder.set_margin_start(12);
            holder.set_margin_end(12);
            holder.set_margin_top(8);
            holder.set_margin_bottom(8);
            let license = gtk::Label::new(Some(&format!("{} · {}", definition.authors, definition.license)));
            license.add_css_class("caption");
            license.add_css_class("dim-label");
            license.set_xalign(1.0);
            license.set_hexpand(true);
            status.set_hexpand(true);
            holder.append(&status);
            holder.append(&license);
            holder
        }));
        group.add(&status_row);
        let in_progress = self
            .installs
            .borrow()
            .iter()
            .any(|(id, _)| id == definition.id);
        if in_progress {
            let note = adw::ActionRow::builder()
                .title("Installation in progress…")
                .build();
            note.add_prefix(&gtk::Spinner::builder().spinning(true).build());
            group.add(&note);
        }
        group
    }

    fn custom_group(self: &Rc<Self>) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title("Custom Engines")
            .description("Any UCI-compatible engine executable can be added")
            .build();
        let records: Vec<EngineRecord> = self
            .registry()
            .borrow()
            .records()
            .iter()
            .filter(|record| record.catalog_id.is_none())
            .cloned()
            .collect();
        for record in records {
            let row = adw::ActionRow::builder()
                .title(record.version_label())
                .subtitle(record.executable.display().to_string())
                .activatable(true)
                .build();
            row.add_prefix(&avatar_for(None, &record.display_name, 40));
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let manager = self.clone();
            let id = record.id.clone();
            row.connect_activated(move |_| manager.open_details(&id));
            group.add(&row);
        }
        let add = adw::ActionRow::builder()
            .title("Add Engine Executable…")
            .subtitle("Select an existing UCI engine program on this computer")
            .activatable(true)
            .build();
        add.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
        let manager = self.clone();
        add.connect_activated(move |_| manager.choose_custom());
        group.add(&add);
        group
    }

    fn adopt(self: &Rc<Self>, definition: &'static EngineDefinition, executable: PathBuf, source: EngineSource) {
        let threads = self
            .controller
            .settings()
            .with(|s| EngineRegistry::default_threads(s.max_threads));
        let record = EngineRecord {
            id: definition.id.to_owned(),
            catalog_id: Some(definition.id.to_owned()),
            display_name: definition.name.to_owned(),
            executable: executable.clone(),
            source,
            threads,
            hash_mb: 128,
            ..EngineRecord::default()
        };
        {
            let registry = self.registry();
            let mut registry = registry.borrow_mut();
            let mut record = record;
            if let Some(existing) = registry.find_by_catalog(definition.id) {
                record.options = existing.options.clone();
                record.weights = existing.weights.clone();
                record.threads = existing.threads;
                record.hash_mb = existing.hash_mb;
            }
            registry.upsert(record);
            let _ = registry.save();
        }
        self.refresh_options(definition.id.to_owned(), executable);
        if self.controller.settings().with(|s| s.default_engine.is_none()) {
            self.controller
                .settings()
                .update(|s| s.default_engine = Some(definition.id.to_owned()));
        }
        self.rebuild();
    }

    fn refresh_options(self: &Rc<Self>, id: String, executable: PathBuf) {
        let receiver = probe_in_background(executable);
        let manager = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(result) = receiver.recv().await {
                match result {
                    Ok(identity) => {
                        let registry = manager.registry();
                        let mut registry = registry.borrow_mut();
                        registry.update(&id, |record| {
                            record.uci_options = identity.options.clone();
                            record.reported_name = Some(identity.name.clone()).filter(|n| !n.is_empty());
                            record.reported_author = Some(identity.author.clone()).filter(|a| !a.is_empty());
                        });
                        let _ = registry.save();
                    }
                    Err(error) => manager.controller.toast(&error),
                }
            }
        });
    }

    fn install(self: &Rc<Self>, definition: &'static EngineDefinition, group: &adw::PreferencesGroup, button: &gtk::Button) {
        let missing = missing_prerequisites(definition);
        if !missing.is_empty() {
            button.set_sensitive(true);
            let command = apt_command(definition).unwrap_or_default();
            let names: Vec<&str> = missing.iter().map(|p| p.program).collect();
            let alert = adw::AlertDialog::new(
                Some(&format!("{} needs build tools", definition.name)),
                Some(&format!(
                    "{} publishes no official Linux binary, so KnightGambit compiles the official source release. These programs are missing: {}.\n\nInstall them with:\n{}",
                    definition.name,
                    names.join(", "),
                    command
                )),
            );
            alert.add_responses(&[("close", "Close"), ("copy", "Copy Command")]);
            alert.set_response_appearance("copy", adw::ResponseAppearance::Suggested);
            let controller = self.controller.clone();
            alert.connect_response(None, move |_, response| {
                if response == "copy" {
                    controller.copy_to_clipboard(&command, "Install command");
                }
            });
            alert.present(Some(&self.dialog));
            return;
        }
        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        progress.set_text(Some("Preparing…"));
        progress.set_valign(gtk::Align::Center);
        progress.set_hexpand(true);
        let cancel = gtk::Button::with_label("Cancel");
        cancel.add_css_class("pill");
        let progress_row = adw::ActionRow::new();
        let holder = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        holder.set_margin_start(12);
        holder.set_margin_end(12);
        holder.set_margin_top(10);
        holder.set_margin_bottom(10);
        holder.append(&progress);
        holder.append(&cancel);
        progress_row.set_child(Some(&holder));
        let log_buffer = gtk::TextBuffer::new(None);
        let log_view = gtk::TextView::builder()
            .buffer(&log_buffer)
            .editable(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .top_margin(6)
            .bottom_margin(6)
            .left_margin(8)
            .right_margin(8)
            .build();
        let log_scroll = gtk::ScrolledWindow::builder()
            .child(&log_view)
            .min_content_height(140)
            .max_content_height(200)
            .build();
        let log_row = adw::ExpanderRow::builder().title("Installation log").build();
        let log_holder = adw::ActionRow::new();
        log_holder.set_child(Some(&log_scroll));
        log_row.add_row(&log_holder);
        group.add(&progress_row);
        group.add(&log_row);

        let registry = self.registry();
        let install_dir = registry.borrow().install_dir(definition.id);
        let handle = Rc::new(install_engine(definition, install_dir, paths::cache_dir()));
        self.installs
            .borrow_mut()
            .push((definition.id.to_owned(), handle.clone()));
        {
            let handle = handle.clone();
            cancel.connect_clicked(move |button| {
                button.set_sensitive(false);
                handle.cancel();
            });
        }
        let manager = self.clone();
        let receiver = handle.progress.clone();
        let install_button = button.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                match event {
                    InstallProgress::Stage(text) => {
                        progress.set_text(Some(&text));
                        progress.pulse();
                    }
                    InstallProgress::Download { label, received, total } => match total {
                        Some(total) if total > 0 => {
                            progress.set_fraction(received as f64 / total as f64);
                            progress.set_text(Some(&format!(
                                "Downloading {label} — {} of {}",
                                format_bytes(received),
                                format_bytes(total)
                            )));
                        }
                        _ => {
                            progress.pulse();
                            progress.set_text(Some(&format!("Downloading {label} — {}", format_bytes(received))));
                        }
                    },
                    InstallProgress::Log(line) => {
                        let mut end = log_buffer.end_iter();
                        log_buffer.insert(&mut end, &line);
                        let mut end = log_buffer.end_iter();
                        log_buffer.insert(&mut end, "\n");
                        let line_count = log_buffer.line_count();
                        if line_count > 400 {
                            let mut start = log_buffer.start_iter();
                            let mut cut = log_buffer.iter_at_line(line_count - 400).unwrap_or(log_buffer.start_iter());
                            log_buffer.delete(&mut start, &mut cut);
                        }
                        progress.pulse();
                        let mark = log_buffer.create_mark(None, &log_buffer.end_iter(), false);
                        log_view.scroll_mark_onscreen(&mark);
                    }
                    InstallProgress::Finished(executable) => {
                        manager.finish_install(definition, executable);
                        break;
                    }
                    InstallProgress::Failed(error) => {
                        manager.installs.borrow_mut().retain(|(id, _)| id != definition.id);
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Installation failed"));
                        progress.add_css_class("error");
                        cancel.set_sensitive(false);
                        log_row.set_expanded(true);
                        let mut end = log_buffer.end_iter();
                        log_buffer.insert(&mut end, &format!("\n{error}\n"));
                        let alert = adw::AlertDialog::new(Some(&format!("Could not install {}", definition.name)), Some(&error));
                        alert.add_response("close", "Close");
                        alert.present(Some(&manager.dialog));
                        install_button.set_sensitive(true);
                        break;
                    }
                }
            }
        });
    }

    fn finish_install(self: &Rc<Self>, definition: &'static EngineDefinition, executable: PathBuf) {
        self.installs.borrow_mut().retain(|(id, _)| id != definition.id);
        self.adopt(definition, executable, EngineSource::Managed);
        self.controller
            .toast(&format!("{} {} installed and verified", definition.name, definition.version));
        if definition.weights_option.is_some() {
            self.open_details(definition.id);
        }
    }

    fn choose_custom(self: &Rc<Self>) {
        let chooser = gtk::FileDialog::builder()
            .title("Select a UCI Engine Executable")
            .modal(true)
            .build();
        let manager = self.clone();
        let parent = self.controller.ui().window.clone();
        chooser.open(Some(&parent), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            let receiver = probe_in_background(path.clone());
            let manager = manager.clone();
            glib::spawn_future_local(async move {
                let Ok(result) = receiver.recv().await else { return };
                match result {
                    Ok(identity) => {
                        let registry = manager.registry();
                        let mut registry = registry.borrow_mut();
                        let id = registry.unique_custom_id();
                        let name = if identity.name.is_empty() {
                            path.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "Custom engine".to_owned())
                        } else {
                            identity.name.clone()
                        };
                        let threads = manager
                            .controller
                            .settings()
                            .with(|s| EngineRegistry::default_threads(s.max_threads));
                        registry.upsert(EngineRecord {
                            id,
                            catalog_id: None,
                            display_name: name.clone(),
                            executable: path.clone(),
                            source: EngineSource::Custom,
                            threads,
                            hash_mb: 128,
                            reported_name: Some(identity.name.clone()).filter(|n| !n.is_empty()),
                            reported_author: Some(identity.author.clone()).filter(|a| !a.is_empty()),
                            uci_options: identity.options,
                            ..EngineRecord::default()
                        });
                        let _ = registry.save();
                        drop(registry);
                        manager.controller.toast(&format!("Added {name}"));
                        manager.rebuild();
                    }
                    Err(error) => manager
                        .controller
                        .toast(&format!("That program is not a UCI chess engine: {error}")),
                }
            });
        });
    }

    fn open_details(self: &Rc<Self>, id: &str) {
        let Some(record) = self.registry().borrow().get(id).cloned() else {
            return;
        };
        let definition = record.definition();
        let page_content = adw::PreferencesPage::new();

        let about = adw::PreferencesGroup::new();
        let header = gtk::Box::new(gtk::Orientation::Vertical, 8);
        header.set_halign(gtk::Align::Center);
        header.set_margin_bottom(12);
        let name = definition.map(|d| d.name.to_owned()).unwrap_or_else(|| record.display_name.clone());
        header.append(&avatar_for(definition, &name, 96));
        let title = gtk::Label::new(Some(&record.version_label()));
        title.add_css_class("title-2");
        header.append(&title);
        if let Some(author) = record.reported_author.clone().or_else(|| definition.map(|d| d.authors.to_owned())) {
            let author_label = gtk::Label::new(Some(&format!("by {author}")));
            author_label.add_css_class("dim-label");
            header.append(&author_label);
        }
        about.add(&header);
        if let Some(definition) = definition {
            let description = adw::ActionRow::builder()
                .title("About")
                .subtitle(definition.description)
                .subtitle_lines(6)
                .build();
            about.add(&description);
            let license = adw::ActionRow::builder()
                .title("License")
                .subtitle(definition.license)
                .build();
            about.add(&license);
            let website = adw::ActionRow::builder()
                .title("Official website")
                .subtitle(definition.homepage)
                .activatable(true)
                .build();
            website.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
            let homepage = definition.homepage.to_owned();
            let window = self.controller.ui().window.clone();
            website.connect_activated(move |_| {
                gtk::UriLauncher::new(&homepage).launch(Some(&window), gio::Cancellable::NONE, |_| {});
            });
            about.add(&website);
        }
        let location = adw::ActionRow::builder()
            .title(record.source.label())
            .subtitle(record.executable.display().to_string())
            .subtitle_selectable(true)
            .build();
        about.add(&location);
        page_content.add(&about);

        let resources = adw::PreferencesGroup::builder()
            .title("Configuration")
            .description("Global limits from Preferences still apply")
            .build();
        let enabled = adw::SwitchRow::builder()
            .title("Enabled")
            .subtitle("Disabled engines are hidden from the new game screen")
            .active(record.enabled)
            .build();
        let total_threads = crate::settings::Settings::total_threads();
        let threads = adw::SpinRow::with_range(1.0, total_threads as f64, 1.0);
        threads.set_title("CPU threads");
        threads.set_value(record.threads as f64);
        let max_memory = (crate::settings::Settings::total_memory_mb() / 2).clamp(64, 32_768);
        let hash = adw::SpinRow::with_range(1.0, max_memory as f64, 16.0);
        hash.set_title("Hash memory (MB)");
        hash.set_value(record.hash_mb as f64);
        let default_row = adw::ActionRow::builder()
            .title("Default engine")
            .subtitle("Preselected when starting a new game")
            .build();
        let default_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let is_default = self.controller.settings().with(|s| s.default_engine.as_deref() == Some(record.id.as_str()));
        default_switch.set_active(is_default);
        default_switch.set_sensitive(!is_default);
        default_row.add_suffix(&default_switch);
        default_row.set_activatable_widget(Some(&default_switch));
        resources.add(&enabled);
        resources.add(&threads);
        resources.add(&hash);
        resources.add(&default_row);
        page_content.add(&resources);
        {
            let manager = self.clone();
            let id = record.id.clone();
            enabled.connect_active_notify(move |row| {
                let active = row.is_active();
                manager.update_record(&id, |record| record.enabled = active);
            });
        }
        {
            let manager = self.clone();
            let id = record.id.clone();
            threads.connect_value_notify(move |row| {
                let value = row.value().round() as u32;
                manager.update_record(&id, |record| record.threads = value.max(1));
            });
        }
        {
            let manager = self.clone();
            let id = record.id.clone();
            hash.connect_value_notify(move |row| {
                let value = row.value().round() as u32;
                manager.update_record(&id, |record| record.hash_mb = value.max(1));
            });
        }
        {
            let controller = self.controller.clone();
            let id = record.id.clone();
            default_switch.connect_active_notify(move |switch| {
                if switch.is_active() {
                    let id = id.clone();
                    controller.settings().update(|settings| {
                        settings.default_engine = Some(id.clone());
                        settings.last_game.engine_id = Some(id);
                    });
                    switch.set_sensitive(false);
                }
            });
        }

        if let Some(definition) = definition.filter(|d| d.weights_option.is_some()) {
            page_content.add(&self.weights_group(definition, &record));
        }

        let options_group = adw::PreferencesGroup::builder()
            .title("Engine Options")
            .description("Options reported by the engine over UCI. Changes apply to the next game.")
            .build();
        self.fill_options(&options_group, &record);
        page_content.add(&options_group);

        let danger = adw::PreferencesGroup::new();
        let remove = gtk::Button::with_label(match record.source {
            EngineSource::Managed => "Uninstall Engine",
            _ => "Remove from KnightGambit",
        });
        remove.add_css_class("destructive-action");
        remove.add_css_class("pill");
        remove.set_halign(gtk::Align::Center);
        danger.add(&remove);
        page_content.add(&danger);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&page_content));
        let page = adw::NavigationPage::builder()
            .title(name.as_str())
            .tag(record.id.as_str())
            .child(&toolbar)
            .build();
        {
            let manager = self.clone();
            let id = record.id.clone();
            let source = record.source;
            let name = name.clone();
            remove.connect_clicked(move |_| manager.confirm_remove(&id, &name, source));
        }
        self.navigation.push(&page);
    }

    fn weights_group(self: &Rc<Self>, definition: &'static EngineDefinition, record: &EngineRecord) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title("Neural Network Weights")
            .description("Leela Chess Zero needs a network file. Networks are optional downloads and are never bundled.")
            .build();
        let current = adw::ActionRow::builder()
            .title("Weights file")
            .subtitle(
                record
                    .weights
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "None selected".to_owned()),
            )
            .subtitle_selectable(true)
            .build();
        let choose = gtk::Button::with_label("Choose…");
        choose.set_valign(gtk::Align::Center);
        current.add_suffix(&choose);
        group.add(&current);
        {
            let manager = self.clone();
            let id = record.id.clone();
            let current = current.clone();
            choose.connect_clicked(move |_| {
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Network weights"));
                for pattern in ["*.pb.gz", "*.pb", "*.onnx", "*.gz"] {
                    filter.add_pattern(pattern);
                }
                let filters = gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                let chooser = gtk::FileDialog::builder()
                    .title("Choose Network Weights")
                    .filters(&filters)
                    .build();
                let manager = manager.clone();
                let id = id.clone();
                let current = current.clone();
                let parent = manager.controller.ui().window.clone();
                chooser.open(Some(&parent), gio::Cancellable::NONE, move |result| {
                    if let Ok(path) = result.map(|file| file.path()) {
                        let Some(path) = path else { return };
                        current.set_subtitle(&path.display().to_string());
                        manager.update_record(&id, |record| record.weights = Some(path.clone()));
                        manager.rebuild();
                    }
                });
            });
        }
        for network in definition.networks {
            let row = adw::ActionRow::builder()
                .title(network.label)
                .subtitle(format!("Official network from storage.lczero.org · {}", format_bytes(network.size)))
                .build();
            let download = gtk::Button::with_label("Download");
            download.set_valign(gtk::Align::Center);
            row.add_suffix(&download);
            group.add(&row);
            let manager = self.clone();
            let id = record.id.clone();
            let directory = self.registry().borrow().install_dir(definition.id).join("networks");
            let current = current.clone();
            download.connect_clicked(move |button| {
                button.set_sensitive(false);
                let handle = download_network(network, directory.clone());
                let manager = manager.clone();
                let id = id.clone();
                let button = button.clone();
                let current = current.clone();
                glib::spawn_future_local(async move {
                    while let Ok(event) = handle.progress.recv().await {
                        match event {
                            InstallProgress::Download { received, total, .. } => {
                                let percent = total.map(|t| received * 100 / t.max(1)).unwrap_or(0);
                                button.set_label(&format!("{percent}%"));
                            }
                            InstallProgress::Finished(path) => {
                                current.set_subtitle(&path.display().to_string());
                                manager.update_record(&id, |record| record.weights = Some(path.clone()));
                                button.set_label("Downloaded");
                                manager.controller.toast("Network downloaded and verified");
                                manager.rebuild();
                                break;
                            }
                            InstallProgress::Failed(error) => {
                                button.set_label("Retry");
                                button.set_sensitive(true);
                                manager.controller.toast(&error);
                                break;
                            }
                            _ => {}
                        }
                    }
                });
            });
        }
        let more = adw::ActionRow::builder()
            .title("Browse official networks")
            .subtitle(LC0_NETWORKS_PAGE)
            .activatable(true)
            .build();
        more.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
        let window = self.controller.ui().window.clone();
        more.connect_activated(move |_| {
            gtk::UriLauncher::new(LC0_NETWORKS_PAGE).launch(Some(&window), gio::Cancellable::NONE, |_| {});
        });
        group.add(&more);
        group
    }

    fn fill_options(self: &Rc<Self>, group: &adw::PreferencesGroup, record: &EngineRecord) {
        let options: Vec<UciOption> = record
            .uci_options
            .iter()
            .filter(|option| !option.is_managed_by_app())
            .filter(|option| {
                record
                    .definition()
                    .and_then(|d| d.weights_option)
                    .is_none_or(|weights| !option.name.eq_ignore_ascii_case(weights))
            })
            .cloned()
            .collect();
        if options.is_empty() {
            let row = adw::ActionRow::builder()
                .title(if record.uci_options.is_empty() {
                    "Reading options from the engine…"
                } else {
                    "No additional options"
                })
                .build();
            group.add(&row);
            if record.uci_options.is_empty() && record.is_runnable() {
                let receiver = probe_in_background(record.executable.clone());
                let manager = self.clone();
                let id = record.id.clone();
                let group = group.clone();
                glib::spawn_future_local(async move {
                    let Ok(result) = receiver.recv().await else { return };
                    match result {
                        Ok(identity) => {
                            manager.update_record(&id, |record| {
                                record.uci_options = identity.options.clone();
                                record.reported_name = Some(identity.name.clone()).filter(|n| !n.is_empty());
                            });
                            group.remove(&row);
                            if let Some(record) = manager.registry().borrow().get(&id).cloned() {
                                manager.fill_options(&group, &record);
                            }
                        }
                        Err(error) => row.set_title(&format!("Could not read options: {error}")),
                    }
                });
            }
            return;
        }
        for option in options {
            let current = record.options.get(&option.name).cloned();
            let id = record.id.clone();
            let name = option.name.clone();
            let default = option.default_value().unwrap_or_default();
            let manager = self.clone();
            let store = move |value: String| {
                let name = name.clone();
                let default = default.clone();
                manager.update_record(&id, move |record| {
                    if value == default {
                        record.options.remove(&name);
                    } else {
                        record.options.insert(name, value);
                    }
                });
            };
            match &option.kind {
                UciOptionKind::Check { default } => {
                    let row = adw::SwitchRow::builder()
                        .title(option.name.as_str())
                        .active(current.map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(*default))
                        .build();
                    row.connect_active_notify(move |row| store(row.is_active().to_string()));
                    group.add(&row);
                }
                UciOptionKind::Spin { default, min, max } => {
                    let low = (*min).max(-1_000_000_000) as f64;
                    let high = (*max).min(1_000_000_000) as f64;
                    let row = adw::SpinRow::with_range(low, high.max(low + 1.0), 1.0);
                    row.set_title(&option.name);
                    let value = current.and_then(|v| v.parse::<f64>().ok()).unwrap_or(*default as f64);
                    row.set_value(value);
                    row.connect_value_notify(move |row| store((row.value().round() as i64).to_string()));
                    group.add(&row);
                }
                UciOptionKind::Combo { default, choices } => {
                    let refs: Vec<&str> = choices.iter().map(String::as_str).collect();
                    let row = adw::ComboRow::builder()
                        .title(option.name.as_str())
                        .model(&gtk::StringList::new(&refs))
                        .build();
                    let selected = current.unwrap_or_else(|| default.clone());
                    if let Some(index) = choices.iter().position(|c| c.eq_ignore_ascii_case(&selected)) {
                        row.set_selected(index as u32);
                    }
                    let choices = choices.clone();
                    row.connect_selected_notify(move |row| {
                        if let Some(choice) = choices.get(row.selected() as usize) {
                            store(choice.clone());
                        }
                    });
                    group.add(&row);
                }
                UciOptionKind::Text { default } => {
                    let row = adw::EntryRow::builder().title(option.name.as_str()).build();
                    row.set_text(&current.unwrap_or_else(|| default.clone()));
                    row.set_show_apply_button(true);
                    row.connect_apply(move |row| store(row.text().to_string()));
                    group.add(&row);
                }
                UciOptionKind::Button => {}
            }
        }
    }

    fn update_record(&self, id: &str, change: impl FnOnce(&mut EngineRecord)) {
        let registry = self.registry();
        let mut registry = registry.borrow_mut();
        if registry.update(id, change) {
            let _ = registry.save();
        }
        drop(registry);
        self.controller.engine_record_changed(id);
    }

    fn confirm_remove(self: &Rc<Self>, id: &str, name: &str, source: EngineSource) {
        let body = match source {
            EngineSource::Managed => "The engine files installed by KnightGambit will be deleted.",
            _ => "The engine program itself will not be touched.",
        };
        let alert = adw::AlertDialog::new(Some(&format!("Remove {name}?")), Some(body));
        alert.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
        alert.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        alert.set_close_response("cancel");
        let manager = self.clone();
        let id = id.to_owned();
        alert.connect_response(None, move |_, response| {
            if response != "remove" {
                return;
            }
            manager.controller.engine_record_changed(&id);
            let registry = manager.registry();
            let removed = registry.borrow_mut().remove(&id);
            let _ = registry.borrow().save();
            if removed.is_some() {
                manager.controller.settings().update(|settings| {
                    if settings.default_engine.as_deref() == Some(id.as_str()) {
                        settings.default_engine = None;
                    }
                });
            }
            manager.navigation.pop();
            manager.rebuild();
        });
        alert.present(Some(&self.dialog));
    }
}

pub fn present(controller: &Rc<Controller>) {
    let dialog = adw::Dialog::builder()
        .title("Engines")
        .content_width(680)
        .content_height(780)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let page = adw::PreferencesPage::new();
    let wrapper = adw::PreferencesGroup::new();
    wrapper.add(&content);
    page.add(&wrapper);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    let root = adw::NavigationPage::builder()
        .title("Chess Engines")
        .tag("root")
        .child(&toolbar)
        .build();
    let navigation = adw::NavigationView::new();
    navigation.add(&root);
    dialog.set_child(Some(&navigation));
    let manager = Rc::new(Manager {
        controller: controller.clone(),
        dialog: dialog.clone(),
        navigation: navigation.clone(),
        content,
        installs: RefCell::new(Vec::new()),
    });
    manager.content.set_spacing(18);
    manager.rebuild();
    {
        let manager = manager.clone();
        navigation.connect_popped(move |_, _| manager.rebuild());
    }
    dialog.present(Some(&controller.ui().window));
}
