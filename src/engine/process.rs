use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::uci::{EngineMessage, parse_line};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEvent {
    Message(EngineMessage),
    Terminated,
}

pub struct EngineProcess {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    events: async_channel::Receiver<ProcessEvent>,
}

impl EngineProcess {
    pub fn spawn(executable: &Path) -> io::Result<Self> {
        let mut command = Command::new(executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(directory) = executable.parent().filter(|dir| dir.is_dir()) {
            command.current_dir(directory);
        }
        let mut attempts = 0;
        let mut child = loop {
            match command.spawn() {
                Ok(child) => break child,
                Err(error) if error.kind() == io::ErrorKind::ExecutableFileBusy && attempts < 20 => {
                    attempts += 1;
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        };
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("engine stdout unavailable"))?;
        let stdin = child.stdin.take();
        let (sender, events) = async_channel::unbounded();
        thread::Builder::new()
            .name("uci-reader".to_owned())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    if sender
                        .send_blocking(ProcessEvent::Message(parse_line(&line)))
                        .is_err()
                    {
                        return;
                    }
                }
                let _ = sender.send_blocking(ProcessEvent::Terminated);
            })?;
        Ok(Self {
            child: Some(child),
            stdin,
            events,
        })
    }

    pub fn events(&self) -> async_channel::Receiver<ProcessEvent> {
        self.events.clone()
    }

    pub fn send(&mut self, command: &str) -> io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "engine input closed"))?;
        stdin.write_all(command.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }

    pub fn is_alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.send("stop");
        let _ = self.send("quit");
        self.stdin.take();
        if let Some(child) = self.child.take() {
            reap_in_background(child, Duration::from_millis(1500));
        }
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn reap_in_background(mut child: Child, grace: Duration) {
    let reaper = move || {
        let deadline = Instant::now() + grace;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
    };
    if thread::Builder::new()
        .name("uci-reaper".to_owned())
        .spawn(reaper)
        .is_err()
    {
        eprintln!("KnightGambit: failed to spawn engine reaper thread");
    }
}

