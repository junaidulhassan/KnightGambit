use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use super::catalog::{
    ArchiveFormat, BinaryVariant, BuildSystem, EngineDefinition, ExtraDownload, InstallRecipe,
    NetworkDownload, Prerequisite,
};
use super::probe::probe_engine;

#[derive(Debug, Clone, PartialEq)]
pub enum InstallProgress {
    Stage(String),
    Download {
        label: String,
        received: u64,
        total: Option<u64>,
    },
    Log(String),
    Finished(PathBuf),
    Failed(String),
}

pub struct InstallHandle {
    pub progress: async_channel::Receiver<InstallProgress>,
    cancel: Arc<AtomicBool>,
}

impl InstallHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

struct Reporter {
    sender: async_channel::Sender<InstallProgress>,
    cancel: Arc<AtomicBool>,
}

impl Reporter {
    fn send(&self, progress: InstallProgress) {
        let _ = self.sender.send_blocking(progress);
    }

    fn stage(&self, text: impl Into<String>) {
        self.send(InstallProgress::Stage(text.into()));
    }

    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) {
            Err("Installation cancelled".to_owned())
        } else {
            Ok(())
        }
    }
}

fn spawn_task<F>(task: F) -> InstallHandle
where
    F: FnOnce(&Reporter) -> Result<PathBuf, String> + Send + 'static,
{
    let (sender, progress) = async_channel::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let reporter = Reporter {
        sender,
        cancel: cancel.clone(),
    };
    let spawned = thread::Builder::new()
        .name("engine-installer".to_owned())
        .spawn(move || {
            let outcome = task(&reporter);
            reporter.send(match outcome {
                Ok(path) => InstallProgress::Finished(path),
                Err(error) => InstallProgress::Failed(error),
            });
        });
    if let Err(error) = spawned {
        let (sender, failed) = async_channel::bounded(1);
        let _ = sender.try_send(InstallProgress::Failed(format!(
            "Could not start installer: {error}"
        )));
        return InstallHandle {
            progress: failed,
            cancel,
        };
    }
    InstallHandle { progress, cancel }
}

pub fn cpu_flags() -> HashSet<String> {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|line| line.starts_with("flags"))
                .map(|line| {
                    line.split(':')
                        .nth(1)
                        .unwrap_or_default()
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect()
                })
        })
        .unwrap_or_default()
}

pub fn select_variant(variants: &'static [BinaryVariant]) -> Option<&'static BinaryVariant> {
    let flags = cpu_flags();
    variants.iter().find(|variant| {
        variant
            .required_cpu_flags
            .iter()
            .all(|flag| flags.contains(*flag))
    })
}

pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain([PathBuf::from("/usr/games"), PathBuf::from("/usr/local/games")])
        .map(|directory| directory.join(program))
        .find(|candidate| is_executable(candidate))
}

pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub fn missing_prerequisites(definition: &EngineDefinition) -> Vec<Prerequisite> {
    match definition.recipe {
        InstallRecipe::Binary { .. } => Vec::new(),
        InstallRecipe::Source { prerequisites, .. } => prerequisites
            .iter()
            .copied()
            .filter(|prerequisite| find_in_path(prerequisite.program).is_none())
            .collect(),
    }
}

