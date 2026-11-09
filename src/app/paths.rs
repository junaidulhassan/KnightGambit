use std::path::PathBuf;

use gtk::glib;

pub const APP_DIR: &str = "knightgambit";

pub fn config_dir() -> PathBuf {
    glib::user_config_dir().join(APP_DIR)
}

pub fn data_dir() -> PathBuf {
    glib::user_data_dir().join(APP_DIR)
}

pub fn cache_dir() -> PathBuf {
    glib::user_cache_dir().join(APP_DIR)
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn history_db_file() -> PathBuf {
    data_dir().join("history.db")
}

pub fn engines_file() -> PathBuf {
    config_dir().join("engines.json")
}

pub fn engines_dir() -> PathBuf {
    data_dir().join("engines")
}

pub fn sounds_dir() -> PathBuf {
    cache_dir().join("sounds")
}

pub fn development_icons_dir() -> Option<PathBuf> {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data").join("icons");
    candidate.is_dir().then_some(candidate)
}
