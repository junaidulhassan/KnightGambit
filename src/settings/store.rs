use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::Settings;

pub fn load_settings(path: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

type Listener = Box<dyn Fn(&Settings)>;

struct Inner {
    path: PathBuf,
    settings: RefCell<Settings>,
    listeners: RefCell<Vec<Listener>>,
}

#[derive(Clone)]
pub struct SettingsStore {
    inner: Rc<Inner>,
}

impl SettingsStore {
    pub fn open(path: PathBuf) -> Self {
        let settings = load_settings(&path);
        Self {
            inner: Rc::new(Inner {
                path,
                settings: RefCell::new(settings),
                listeners: RefCell::new(Vec::new()),
            }),
        }
    }

    pub fn get(&self) -> Settings {
        self.inner.settings.borrow().clone()
    }

    pub fn with<R>(&self, reader: impl FnOnce(&Settings) -> R) -> R {
        reader(&self.inner.settings.borrow())
    }

    pub fn update(&self, change: impl FnOnce(&mut Settings)) {
        let snapshot = {
            let mut settings = self.inner.settings.borrow_mut();
            let before = settings.clone();
            change(&mut settings);
            if *settings == before {
                return;
            }
            settings.clone()
        };
        if let Err(error) = save_settings(&self.inner.path, &snapshot) {
            eprintln!("KnightGambit: could not save settings: {error}");
        }
        for listener in self.inner.listeners.borrow().iter() {
            listener(&snapshot);
        }
    }

    pub fn update_silently(&self, change: impl FnOnce(&mut Settings)) {
        let snapshot = {
            let mut settings = self.inner.settings.borrow_mut();
            change(&mut settings);
            settings.clone()
        };
        if let Err(error) = save_settings(&self.inner.path, &snapshot) {
            eprintln!("KnightGambit: could not save settings: {error}");
        }
    }

    pub fn connect_changed(&self, listener: impl Fn(&Settings) + 'static) {
        self.inner.listeners.borrow_mut().push(Box::new(listener));
    }
}