pub fn apt_command(definition: &EngineDefinition) -> Option<String> {
    match definition.recipe {
        InstallRecipe::Binary { .. } => None,
        InstallRecipe::Source {
            prerequisites,
            extra_packages,
            ..
        } => {
            let mut packages: Vec<&str> = prerequisites.iter().map(|p| p.package).collect();
            packages.extend_from_slice(extra_packages);
            packages.dedup();
            Some(format!("sudo apt install {}", packages.join(" ")))
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .user_agent(concat!("KnightGambit/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn download(
    reporter: &Reporter,
    url: &str,
    expected_sha256: &str,
    destination: &Path,
    label: &str,
) -> Result<(), String> {
    reporter.check_cancelled()?;
    reporter.stage(format!("Downloading {label}"));
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|error| format!("Download failed for {url}: {error}"))?;
    let total = response.body().content_length();
    let mut reader = response.body_mut().as_reader();
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let partial = destination.with_extension("part");
    let mut file = File::create(&partial).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut received = 0u64;
    let mut last_report = 0u64;
    loop {
        if reporter.cancel.load(Ordering::SeqCst) {
            drop(file);
            let _ = fs::remove_file(&partial);
            return Err("Installation cancelled".to_owned());
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Download interrupted: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
        received += read as u64;
        if received - last_report >= 512 * 1024 || Some(received) == total {
            last_report = received;
            reporter.send(InstallProgress::Download {
                label: label.to_owned(),
                received,
                total,
            });
        }
    }
    file.flush().map_err(|error| error.to_string())?;
    drop(file);
    reporter.stage(format!("Verifying {label}"));
    let digest = hex(&hasher.finalize());
    if !digest.eq_ignore_ascii_case(expected_sha256) {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "Checksum mismatch for {label}. Expected {expected_sha256}, received {digest}. The download was discarded."
        ));
    }
    fs::rename(&partial, destination).map_err(|error| error.to_string())?;
    Ok(())
}

fn sanitized_relative(path: &Path, strip: usize) -> Option<PathBuf> {
    let mut output = PathBuf::new();
    for component in path.components().skip(strip) {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!output.as_os_str().is_empty()).then_some(output)
}

fn extract_tar_gz(archive: &Path, destination: &Path, strip: usize) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let mut tar = tar::Archive::new(GzDecoder::new(BufReader::new(file)));
    tar.set_preserve_permissions(true);
    for entry in tar.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path().map_err(|error| error.to_string())?.into_owned();
        let Some(relative) = sanitized_relative(&path, strip) else {
            continue;
        };
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            continue;
        }
        let target = destination.join(relative);
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        entry.unpack(&target).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn extract_member(archive: &Path, member: &str, destination: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let mut tar = tar::Archive::new(GzDecoder::new(BufReader::new(file)));
    for entry in tar.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path().map_err(|error| error.to_string())?.into_owned();
        if path == Path::new(member) {
            let mut output = File::create(destination).map_err(|error| error.to_string())?;
            io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
            return Ok(());
        }
    }
    Err(format!("The archive does not contain {member}"))
}

fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())
}

fn run_logged(reporter: &Reporter, mut command: Command, description: &str) -> Result<(), String> {
    reporter.check_cancelled()?;
    reporter.stage(description.to_owned());
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not run {description}: {error}"))?;
    let (line_sender, line_receiver) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let sender = line_sender.clone();
        readers.push(thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let sender = line_sender.clone();
        readers.push(thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        }));
    }
    drop(line_sender);
    let mut tail: Vec<String> = Vec::new();
    loop {
        match line_receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                tail.push(line.clone());
                if tail.len() > 12 {
                    tail.remove(0);
                }
                reporter.send(InstallProgress::Log(line));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if reporter.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Installation cancelled".to_owned());
        }
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{description} failed ({status}).\n{}",
            tail.join("\n")
        ))
    }
}

fn install_binary(
    reporter: &Reporter,
    definition: &EngineDefinition,
    variants: &'static [BinaryVariant],
    install_dir: &Path,
    cache_dir: &Path,
) -> Result<PathBuf, String> {
    let variant = select_variant(variants)
        .ok_or_else(|| "No release build is compatible with this processor".to_owned())?;
    reporter.send(InstallProgress::Log(format!(
        "Selected {} build ({})",
        definition.name, variant.label
    )));
    fs::create_dir_all(install_dir).map_err(|error| error.to_string())?;
    let executable = install_dir.join(definition.id);
    match variant.format {
        ArchiveFormat::RawExecutable => {
            download(reporter, variant.url, variant.sha256, &executable, definition.name)?;
        }
        ArchiveFormat::TarGz { member } => {
            let archive = cache_dir.join(format!("{}.tar.gz", definition.id));
            download(reporter, variant.url, variant.sha256, &archive, definition.name)?;
            reporter.stage("Extracting");
            let result = extract_member(&archive, member, &executable);
            let _ = fs::remove_file(&archive);
            result?;
        }
    }
    make_executable(&executable)?;
    Ok(executable)
}

