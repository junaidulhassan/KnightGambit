use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gtk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sound {
    Move,
    Capture,
    Castle,
    Check,
    Promote,
    GameStart,
    GameEnd,
    LowTime,
    Illegal,
}

impl Sound {
    const ALL: [Sound; 9] = [
        Sound::Move,
        Sound::Capture,
        Sound::Castle,
        Sound::Check,
        Sound::Promote,
        Sound::GameStart,
        Sound::GameEnd,
        Sound::LowTime,
        Sound::Illegal,
    ];

    fn file_name(self) -> &'static str {
        match self {
            Sound::Move => "move.wav",
            Sound::Capture => "capture.wav",
            Sound::Castle => "castle.wav",
            Sound::Check => "check.wav",
            Sound::Promote => "promote.wav",
            Sound::GameStart => "start.wav",
            Sound::GameEnd => "end.wav",
            Sound::LowTime => "lowtime.wav",
            Sound::Illegal => "illegal.wav",
        }
    }
}

const SAMPLE_RATE: u32 = 44_100;
const SYNTH_VERSION: &str = "2";

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn knock(samples: &mut [f32], offset_seconds: f32, pitch: f32, gain: f32, seed: u32) {
    let start = (offset_seconds * SAMPLE_RATE as f32) as usize;
    let length = (0.11 * SAMPLE_RATE as f32) as usize;
    let mut noise = Noise(seed);
    let mut low = 0.0f32;
    for index in 0..length {
        let Some(slot) = samples.get_mut(start + index) else { break };
        let t = index as f32 / SAMPLE_RATE as f32;
        let body = (TAU * pitch * t).sin() * (-t * 48.0).exp();
        let overtone = (TAU * pitch * 2.63 * t).sin() * (-t * 85.0).exp() * 0.45;
        let raw = noise.next();
        low += (raw - low) * 0.32;
        let click = low * (-t * 220.0).exp() * 0.9;
        *slot += (body + overtone + click) * gain;
    }
}

fn tone(samples: &mut [f32], offset_seconds: f32, frequency: f32, duration: f32, gain: f32) {
    let start = (offset_seconds * SAMPLE_RATE as f32) as usize;
    let length = (duration * SAMPLE_RATE as f32) as usize;
    for index in 0..length {
        let Some(slot) = samples.get_mut(start + index) else { break };
        let t = index as f32 / SAMPLE_RATE as f32;
        let attack = (t / 0.008).min(1.0);
        let envelope = attack * (-t * 5.5).exp();
        let wave = (TAU * frequency * t).sin() + 0.25 * (TAU * frequency * 2.0 * t).sin() + 0.08 * (TAU * frequency * 3.0 * t).sin();
        *slot += wave * envelope * gain;
    }
}

fn synthesize(sound: Sound) -> Vec<f32> {
    let duration = match sound {
        Sound::GameStart | Sound::GameEnd => 1.1,
        Sound::Check | Sound::Promote => 0.6,
        _ => 0.3,
    };
    let mut samples = vec![0.0f32; (duration * SAMPLE_RATE as f32) as usize];
    match sound {
        Sound::Move => knock(&mut samples, 0.0, 210.0, 0.9, 0x1234_5678),
        Sound::Capture => {
            knock(&mut samples, 0.0, 260.0, 1.0, 0x2468_ace0);
            knock(&mut samples, 0.035, 170.0, 0.8, 0x1357_9bdf);
        }
        Sound::Castle => {
            knock(&mut samples, 0.0, 200.0, 0.85, 0x0bad_f00d);
            knock(&mut samples, 0.09, 230.0, 0.8, 0x0dec_afe0);
        }
        Sound::Check => {
            knock(&mut samples, 0.0, 230.0, 0.7, 0x5eed_0001);
            tone(&mut samples, 0.02, 880.0, 0.45, 0.22);
            tone(&mut samples, 0.10, 1174.7, 0.40, 0.18);
        }
        Sound::Promote => {
            knock(&mut samples, 0.0, 220.0, 0.8, 0x5eed_0002);
            tone(&mut samples, 0.04, 659.3, 0.4, 0.2);
            tone(&mut samples, 0.12, 987.8, 0.4, 0.18);
        }
        Sound::GameStart => {
            tone(&mut samples, 0.0, 523.3, 0.8, 0.22);
            tone(&mut samples, 0.12, 659.3, 0.8, 0.2);
            tone(&mut samples, 0.24, 784.0, 0.8, 0.2);
        }
        Sound::GameEnd => {
            tone(&mut samples, 0.0, 784.0, 0.9, 0.2);
            tone(&mut samples, 0.16, 659.3, 0.9, 0.2);
            tone(&mut samples, 0.32, 523.3, 1.0, 0.24);
        }
        Sound::LowTime => {
            tone(&mut samples, 0.0, 1318.5, 0.12, 0.25);
            tone(&mut samples, 0.14, 1318.5, 0.12, 0.25);
        }
        Sound::Illegal => tone(&mut samples, 0.0, 196.0, 0.18, 0.3),
    }
    let peak = samples.iter().fold(0.0f32, |peak, value| peak.max(value.abs()));
    if peak > 0.0 {
        let normalize = 0.85 / peak;
        for value in &mut samples {
            *value *= normalize;
        }
    }
    let fade = (0.01 * SAMPLE_RATE as f32) as usize;
    let length = samples.len();
    for index in 0..fade.min(length) {
        samples[length - 1 - index] *= index as f32 / fade as f32;
    }
    samples
}

fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_length = samples.len() as u32 * 2;
    let mut bytes = Vec::with_capacity(44 + data_length as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_length).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_length.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn prepare_files(directory: &Path) -> Option<HashMap<Sound, PathBuf>> {
    let versioned = directory.join(SYNTH_VERSION);
    fs::create_dir_all(&versioned).ok()?;
    let mut files = HashMap::new();
    for sound in Sound::ALL {
        let path = versioned.join(sound.file_name());
        if !path.is_file() {
            fs::write(&path, encode_wav(&synthesize(sound))).ok()?;
        }
        files.insert(sound, path);
    }
    Some(files)
}

#[derive(Debug, Clone)]
enum Backend {
    PipeWire(PathBuf),
    Pulse(PathBuf),
    Alsa(PathBuf),
    GtkMedia,
}

fn detect_backend() -> Backend {
    let find = |name: &str| crate::engine::installer::find_in_path(name);
    if let Some(path) = find("pw-play") {
        Backend::PipeWire(path)
    } else if let Some(path) = find("paplay") {
        Backend::Pulse(path)
    } else if let Some(path) = find("aplay") {
        Backend::Alsa(path)
    } else {
        Backend::GtkMedia
    }
}

pub struct SoundPlayer {
    files: HashMap<Sound, PathBuf>,
    backend: Backend,
    streams: RefCell<HashMap<Sound, gtk::MediaFile>>,
    enabled: Cell<bool>,
    volume: Cell<f64>,
}

impl SoundPlayer {
    pub fn new(directory: &Path) -> Self {
        Self {
            files: prepare_files(directory).unwrap_or_default(),
            backend: detect_backend(),
            streams: RefCell::new(HashMap::new()),
            enabled: Cell::new(true),
            volume: Cell::new(0.7),
        }
    }

    pub fn configure(&self, enabled: bool, volume: f64) {
        self.enabled.set(enabled);
        self.volume.set(volume.clamp(0.0, 1.0));
    }

    pub fn play(&self, sound: Sound) {
        if !self.enabled.get() || self.volume.get() <= 0.0 {
            return;
        }
        let Some(path) = self.files.get(&sound).cloned() else { return };
        let volume = self.volume.get();
        let command = match &self.backend {
            Backend::PipeWire(program) => {
                let mut command = Command::new(program);
                command.arg(format!("--volume={volume:.2}")).arg(&path);
                command
            }
            Backend::Pulse(program) => {
                let mut command = Command::new(program);
                command
                    .arg(format!("--volume={}", (volume * 65536.0) as u32))
                    .arg(&path);
                command
            }
            Backend::Alsa(program) => {
                let mut command = Command::new(program);
                command.arg("-q").arg(&path);
                command
            }
            Backend::GtkMedia => {
                self.play_with_gtk(sound, &path);
                return;
            }
        };
        spawn_detached(command);
    }

    fn play_with_gtk(&self, sound: Sound, path: &Path) {
        let mut streams = self.streams.borrow_mut();
        let stream = streams
            .entry(sound)
            .or_insert_with(|| gtk::MediaFile::for_filename(path));
        if stream.error().is_some() {
            return;
        }
        stream.set_volume(self.volume.get());
        if stream.is_playing() || stream.is_ended() {
            stream.seek(0);
        }
        stream.play();
    }
}

fn spawn_detached(mut command: Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(mut child) = command.spawn() {
        let _ = std::thread::Builder::new()
            .name("sound".to_owned())
            .spawn(move || {
                let _ = child.wait();
            });
    }
}
