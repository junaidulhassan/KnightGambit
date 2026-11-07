use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use super::process::{EngineProcess, ProcessEvent};
use super::session::EngineIdentity;
use super::uci::EngineMessage;

pub fn probe_engine(executable: &Path, timeout: Duration) -> Result<EngineIdentity, String> {
    if !executable.is_file() {
        return Err(format!("{} does not exist", executable.display()));
    }
    let mut process = EngineProcess::spawn(executable)
        .map_err(|error| format!("Could not start {}: {error}", executable.display()))?;
    let events = process.events();
    process
        .send("uci")
        .map_err(|error| format!("Could not talk to the engine: {error}"))?;
    let deadline = Instant::now() + timeout;
    let (sender, receiver) = std::sync::mpsc::channel();
    let forwarder = thread::spawn(move || {
        while let Ok(event) = events.recv_blocking() {
            let finished = matches!(
                event,
                ProcessEvent::Terminated | ProcessEvent::Message(EngineMessage::UciOk)
            );
            if sender.send(event).is_err() || finished {
                break;
            }
        }
    });
    let mut identity = EngineIdentity::default();
    let outcome = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Err("The engine did not answer the UCI handshake in time".to_owned());
        }
        match receiver.recv_timeout(remaining) {
            Ok(ProcessEvent::Message(EngineMessage::IdName(name))) => identity.name = name,
            Ok(ProcessEvent::Message(EngineMessage::IdAuthor(author))) => identity.author = author,
            Ok(ProcessEvent::Message(EngineMessage::Option(option))) => identity.options.push(option),
            Ok(ProcessEvent::Message(EngineMessage::UciOk)) => break Ok(()),
            Ok(ProcessEvent::Terminated) => {
                break Err("The program exited before completing the UCI handshake".to_owned());
            }
            Ok(_) => {}
            Err(_) => break Err("The engine did not answer the UCI handshake in time".to_owned()),
        }
    };
    process.shutdown();
    drop(forwarder);
    outcome.map(|_| identity)
}

pub fn probe_in_background(
    executable: PathBuf,
) -> async_channel::Receiver<Result<EngineIdentity, String>> {
    let (sender, receiver) = async_channel::bounded(1);
    let spawned = thread::Builder::new()
        .name("uci-probe".to_owned())
        .spawn(move || {
            let result = probe_engine(&executable, Duration::from_secs(15));
            let _ = sender.send_blocking(result);
        });
    if let Err(error) = spawned {
        let (fallback_sender, fallback) = async_channel::bounded(1);
        let _ = fallback_sender.try_send(Err(format!("Could not start probe: {error}")));
        return fallback;
    }
    receiver
}
