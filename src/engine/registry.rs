use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::catalog::{CATALOG, EngineDefinition, find_definition};
use super::installer::{find_in_path, is_executable};
use super::uci::UciOption;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineSource {
    Managed,
    System,
    Custom,
}

impl EngineSource {
    pub fn label(self) -> &'static str {
        match self {
            EngineSource::Managed => "Installed by KnightGambit",
            EngineSource::System => "System installation",
            EngineSource::Custom => "Custom executable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineRecord {
    pub id: String,
    pub catalog_id: Option<String>,
    pub display_name: String,
    pub executable: PathBuf,
    pub source: EngineSource,
    pub enabled: bool,
    pub threads: u32,
    pub hash_mb: u32,
    pub weights: Option<PathBuf>,
    pub options: BTreeMap<String, String>,
    pub reported_name: Option<String>,
    pub reported_author: Option<String>,
    pub uci_options: Vec<UciOption>,
}

impl Default for EngineRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            catalog_id: None,
            display_name: String::new(),
            executable: PathBuf::new(),
            source: EngineSource::Custom,
            enabled: true,
            threads: 1,
            hash_mb: 64,
            weights: None,
            options: BTreeMap::new(),
            reported_name: None,
            reported_author: None,
            uci_options: Vec::new(),
        }
    }
}

impl EngineRecord {
    pub fn definition(&self) -> Option<&'static EngineDefinition> {
        self.catalog_id.as_deref().and_then(find_definition)
    }

    pub fn is_runnable(&self) -> bool {
        is_executable(&self.executable)
    }

    pub fn is_available(&self) -> bool {
        self.enabled && self.is_runnable() && !self.needs_weights()
    }

    pub fn needs_weights(&self) -> bool {
        self.definition()
            .and_then(|definition| definition.weights_option)
            .is_some()
            && self.weights.as_ref().is_none_or(|path| !path.is_file())
    }

    pub fn version_label(&self) -> String {
        if let Some(name) = &self.reported_name {
            return name.clone();
        }
        match (self.definition(), self.source) {
            (Some(definition), EngineSource::Managed) => {
                format!("{} {}", definition.name, definition.version)
            }
            _ => self.display_name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetectedEngine {
    pub definition: &'static EngineDefinition,
    pub executable: PathBuf,
}

pub struct EngineRegistry {
    path: PathBuf,
    engines_dir: PathBuf,
    records: Vec<EngineRecord>,
}

impl EngineRegistry {
    pub fn load(path: PathBuf, engines_dir: PathBuf) -> Self {
        let records = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<EngineRecord>>(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            engines_dir,
            records,
        }
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.records).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, &self.path)
    }

    pub fn engines_dir(&self) -> &Path {
        &self.engines_dir
    }

    pub fn install_dir(&self, catalog_id: &str) -> PathBuf {
        self.engines_dir.join(catalog_id)
    }

    pub fn records(&self) -> &[EngineRecord] {
        &self.records
    }

    pub fn get(&self, id: &str) -> Option<&EngineRecord> {
        self.records.iter().find(|record| record.id == id)
    }

    pub fn find_by_catalog(&self, catalog_id: &str) -> Option<&EngineRecord> {
        self.records
            .iter()
            .find(|record| record.catalog_id.as_deref() == Some(catalog_id))
    }

    pub fn available(&self) -> Vec<EngineRecord> {
        self.records
            .iter()
            .filter(|record| record.is_available())
            .cloned()
            .collect()
    }

    pub fn upsert(&mut self, record: EngineRecord) {
        match self.records.iter_mut().find(|existing| existing.id == record.id) {
            Some(existing) => *existing = record,
            None => self.records.push(record),
        }
    }

    pub fn update(&mut self, id: &str, change: impl FnOnce(&mut EngineRecord)) -> bool {
        match self.records.iter_mut().find(|record| record.id == id) {
            Some(record) => {
                change(record);
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<EngineRecord> {
        let index = self.records.iter().position(|record| record.id == id)?;
        let record = self.records.remove(index);
        if record.source == EngineSource::Managed
            && let Some(catalog_id) = &record.catalog_id
        {
            let directory = self.install_dir(catalog_id);
            if directory.starts_with(&self.engines_dir) {
                let _ = fs::remove_dir_all(directory);
            }
        }
        Some(record)
    }

    pub fn unique_custom_id(&self) -> String {
        (1..)
            .map(|index| format!("custom-{index}"))
            .find(|candidate| self.get(candidate).is_none())
            .unwrap_or_else(|| "custom".to_owned())
    }

    pub fn detect_system_engines(&self) -> Vec<DetectedEngine> {
        CATALOG
            .iter()
            .filter_map(|definition| {
                definition
                    .executable_names
                    .iter()
                    .find_map(|name| find_in_path(name))
                    .map(|executable| DetectedEngine {
                        definition,
                        executable,
                    })
            })
            .filter(|detected| {
                !self
                    .records
                    .iter()
                    .any(|record| record.executable == detected.executable)
            })
            .collect()
    }

    pub fn default_threads(max_threads: u32) -> u32 {
        max_threads.clamp(1, 4)
    }
}
