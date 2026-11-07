use std::io;
use std::path::PathBuf;

use super::process::{EngineProcess, ProcessEvent};
use super::uci::{EngineMessage, SearchInfo, UciOption};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineIdentity {
    pub name: String,
    pub author: String,
    pub options: Vec<UciOption>,
}

impl EngineIdentity {
    pub fn find_option(&self, name: &str) -> Option<&UciOption> {
        self.options
            .iter()
            .find(|option| option.name.eq_ignore_ascii_case(name))
    }

    pub fn supports(&self, name: &str) -> bool {
        self.find_option(name).is_some()
    }
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub executable: PathBuf,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub id: u64,
    pub position: String,
    pub go: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Ready(EngineIdentity),
    Info { search_id: u64, info: SearchInfo },
    BestMove { search_id: u64, best: String },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Handshake,
    WaitingReady,
    Idle,
    Searching(u64),
    Stopping,
    Dead,
}

pub struct EngineSession {
    process: EngineProcess,
    state: State,
    identity: EngineIdentity,
    initial_options: Vec<(String, String)>,
    pending_options: Vec<(String, String)>,
    queued: Option<SearchRequest>,
    needs_new_game: bool,
    announced_ready: bool,
}

impl EngineSession {
    pub fn start(config: &SessionConfig) -> io::Result<(Self, async_channel::Receiver<ProcessEvent>)> {
        let mut process = EngineProcess::spawn(&config.executable)?;
        let events = process.events();
        process.send("uci")?;
        Ok((
            Self {
                process,
                state: State::Handshake,
                identity: EngineIdentity::default(),
                initial_options: config.options.clone(),
                pending_options: Vec::new(),
                queued: None,
                needs_new_game: true,
                announced_ready: false,
            },
            events,
        ))
    }

    pub fn identity(&self) -> &EngineIdentity {
        &self.identity
    }

    pub fn is_ready(&self) -> bool {
        self.announced_ready && self.state != State::Dead
    }

    pub fn is_dead(&self) -> bool {
        self.state == State::Dead
    }

    pub fn is_searching(&self) -> bool {
        matches!(self.state, State::Searching(_))
    }

    pub fn active_search(&self) -> Option<u64> {
        match self.state {
            State::Searching(id) => Some(id),
            _ => self.queued.as_ref().map(|request| request.id),
        }
    }

    pub fn search(&mut self, request: SearchRequest) -> Result<(), String> {
        match self.state {
            State::Dead => return Err("Engine is not running".to_owned()),
            State::Searching(_) => {
                self.queued = Some(request);
                self.send("stop")?;
                self.state = State::Stopping;
                return Ok(());
            }
            State::Idle => {
                self.queued = Some(request);
                self.drain()?;
            }
            State::Handshake | State::WaitingReady | State::Stopping => {
                self.queued = Some(request);
            }
        }
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.queued = None;
        if let State::Searching(_) = self.state {
            if self.send("stop").is_ok() {
                self.state = State::Stopping;
            }
        }
    }

    pub fn new_game(&mut self) {
        self.cancel();
        self.needs_new_game = true;
        if self.state == State::Idle {
            let _ = self.drain();
        }
    }

    pub fn set_options(&mut self, options: Vec<(String, String)>) {
        self.pending_options.extend(options);
        if self.state == State::Idle {
            let _ = self.drain();
        }
    }

    pub fn shutdown(&mut self) {
        self.queued = None;
        self.state = State::Dead;
        self.process.shutdown();
    }

    fn send(&mut self, command: &str) -> Result<(), String> {
        self.process.send(command).map_err(|error| {
            self.state = State::Dead;
            format!("Engine stopped responding: {error}")
        })
    }

    fn apply_options(&mut self, options: Vec<(String, String)>) -> Result<(), String> {
        for (name, value) in options {
            let Some(option) = self.identity.find_option(&name) else {
                continue;
            };
            let canonical = option.name.clone();
            let command = if value.is_empty() {
                format!("setoption name {canonical}")
            } else {
                format!("setoption name {canonical} value {value}")
            };
            self.send(&command)?;
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), String> {
        if self.state != State::Idle {
            return Ok(());
        }
        if !self.pending_options.is_empty() {
            let options = std::mem::take(&mut self.pending_options);
            self.apply_options(options)?;
            self.send("isready")?;
            self.state = State::WaitingReady;
            return Ok(());
        }
        if self.needs_new_game {
            self.needs_new_game = false;
            self.send("ucinewgame")?;
            self.send("isready")?;
            self.state = State::WaitingReady;
            return Ok(());
        }
        if let Some(request) = self.queued.take() {
            self.send(&request.position)?;
            self.send(&request.go)?;
            self.state = State::Searching(request.id);
        }
        Ok(())
    }

    pub fn handle(&mut self, event: ProcessEvent) -> Vec<SessionEvent> {
        let mut output = Vec::new();
        if self.state == State::Dead {
            return output;
        }
        let message = match event {
            ProcessEvent::Terminated => {
                self.state = State::Dead;
                output.push(SessionEvent::Failed(
                    "The engine process exited unexpectedly".to_owned(),
                ));
                return output;
            }
            ProcessEvent::Message(message) => message,
        };
        let result = match message {
            EngineMessage::IdName(name) => {
                self.identity.name = name;
                Ok(())
            }
            EngineMessage::IdAuthor(author) => {
                self.identity.author = author;
                Ok(())
            }
            EngineMessage::Option(option) => {
                if self.state == State::Handshake {
                    self.identity.options.push(option);
                }
                Ok(())
            }
            EngineMessage::UciOk if self.state == State::Handshake => {
                let options = std::mem::take(&mut self.initial_options);
                self.apply_options(options).and_then(|_| {
                    self.state = State::WaitingReady;
                    self.send("isready")
                })
            }
            EngineMessage::ReadyOk if self.state == State::WaitingReady => {
                self.state = State::Idle;
                if !self.announced_ready {
                    self.announced_ready = true;
                    output.push(SessionEvent::Ready(self.identity.clone()));
                }
                self.drain()
            }
            EngineMessage::Info(info) => {
                if let State::Searching(search_id) = self.state {
                    output.push(SessionEvent::Info { search_id, info });
                }
                Ok(())
            }
            EngineMessage::BestMove { best, .. } => match self.state {
                State::Searching(search_id) => {
                    self.state = State::Idle;
                    output.push(SessionEvent::BestMove { search_id, best });
                    self.drain()
                }
                State::Stopping => {
                    self.state = State::Idle;
                    self.drain()
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.state = State::Dead;
            output.push(SessionEvent::Failed(error));
        }
        output
    }
}

impl Drop for EngineSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}
