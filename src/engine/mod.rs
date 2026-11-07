pub mod catalog;
pub mod installer;
pub mod probe;
pub mod process;
pub mod registry;
pub mod session;
pub mod strength;
pub mod uci;

use registry::EngineRecord;
use session::SessionConfig;

pub fn session_config(record: &EngineRecord, max_threads: u32, max_hash_mb: u32) -> SessionConfig {
    let mut options: Vec<(String, String)> = vec![
        ("Threads".to_owned(), record.threads.clamp(1, max_threads.max(1)).to_string()),
        ("Hash".to_owned(), record.hash_mb.clamp(1, max_hash_mb.max(1)).to_string()),
    ];
    if let (Some(definition), Some(weights)) = (record.definition(), &record.weights)
        && let Some(option) = definition.weights_option
    {
        options.push((option.to_owned(), weights.display().to_string()));
    }
    for (name, value) in &record.options {
        options.push((name.clone(), value.clone()));
    }
    SessionConfig {
        executable: record.executable.clone(),
        options,
    }
}