#[allow(clippy::too_many_arguments)]
fn install_source(
    reporter: &Reporter,
    definition: &EngineDefinition,
    archive_url: &str,
    sha256: &str,
    extras: &[ExtraDownload],
    build: BuildSystem,
    output: &str,
    install_dir: &Path,
    cache_dir: &Path,
) -> Result<PathBuf, String> {
    let missing = missing_prerequisites(definition);
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|p| p.program).collect();
        return Err(format!(
            "Missing build tools: {}. Install them with:\n{}",
            names.join(", "),
            apt_command(definition).unwrap_or_default()
        ));
    }
    let build_dir = cache_dir.join(format!("build-{}", definition.id));
    let _ = fs::remove_dir_all(&build_dir);
    fs::create_dir_all(&build_dir).map_err(|error| error.to_string())?;
    let archive = cache_dir.join(format!("{}-source.tar.gz", definition.id));
    download(reporter, archive_url, sha256, &archive, &format!("{} source", definition.name))?;
    reporter.stage("Extracting source");
    extract_tar_gz(&archive, &build_dir, 1)?;
    let _ = fs::remove_file(&archive);
    for extra in extras {
        let destination = build_dir.join(extra.destination);
        let label = extra
            .destination
            .rsplit('/')
            .next()
            .unwrap_or(extra.destination)
            .to_owned();
        download(reporter, extra.url, extra.sha256, &destination, &label)?;
        if let Some(target) = extra.extract_into {
            let target = build_dir.join(target);
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            extract_tar_gz(&destination, &target, 1)?;
            let _ = fs::remove_file(&destination);
        }
    }
    let jobs = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(2)
        .to_string();
    match build {
        BuildSystem::Make {
            directory,
            arguments,
        } => {
            let mut command = Command::new("make");
            command
                .current_dir(build_dir.join(directory))
                .arg(format!("-j{jobs}"))
                .args(arguments);
            run_logged(reporter, command, &format!("Compiling {}", definition.name))?;
        }
        BuildSystem::Meson { options } => {
            let mut setup = Command::new("meson");
            setup
                .current_dir(&build_dir)
                .arg("setup")
                .arg("build")
                .args(options);
            run_logged(reporter, setup, "Configuring build")?;
            let mut compile = Command::new("ninja");
            compile
                .current_dir(&build_dir)
                .arg("-C")
                .arg("build")
                .arg(format!("-j{jobs}"));
            run_logged(reporter, compile, &format!("Compiling {}", definition.name))?;
        }
    }
    let built = build_dir.join(output);
    if !built.is_file() {
        return Err(format!("The build finished but {} was not produced", output));
    }
    fs::create_dir_all(install_dir).map_err(|error| error.to_string())?;
    let executable = install_dir.join(definition.id);
    fs::copy(&built, &executable).map_err(|error| error.to_string())?;
    make_executable(&executable)?;
    let _ = fs::remove_dir_all(&build_dir);
    Ok(executable)
}

pub fn install_engine(
    definition: &'static EngineDefinition,
    install_dir: PathBuf,
    cache_dir: PathBuf,
) -> InstallHandle {
    spawn_task(move |reporter| {
        fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;
        let executable = match definition.recipe {
            InstallRecipe::Binary { variants } => {
                install_binary(reporter, definition, variants, &install_dir, &cache_dir)?
            }
            InstallRecipe::Source {
                archive_url,
                sha256,
                extras,
                build,
                output,
                ..
            } => install_source(
                reporter,
                definition,
                archive_url,
                sha256,
                extras,
                build,
                output,
                &install_dir,
                &cache_dir,
            )?,
        };
        reporter.check_cancelled()?;
        reporter.stage("Verifying the UCI handshake");
        let identity = probe_engine(&executable, Duration::from_secs(20))?;
        reporter.send(InstallProgress::Log(format!(
            "Engine reports: {} by {}",
            identity.name, identity.author
        )));
        Ok(executable)
    })
}

pub fn download_network(network: &'static NetworkDownload, directory: PathBuf) -> InstallHandle {
    spawn_task(move |reporter| {
        let destination = directory.join(network.file_name);
        download(reporter, network.url, network.sha256, &destination, network.label)?;
        Ok(destination)
    })
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
