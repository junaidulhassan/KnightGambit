use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::{gdk, glib};
use shakmaty::uci::UciMove;
use shakmaty::san::SanPlus;
use shakmaty::{Chess, Color, Move, Position, Role, Square};

use crate::chess::{CapturedPieces, Game, GameResult, PgnHeaders, Termination, material_balance};
use crate::clock::{ChessClock, format_clock};
use crate::engine::registry::{EngineRecord, EngineRegistry};
use crate::engine::session::{EngineIdentity, EngineSession, SearchRequest, SessionEvent};
use crate::engine::strength::{ClockState, StrengthPlan, go_command, plan_strength, position_command};
use crate::engine::uci::{Score, SearchInfo};
use crate::engine::session_config;
use crate::settings::{ColorChoice, GameMode, NewGameSettings, Settings, SettingsStore, StrengthSettings};
use crate::ui::board::{AnimationHint, BoardAppearance, BoardWidget};
use crate::ui::sound::{Sound, SoundPlayer};
use crate::review::{GameReview, MoveClass, PositionEval, classify_game};
use crate::ui::art::review_icon_texture;
use crate::ui::widgets::{AnalysisPanel, AvatarKind, GameOverCard, MoveList, PlayerBar, ReviewPanel};

pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub toasts: adw::ToastOverlay,
    pub title: adw::WindowTitle,
    pub board: BoardWidget,
    pub top_bar: PlayerBar,
    pub bottom_bar: PlayerBar,
    pub move_list: Rc<MoveList>,
    pub analysis: AnalysisPanel,
    pub status_title: gtk::Label,
    pub status_detail: gtk::Label,
    pub status_icon: gtk::Image,
    pub resign: gtk::Button,
    pub offer_draw: gtk::Button,
    pub claim_draw: gtk::Button,
    pub takeback: gtk::Button,
    pub first: gtk::Button,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub last: gtk::Button,
    pub game_over: GameOverCard,
    pub review: ReviewPanel,
    pub review_button: gtk::Button,
}

struct AnalyzerLink {
    session: EngineSession,
    generation: u64,
    name: String,
    ready: bool,
}

struct ReviewJob {
    session: EngineSession,
    generation: u64,
    engine_name: String,
    positions: Vec<Option<String>>,
    evals: Vec<Option<PositionEval>>,
    index: Option<usize>,
    movetime: u64,
    best: Option<(Score, Option<String>)>,
    second: Option<Score>,
    ply_count: usize,
}

#[derive(Debug, Clone)]
pub enum Participant {
    Human(String),
    Engine(Box<EngineRecord>),
}

impl Participant {
    fn is_engine(&self) -> bool {
        matches!(self, Participant::Engine(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchPurpose {
    Move,
    Analysis,
}

struct EngineLink {
    record: EngineRecord,
    session: EngineSession,
    identity: Option<EngineIdentity>,
    plan: Option<StrengthPlan>,
    strength: StrengthSettings,
    generation: u64,
}

#[derive(Debug, Clone)]
struct ActiveSearch {
    id: u64,
    purpose: SearchPurpose,
    position: Chess,
    started: Instant,
}

pub struct Controller {
    ui: Ui,
    settings: SettingsStore,
    registry: Rc<RefCell<EngineRegistry>>,
    sounds: SoundPlayer,
    game: RefCell<Game>,
    view_ply: Cell<usize>,
    config: RefCell<NewGameSettings>,
    participants: RefCell<[Participant; 2]>,
    clock: RefCell<Option<ChessClock>>,
    clock_source: RefCell<Option<glib::SourceId>>,
    engine: RefCell<Option<EngineLink>>,
    generation: Cell<u64>,
    search_counter: Cell<u64>,
    active_search: RefCell<Option<ActiveSearch>>,
    engine_eval: Cell<Option<i32>>,
    last_panel_update: Cell<Option<Instant>>,
    headers: RefCell<PgnHeaders>,
    low_time_warned: Cell<[bool; 2]>,
    weak_self: RefCell<Weak<Controller>>,
    review: RefCell<Option<GameReview>>,
    review_job: RefCell<Option<ReviewJob>>,
    analyzer: RefCell<Option<AnalyzerLink>>,
    analysis_search: RefCell<Option<ActiveSearch>>,
    analyzer_generation: Cell<u64>,
    review_generation: Cell<u64>,
    loaded_piece_dir: RefCell<Option<Option<String>>>,
}

fn color_slot(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

pub fn appearance_from(settings: &Settings) -> BoardAppearance {
    BoardAppearance {
        light: settings.light_color(),
        dark: settings.dark_color(),
        texture: settings.board_texture,
        style: settings.piece_style,
        coordinates: settings.coordinates,
        show_legal: settings.show_legal_moves,
        highlight_last: settings.highlight_last_move,
        animation_ms: settings.animation_speed.move_millis(),
        auto_queen: settings.auto_queen,
        art_revision: crate::assets::custom_revision(),
    }
}

fn today() -> String {
    glib::DateTime::now_local()
        .ok()
        .and_then(|now| now.format("%Y.%m.%d").ok())
        .map(|text| text.to_string())
        .unwrap_or_default()
}

impl Controller {
    pub fn new(ui: Ui, settings: SettingsStore, registry: Rc<RefCell<EngineRegistry>>, sounds: SoundPlayer) -> Rc<Self> {
        let controller = Rc::new(Self {
            ui,
            settings,
            registry,
            sounds,
            game: RefCell::new(Game::new()),
            view_ply: Cell::new(0),
            config: RefCell::new(NewGameSettings::default()),
            participants: RefCell::new([
                Participant::Human("White".to_owned()),
                Participant::Human("Black".to_owned()),
            ]),
            clock: RefCell::new(None),
            clock_source: RefCell::new(None),
            engine: RefCell::new(None),
            generation: Cell::new(0),
            search_counter: Cell::new(0),
            active_search: RefCell::new(None),
            engine_eval: Cell::new(None),
            last_panel_update: Cell::new(None),
            headers: RefCell::new(PgnHeaders::new("White", "Black")),
            low_time_warned: Cell::new([false, false]),
            weak_self: RefCell::new(Weak::new()),
            review: RefCell::new(None),
            review_job: RefCell::new(None),
            analyzer: RefCell::new(None),
            analysis_search: RefCell::new(None),
            analyzer_generation: Cell::new(0),
            review_generation: Cell::new(0),
            loaded_piece_dir: RefCell::new(None),
        });
        controller.weak_self.replace(Rc::downgrade(&controller));
        controller.connect_ui();
        controller.apply_settings(&controller.settings.get());
        controller
    }

    fn weak(&self) -> Weak<Controller> {
        self.weak_self.borrow().clone()
    }

    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    pub fn settings(&self) -> &SettingsStore {
        &self.settings
    }

    pub fn registry(&self) -> Rc<RefCell<EngineRegistry>> {
        self.registry.clone()
    }

    pub fn config(&self) -> NewGameSettings {
        self.config.borrow().clone()
    }

    fn connect_ui(&self) {
        let weak = self.weak();
        self.ui.board.connect_move_requested(move |from, to, promotion| {
            if let Some(controller) = weak.upgrade() {
                controller.submit_move(from, to, promotion);
            }
        });
        let weak = self.weak();
        self.ui.move_list.connect_select(move |ply| {
            if let Some(controller) = weak.upgrade() {
                controller.go_to_ply(ply);
            }
        });
        let bind = |button: &gtk::Button, action: fn(&Controller)| {
            let weak = self.weak();
            button.connect_clicked(move |_| {
                if let Some(controller) = weak.upgrade() {
                    action(&controller);
                }
            });
        };
        bind(&self.ui.first, |c| c.go_to_ply(0));
        bind(&self.ui.previous, |c| c.step(-1));
        bind(&self.ui.next, |c| c.step(1));
        bind(&self.ui.last, |c| c.go_to_live());
        bind(&self.ui.resign, |c| c.confirm_resign());
        bind(&self.ui.offer_draw, |c| c.offer_draw());
        bind(&self.ui.claim_draw, |c| c.claim_draw());
        bind(&self.ui.takeback, |c| c.takeback());
        bind(&self.ui.game_over.review, |c| {
            c.ui.game_over.dismiss();
            c.start_review();
        });
        bind(&self.ui.review_button, |c| c.start_review());
        bind(&self.ui.review.close, |c| c.close_review());
        let weak = self.weak();
        self.ui.review.connect_seek(move |ply| {
            if let Some(controller) = weak.upgrade() {
                controller.go_to_ply(ply);
            }
        });
        let weak = self.weak();
        self.ui.board.connect_premove(move |_, _, _| {
            if let Some(controller) = weak.upgrade() {
                controller.sounds.play(Sound::Move);
            }
        });
        bind(&self.ui.game_over.rematch, |c| c.rematch());
        bind(&self.ui.game_over.new_game, |c| {
            c.ui.game_over.dismiss();
            let _ = WidgetExt::activate_action(&c.ui.window, "win.new-game", None);
        });
        let weak = self.weak();
        self.settings.connect_changed(move |settings| {
            if let Some(controller) = weak.upgrade() {
                controller.apply_settings(settings);
            }
        });
    }

    pub fn apply_settings(&self, settings: &Settings) {
        let wanted_dir = (settings.piece_style == crate::assets::PieceStyle::Custom)
            .then(|| settings.custom_piece_dir.clone())
            .flatten();
        if self.loaded_piece_dir.borrow().as_ref() != Some(&wanted_dir) {
            crate::assets::load_custom_pieces(wanted_dir.as_deref().map(std::path::Path::new));
            self.loaded_piece_dir.replace(Some(wanted_dir));
        }
        self.ui.board.set_appearance(appearance_from(settings));
        self.sounds.configure(settings.sound_enabled, settings.sound_volume);
        let style_manager = adw::StyleManager::default();
        style_manager.set_color_scheme(match settings.theme {
            crate::settings::ThemePreference::System => adw::ColorScheme::Default,
            crate::settings::ThemePreference::Light => adw::ColorScheme::ForceLight,
            crate::settings::ThemePreference::Dark => adw::ColorScheme::ForceDark,
        });
        self.ui.analysis.root.set_visible(settings.show_analysis && self.analysis_relevant());
        self.refresh_side_panels();
        self.refresh_clocks();
    }

    pub fn toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        toast.set_timeout(4);
        self.ui.toasts.add_toast(toast);
    }

    fn mode(&self) -> GameMode {
        self.config.borrow().mode
    }

    fn participant(&self, color: Color) -> Participant {
        self.participants.borrow()[color_slot(color)].clone()
    }

    fn is_engine_turn(&self) -> bool {
        let game = self.game.borrow();
        !game.is_over() && self.participant(game.turn()).is_engine()
    }

    fn is_live_view(&self) -> bool {
        self.view_ply.get() == self.game.borrow().ply_count()
    }

    fn analysis_relevant(&self) -> bool {
        let over = self.game.borrow().is_over();
        match self.mode() {
            GameMode::Practice => true,
            GameMode::VersusEngine | GameMode::TwoPlayers => over,
        }
    }

    fn wants_background_analysis(&self) -> bool {
        let settings = self.settings.with(|s| s.show_analysis);
        if !settings || self.review_active() {
            return false;
        }
        let over = self.game.borrow().is_over();
        match self.mode() {
            GameMode::Practice => true,
            GameMode::VersusEngine | GameMode::TwoPlayers => over,
        }
    }

    pub fn start_game(&self, config: NewGameSettings) {
        let game = if config.start_fen.trim().is_empty() {
            Game::new()
        } else {
            match Game::from_fen(&config.start_fen) {
                Ok(game) => game,
                Err(error) => {
                    self.toast(&error.to_string());
                    return;
                }
            }
        };
        self.start_with_game(config, game, None);
    }

    pub fn start_with_game(&self, mut config: NewGameSettings, game: Game, headers: Option<PgnHeaders>) {
        let user_color = match config.color {
            ColorChoice::White => Color::White,
            ColorChoice::Black => Color::Black,
            ColorChoice::Random => {
                if rand::random::<bool>() {
                    Color::White
                } else {
                    Color::Black
                }
            }
        };
        let engine_record = self.resolve_engine(&config);
        if config.mode == GameMode::VersusEngine && engine_record.is_none() {
            self.toast("Install or enable a chess engine to play against the computer");
            let _ = WidgetExt::activate_action(&self.ui.window, "win.engines", None);
            return;
        }
        if let Some(record) = &engine_record {
            config.engine_id = Some(record.id.clone());
        }
        self.cancel_search();
        self.stop_clock_timer();
        self.close_review();
        self.ui.game_over.dismiss();
        self.ui.board.cancel_interaction();

        let participants = match config.mode {
            GameMode::VersusEngine => {
                let record = engine_record.clone().expect("engine checked above");
                let mut pair = [
                    Participant::Human("You".to_owned()),
                    Participant::Human("You".to_owned()),
                ];
                pair[color_slot(!user_color)] = Participant::Engine(Box::new(record));
                pair
            }
            GameMode::TwoPlayers => [
                Participant::Human("White".to_owned()),
                Participant::Human("Black".to_owned()),
            ],
            GameMode::Practice => [
                Participant::Human("White".to_owned()),
                Participant::Human("Black".to_owned()),
            ],
        };
        let names: Vec<String> = participants
            .iter()
            .map(|participant| match participant {
                Participant::Human(name) => name.clone(),
                Participant::Engine(record) => record.version_label(),
            })
            .collect();
        let clocked = config.clock_enabled && config.mode != GameMode::Practice && !game.is_over();
        let mut new_headers = headers.unwrap_or_else(|| {
            let mut headers = PgnHeaders::new(&names[0], &names[1]);
            headers.event = match config.mode {
                GameMode::VersusEngine => "Casual game against the computer".to_owned(),
                GameMode::TwoPlayers => "Casual two-player game".to_owned(),
                GameMode::Practice => "Practice".to_owned(),
            };
            headers.date = today();
            headers
        });
        if clocked {
            new_headers.time_control = Some(config.time_control.pgn_tag());
        }

        let flipped = config.mode == GameMode::VersusEngine && user_color == Color::Black;
        self.participants.replace(participants);
        self.headers.replace(new_headers);
        let ply = game.ply_count();
        self.game.replace(game);
        self.view_ply.set(ply);
        self.engine_eval.set(None);
        self.low_time_warned.set([false, false]);
        self.clock.replace(clocked.then(|| ChessClock::new(config.time_control)));
        self.config.replace(config.clone());
        self.settings.update_silently(|settings| settings.last_game = config.clone());
        self.ui.board.set_flipped(flipped);

        let wants_engine = config.mode == GameMode::VersusEngine;
        if !matches!(config.mode, GameMode::Practice) {
            self.drop_analyzer();
        }
        if wants_engine {
            if let Some(record) = engine_record {
                self.ensure_engine(record, config.strength);
            }
        } else {
            self.drop_engine();
        }

        self.refresh_all(None);
        self.sounds.play(Sound::GameStart);
        if self.game.borrow().is_over() {
            self.on_game_over(false);
            return;
        }
        if clocked {
            let turn = self.game.borrow().turn();
            if let Some(clock) = self.clock.borrow_mut().as_mut() {
                clock.start(turn, Instant::now());
            }
            self.start_clock_timer();
        }
        self.schedule_engine();
    }

    pub fn rematch(&self) {
        let mut config = self.config();
        if config.mode == GameMode::VersusEngine && config.color != ColorChoice::Random {
            config.color = match config.color {
                ColorChoice::White => ColorChoice::Black,
                _ => ColorChoice::White,
            };
        }
        self.start_game(config);
    }

    pub fn restart(&self) {
        self.start_game(self.config());
    }

    fn resolve_engine(&self, config: &NewGameSettings) -> Option<EngineRecord> {
        let registry = self.registry.borrow();
        let available = registry.available();
        let preferred = config
            .engine_id
            .clone()
            .or_else(|| self.settings.with(|s| s.default_engine.clone()));
        preferred
            .and_then(|id| {
                available
                    .iter()
                    .find(|record| record.id == id || record.catalog_id.as_deref() == Some(id.as_str()))
                    .cloned()
            })
            .or_else(|| available.first().cloned())
    }

    fn ensure_engine(&self, record: EngineRecord, strength: StrengthSettings) {
        let reusable = {
            let mut engine = self.engine.borrow_mut();
            match engine.as_mut() {
                Some(link) if link.record.id == record.id
                    && link.record.executable == record.executable
                    && link.record.threads == record.threads
                    && link.record.hash_mb == record.hash_mb
                    && link.record.weights == record.weights
                    && link.record.options == record.options
                    && !link.session.is_dead() =>
                {
                    link.session.new_game();
                    link.strength = strength;
                    link.record = record.clone();
                    if let Some(identity) = link.identity.clone() {
                        let plan = plan_strength(record.definition(), &identity, &strength);
                        link.session.set_options(plan.options.clone());
                        link.plan = Some(plan);
                    }
                    true
                }
                _ => false,
            }
        };
        if reusable {
            return;
        }
        self.drop_engine();
        let (max_threads, max_hash) = self.settings.with(|s| (s.max_threads, s.max_hash_mb));
        let config = session_config(&record, max_threads, max_hash);
        match EngineSession::start(&config) {
            Ok((session, events)) => {
                let generation = self.generation.get() + 1;
                self.generation.set(generation);
                self.engine.replace(Some(EngineLink {
                    record: record.clone(),
                    session,
                    identity: None,
                    plan: None,
                    strength,
                    generation,
                }));
                let weak = self.weak();
                glib::spawn_future_local(async move {
                    while let Ok(event) = events.recv().await {
                        let Some(controller) = weak.upgrade() else { break };
                        let produced = {
                            let mut engine = controller.engine.borrow_mut();
                            match engine.as_mut() {
                                Some(link) if link.generation == generation => link.session.handle(event),
                                _ => break,
                            }
                        };
                        for session_event in produced {
                            controller.on_session_event(session_event);
                        }
                    }
                });
            }
            Err(error) => {
                self.toast(&format!("Could not start {}: {error}", record.display_name));
            }
        }
    }

    fn drop_engine(&self) {
        if let Some(mut link) = self.engine.take() {
            link.session.shutdown();
        }
        self.active_search.replace(None);
    }

    pub fn shutdown(&self) {
        self.drop_analyzer();
        self.stop_review_job();
        self.stop_clock_timer();
        self.drop_engine();
    }

    fn on_session_event(&self, event: SessionEvent) {
        match event {
            SessionEvent::Ready(identity) => {
                {
                    let mut engine = self.engine.borrow_mut();
                    if let Some(link) = engine.as_mut() {
                        let plan = plan_strength(link.record.definition(), &identity, &link.strength);
                        link.session.set_options(plan.options.clone());
                        link.plan = Some(plan);
                        link.identity = Some(identity.clone());
                        let record_id = link.record.id.clone();
                        let options = identity.options.clone();
                        let reported = identity.name.clone();
                        let author = identity.author.clone();
                        drop(engine);
                        let mut registry = self.registry.borrow_mut();
                        registry.update(&record_id, |record| {
                            record.uci_options = options;
                            record.reported_name = Some(reported);
                            record.reported_author = Some(author);
                        });
                        let _ = registry.save();
                    }
                }
                self.refresh_side_panels();
                self.schedule_engine();
            }
            SessionEvent::Info { search_id, info } => self.on_search_info(search_id, info),
            SessionEvent::BestMove { search_id, best } => self.on_best_move(search_id, best),
            SessionEvent::Failed(message) => {
                let name = self
                    .engine
                    .borrow()
                    .as_ref()
                    .map(|link| link.record.display_name.clone())
                    .unwrap_or_else(|| "The engine".to_owned());
                self.drop_engine();
                self.toast(&format!("{name} stopped: {message}"));
                if self.mode() == GameMode::VersusEngine && !self.game.borrow().is_over() {
                    self.pause_clock();
                    self.ui.status_title.set_label("Engine unavailable");
                    self.ui
                        .status_detail
                        .set_label("Start a new game or choose another engine in the Engines manager.");
                }
                self.refresh_side_panels();
            }
        }
    }

    fn engine_ready(&self) -> bool {
        self.engine
            .borrow()
            .as_ref()
            .is_some_and(|link| link.identity.is_some() && link.session.is_ready())
    }

    fn schedule_engine(&self) {
        self.schedule_move_search();
        self.schedule_analysis();
        self.refresh_thinking();
    }

    fn schedule_move_search(&self) {
        if !self.engine_ready() {
            return;
        }
        let over = self.game.borrow().is_over();
        if !over && self.is_engine_turn() {
            let claim = self.game.borrow().available_claim();
            if claim.is_some() && self.engine_eval.get().is_some_and(|eval| eval <= 0) {
                let result = self.game.borrow_mut().claim_draw();
                if result.is_some() {
                    self.toast("The engine claims a draw");
                    self.refresh_all(None);
                    self.on_game_over(true);
                    return;
                }
            }
            let live = self.game.borrow().current().clone();
            self.request_search(SearchPurpose::Move, live);
        } else {
            self.cancel_search();
        }
    }

    fn stockfish_record(&self) -> Option<EngineRecord> {
        let existing = self.registry.borrow().find_by_catalog("stockfish").cloned();
        if let Some(record) = existing {
            return record.is_available().then_some(record);
        }
        let detected = self
            .registry
            .borrow()
            .detect_system_engines()
            .into_iter()
            .find(|engine| engine.definition.id == "stockfish")?;
        let threads = self.settings.with(|s| EngineRegistry::default_threads(s.max_threads));
        let record = EngineRecord {
            id: "stockfish".to_owned(),
            catalog_id: Some("stockfish".to_owned()),
            display_name: "Stockfish".to_owned(),
            executable: detected.executable,
            source: crate::engine::registry::EngineSource::System,
            threads,
            hash_mb: 128,
            ..EngineRecord::default()
        };
        let mut registry = self.registry.borrow_mut();
        registry.upsert(record.clone());
        let _ = registry.save();
        Some(record)
    }

    fn full_strength_config(&self, record: &EngineRecord, multipv: u32) -> crate::engine::session::SessionConfig {
        let (max_threads, max_hash) = self.settings.with(|s| (s.max_threads, s.max_hash_mb));
        let mut strongest = record.clone();
        strongest.threads = max_threads.max(1);
        strongest.hash_mb = max_hash.max(16);
        let mut config = session_config(&strongest, max_threads, max_hash);
        config.options.push(("UCI_LimitStrength".to_owned(), "false".to_owned()));
        config.options.push(("Skill Level".to_owned(), "20".to_owned()));
        config.options.push(("MultiPV".to_owned(), multipv.to_string()));
        config
    }

    fn ensure_analyzer(&self) -> bool {
        if self.analyzer.borrow().as_ref().is_some_and(|link| !link.session.is_dead()) {
            return true;
        }
        self.analyzer.replace(None);
        let Some(record) = self.stockfish_record() else {
            return false;
        };
        let config = self.full_strength_config(&record, 1);
        match EngineSession::start(&config) {
            Ok((session, events)) => {
                let generation = self.analyzer_generation.get() + 1;
                self.analyzer_generation.set(generation);
                self.analyzer.replace(Some(AnalyzerLink {
                    session,
                    generation,
                    name: "Stockfish".to_owned(),
                    ready: false,
                }));
                let weak = self.weak();
                glib::spawn_future_local(async move {
                    while let Ok(event) = events.recv().await {
                        let Some(controller) = weak.upgrade() else { break };
                        let produced = {
                            let mut analyzer = controller.analyzer.borrow_mut();
                            match analyzer.as_mut() {
                                Some(link) if link.generation == generation => link.session.handle(event),
                                _ => break,
                            }
                        };
                        for session_event in produced {
                            controller.on_analyzer_event(session_event);
                        }
                    }
                });
                true
            }
            Err(error) => {
                self.toast(&format!("Could not start Stockfish for analysis: {error}"));
                false
            }
        }
    }

    fn drop_analyzer(&self) {
        if let Some(mut link) = self.analyzer.take() {
            link.session.shutdown();
        }
        self.analysis_search.replace(None);
    }

    fn on_analyzer_event(&self, event: SessionEvent) {
        match event {
            SessionEvent::Ready(identity) => {
                if let Some(link) = self.analyzer.borrow_mut().as_mut() {
                    link.ready = true;
                    if !identity.name.is_empty() {
                        link.name = identity.name;
                    }
                }
                self.schedule_analysis();
            }
            SessionEvent::Info { search_id, info } => self.on_analysis_info(search_id, info),
            SessionEvent::BestMove { .. } => {}
            SessionEvent::Failed(message) => {
                self.drop_analyzer();
                self.toast(&format!("Stockfish analysis stopped: {message}"));
            }
        }
    }

    fn cancel_analysis(&self) {
        if let Some(link) = self.analyzer.borrow_mut().as_mut() {
            link.session.cancel();
        }
        self.analysis_search.replace(None);
    }

    fn schedule_analysis(&self) {
        if !self.wants_background_analysis() {
            self.cancel_analysis();
            return;
        }
        if !self.ensure_analyzer() {
            self.ui.analysis.show(
                "Stockfish analysis",
                "—",
                0.5,
                "Install Stockfish from Engines to see live analysis",
                "",
            );
            return;
        }
        if !self.analyzer.borrow().as_ref().is_some_and(|link| link.ready) {
            self.ui.analysis.reset("Stockfish");
            return;
        }
        let position = self.game.borrow().position_at(self.view_ply.get()).clone();
        if position.is_game_over() {
            self.cancel_analysis();
            return;
        }
        let id = self.search_counter.get() + 1;
        self.search_counter.set(id);
        let request = SearchRequest {
            id,
            position: self.position_command_for(SearchPurpose::Analysis),
            go: "go infinite".to_owned(),
        };
        let result = match self.analyzer.borrow_mut().as_mut() {
            Some(link) => link.session.search(request),
            None => return,
        };
        match result {
            Ok(()) => {
                self.analysis_search.replace(Some(ActiveSearch {
                    id,
                    purpose: SearchPurpose::Analysis,
                    position,
                    started: Instant::now(),
                }));
                let title = self.engine_title();
                self.ui.analysis.reset(&title);
            }
            Err(error) => self.toast(&error),
        }
    }

    fn position_command_for(&self, purpose: SearchPurpose) -> String {
        let game = self.game.borrow();
        let ply = match purpose {
            SearchPurpose::Move => game.ply_count(),
            SearchPurpose::Analysis => self.view_ply.get(),
        };
        let moves: Vec<String> = game.moves()[..ply].iter().map(|played| played.uci.clone()).collect();
        position_command(game.initial_fen(), game.starts_from_standard_position(), &moves)
    }

    fn request_search(&self, purpose: SearchPurpose, position: Chess) {
        let id = self.search_counter.get() + 1;
        self.search_counter.set(id);
        let go = match purpose {
            SearchPurpose::Analysis => return,
            SearchPurpose::Move => {
                let plan = self
                    .engine
                    .borrow()
                    .as_ref()
                    .and_then(|link| link.plan.clone());
                let Some(plan) = plan else { return };
                let clock_state = self.clock.borrow().as_ref().map(|clock| {
                    let now = Instant::now();
                    ClockState {
                        white: clock.remaining(Color::White, now),
                        black: clock.remaining(Color::Black, now),
                        increment: clock.control().increment(),
                    }
                });
                go_command(&plan, clock_state)
            }
        };
        let request = SearchRequest {
            id,
            position: self.position_command_for(purpose),
            go,
        };
        let result = {
            let mut engine = self.engine.borrow_mut();
            match engine.as_mut() {
                Some(link) => link.session.search(request),
                None => Err("No engine".to_owned()),
            }
        };
        match result {
            Ok(()) => {
                self.active_search.replace(Some(ActiveSearch {
                    id,
                    purpose,
                    position,
                    started: Instant::now(),
                }));
            }
            Err(error) => self.toast(&error),
        }
    }

    fn cancel_search(&self) {
        if let Some(link) = self.engine.borrow_mut().as_mut() {
            link.session.cancel();
        }
        self.active_search.replace(None);
        self.refresh_thinking();
    }

    fn engine_title(&self) -> String {
        self.analyzer
            .borrow()
            .as_ref()
            .map(|link| link.name.clone())
            .unwrap_or_else(|| "Stockfish".to_owned())
    }

    fn pv_to_san(position: &Chess, pv: &[String], limit: usize) -> String {
        let mut position = position.clone();
        let mut words = Vec::new();
        let mut number = position.fullmoves().get();
        for (index, uci) in pv.iter().take(limit).enumerate() {
            let Ok(parsed) = uci.parse::<UciMove>() else { break };
            let Ok(mv) = parsed.to_move(&position) else { break };
            let white = position.turn() == Color::White;
            if white {
                words.push(format!("{number}."));
            } else if index == 0 {
                words.push(format!("{number}…"));
            }
            words.push(SanPlus::from_move_and_play_unchecked(&mut position, mv).to_string());
            if !white {
                number += 1;
            }
        }
        words.join(" ")
    }

    fn on_search_info(&self, search_id: u64, info: SearchInfo) {
        let active = self.active_search.borrow().clone();
        let Some(active) = active.filter(|search| search.id == search_id) else {
            return;
        };
        let Some(score) = info.score else { return };
        if active.purpose == SearchPurpose::Move && !info.is_bound && !info.pv.is_empty() {
            self.engine_eval.set(Some(score.as_centipawns().clamp(-100_000, 100_000)));
        }
    }

    fn on_analysis_info(&self, search_id: u64, info: SearchInfo) {
        let active = self.analysis_search.borrow().clone();
        let Some(active) = active.filter(|search| search.id == search_id) else {
            return;
        };
        let Some(score) = info.score else { return };
        if info.pv.is_empty() || !self.settings.with(|s| s.show_analysis) {
            return;
        }
        let now = Instant::now();
        if self
            .last_panel_update
            .get()
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(120))
        {
            return;
        }
        self.last_panel_update.set(Some(now));
        let white_score = if active.position.turn() == Color::White {
            score
        } else {
            score.negate()
        };
        let share = match white_score {
            Score::Mate(moves) if moves > 0 => 1.0,
            Score::Mate(_) => 0.0,
            Score::Centipawns(cp) => 1.0 / (1.0 + (-0.004 * cp as f64).exp()),
        };
        let mut stats = Vec::new();
        if let Some(depth) = info.depth {
            stats.push(format!("Depth {depth}"));
        }
        if let Some(nodes) = info.nodes {
            stats.push(format!("{} nodes", compact_number(nodes)));
        }
        if let Some(nps) = info.nps {
            stats.push(format!("{}/s", compact_number(nps)));
        }
        let line = Self::pv_to_san(&active.position, &info.pv, 12);
        self.ui.analysis.show(
            &self.engine_title(),
            &white_score.display(),
            share,
            &stats.join(" · "),
            &line,
        );
    }

    fn on_best_move(&self, search_id: u64, best: String) {
        let active = self.active_search.borrow().clone();
        let Some(active) = active.filter(|search| search.id == search_id) else {
            return;
        };
        if active.purpose != SearchPurpose::Move {
            return;
        }
        self.active_search.replace(None);
        if !self.is_engine_turn() || best == "(none)" || best == "0000" {
            self.refresh_thinking();
            return;
        }
        let minimum = Duration::from_millis(280);
        let elapsed = active.started.elapsed();
        if elapsed < minimum {
            let weak = self.weak();
            let expected_ply = self.game.borrow().ply_count();
            glib::timeout_add_local_once(minimum - elapsed, move || {
                if let Some(controller) = weak.upgrade()
                    && controller.game.borrow().ply_count() == expected_ply
                    && controller.is_engine_turn()
                {
                    controller.apply_engine_move(&best);
                }
            });
        } else {
            self.apply_engine_move(&best);
        }
    }

    fn apply_engine_move(&self, uci: &str) {
        let mv = {
            let mut game = self.game.borrow_mut();
            match game.play_uci(uci) {
                Ok(played) => Some(played.mv),
                Err(_) => None,
            }
        };
        match mv {
            Some(mv) => self.after_move(mv),
            None => {
                self.toast(&format!("The engine suggested an illegal move ({uci}); the game was stopped"));
                self.game.borrow_mut().abandon();
                self.refresh_all(None);
                self.on_game_over(false);
            }
        }
    }

    pub fn submit_move(&self, from: Square, to: Square, promotion: Option<Role>) {
        if !self.is_live_view() || self.is_engine_turn() || self.game.borrow().is_over() {
            self.refresh_board(None);
            return;
        }
        let candidate = self.game.borrow().find_move(from, to, promotion);
        let Some(mv) = candidate else {
            self.sounds.play(Sound::Illegal);
            self.refresh_board(None);
            return;
        };
        let played = self.game.borrow_mut().play(mv).map(|played| played.mv);
        match played {
            Ok(mv) => self.after_move(mv),
            Err(_) => {
                self.sounds.play(Sound::Illegal);
                self.refresh_board(None);
            }
        }
    }

    fn after_move(&self, mv: Move) {
        self.invalidate_review();
        let ply = self.game.borrow().ply_count();
        self.view_ply.set(ply);
        let pending_premove = if self.mode() == GameMode::VersusEngine && !self.is_engine_turn() {
            self.ui.board.take_premove()
        } else {
            None
        };
        let flagged = self
            .clock
            .borrow_mut()
            .as_mut()
            .and_then(|clock| clock.press(Instant::now()));
        if let Some(flagged) = flagged {
            self.game.borrow_mut().undo_plies(1);
            self.view_ply.set(self.game.borrow().ply_count());
            self.on_flag(flagged);
            return;
        }
        let (gives_check, captured, castle, promotion, over) = {
            let game = self.game.borrow();
            let played = game.last_move().expect("a move was just played");
            (
                played.gives_check,
                played.captured.is_some(),
                played.mv.is_castle(),
                played.mv.is_promotion(),
                game.is_over(),
            )
        };
        self.refresh_all(Some(AnimationHint::Forward(mv)));
        let sound = if gives_check {
            Sound::Check
        } else if promotion {
            Sound::Promote
        } else if castle {
            Sound::Castle
        } else if captured {
            Sound::Capture
        } else {
            Sound::Move
        };
        self.sounds.play(sound);
        if over {
            self.on_game_over(true);
        } else {
            self.schedule_engine();
            if let Some((from, to, promotion)) = pending_premove {
                let delay = self.settings.with(|s| s.animation_speed.move_millis()) + 30;
                let weak = self.weak();
                let expected = self.game.borrow().ply_count();
                glib::timeout_add_local_once(Duration::from_millis(delay), move || {
                    if let Some(controller) = weak.upgrade()
                        && controller.game.borrow().ply_count() == expected
                    {
                        controller.apply_premove(from, to, promotion);
                    }
                });
            }
        }
    }

    fn apply_premove(&self, from: Square, to: Square, promotion: Option<Role>) {
        if !self.is_live_view() || self.is_engine_turn() || self.game.borrow().is_over() {
            return;
        }
        let candidate = {
            let game = self.game.borrow();
            game.find_move(from, to, promotion).or_else(|| game.find_move(from, to, None))
        };
        let Some(mv) = candidate else {
            self.refresh_board(None);
            return;
        };
        let played = self.game.borrow_mut().play(mv).map(|played| played.mv);
        if let Ok(mv) = played {
            self.after_move(mv);
        }
    }

    fn on_flag(&self, flagged: Color) {
        let result = self.game.borrow_mut().flag(flagged);
        if result.is_some() {
            self.refresh_all(None);
            self.on_game_over(true);
        }
    }

    fn on_game_over(&self, play_sound: bool) {
        self.pause_clock();
        self.stop_clock_timer();
        self.cancel_search();
        let Some(result) = self.game.borrow().result() else {
            return;
        };
        if play_sound {
            self.sounds.play(Sound::GameEnd);
        }
        self.present_result(&result);
        self.refresh_all(None);
        self.schedule_engine();
    }

    fn present_result(&self, result: &GameResult) {
        let user_colors: Vec<Color> = [Color::White, Color::Black]
            .into_iter()
            .filter(|color| !self.participant(*color).is_engine())
            .collect();
        let (icon, tone) = match result.winner {
            None => ("kg-draw-symbolic", "draw"),
            Some(winner) if self.mode() == GameMode::VersusEngine && !user_colors.contains(&winner) => {
                ("kg-checkmate-symbolic", "loss")
            }
            Some(_) => ("kg-trophy-symbolic", "win"),
        };
        let title = match (result.termination, result.winner, self.mode()) {
            (Termination::Checkmate, Some(winner), GameMode::VersusEngine) if user_colors.contains(&winner) => {
                "Checkmate — You Win!".to_owned()
            }
            (Termination::Checkmate, Some(_), _) => "Checkmate".to_owned(),
            _ => result.headline(),
        };
        self.ui
            .game_over
            .present(&title, &result.summary(), result.score_text(), icon, tone);
    }

    fn start_clock_timer(&self) {
        self.stop_clock_timer();
        let weak = self.weak();
        let source = glib::timeout_add_local(Duration::from_millis(100), move || {
            let Some(controller) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            controller.on_clock_tick()
        });
        self.clock_source.replace(Some(source));
    }

    fn stop_clock_timer(&self) {
        if let Some(source) = self.clock_source.take() {
            source.remove();
        }
    }

    fn pause_clock(&self) {
        if let Some(clock) = self.clock.borrow_mut().as_mut() {
            clock.stop(Instant::now());
        }
        self.refresh_clocks();
    }

    fn on_clock_tick(&self) -> glib::ControlFlow {
        let flagged = self
            .clock
            .borrow_mut()
            .as_mut()
            .and_then(|clock| clock.check_flag(Instant::now()));
        self.refresh_clocks();
        if let Some(color) = flagged {
            self.clock_source.replace(None);
            if !self.game.borrow().is_over() {
                self.on_flag(color);
            }
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    }

    fn refresh_clocks(&self) {
        let clock = self.clock.borrow();
        let (warning, tenths) = self.settings.with(|s| (s.low_time_warning_seconds, s.clock_tenths));
        let flipped = self.ui.board.is_flipped();
        let bottom_color = if flipped { Color::Black } else { Color::White };
        let mut warned = self.low_time_warned.get();
        let mut play_warning = false;
        for (bar, color) in [(&self.ui.bottom_bar, bottom_color), (&self.ui.top_bar, !bottom_color)] {
            match clock.as_ref() {
                Some(clock) => {
                    let remaining = clock.remaining(color, Instant::now());
                    let mut text = format_clock(remaining);
                    if !tenths && remaining.as_secs() < 20 {
                        text = format_clock(Duration::from_secs(remaining.as_secs()));
                        if let Some(index) = text.find('.') {
                            text.truncate(index);
                        }
                    }
                    let low = warning > 0 && remaining < Duration::from_secs(warning);
                    let running = clock.running_side() == Some(color);
                    if low && running && !warned[color_slot(color)] && !self.participant(color).is_engine() {
                        warned[color_slot(color)] = true;
                        play_warning = true;
                    }
                    bar.set_clock(Some(&text), running, low);
                }
                None => bar.set_clock(None, false, false),
            }
        }
        drop(clock);
        self.low_time_warned.set(warned);
        if play_warning {
            self.sounds.play(Sound::LowTime);
        }
    }

    fn refresh_thinking(&self) {
        let thinking = self
            .active_search
            .borrow()
            .as_ref()
            .is_some_and(|search| search.purpose == SearchPurpose::Move);
        let flipped = self.ui.board.is_flipped();
        let bottom_color = if flipped { Color::Black } else { Color::White };
        let turn = self.game.borrow().turn();
        for (bar, color) in [(&self.ui.bottom_bar, bottom_color), (&self.ui.top_bar, !bottom_color)] {
            bar.set_thinking(thinking && color == turn && self.participant(color).is_engine());
        }
    }

    fn refresh_board(&self, hint: Option<AnimationHint>) {
        let game = self.game.borrow();
        let ply = self.view_ply.get();
        let position = game.position_at(ply);
        let last = ply.checked_sub(1).and_then(|index| game.moves().get(index)).map(|played| {
            let from = played.mv.from().unwrap_or_else(|| played.mv.to());
            (from, Game::destination_of(&played.mv))
        });
        self.ui.board.set_position(position, last, hint);
        let check = game.checked_king(ply);
        let mate = (ply == game.ply_count() && position.is_checkmate()).then_some(check).flatten();
        self.ui.board.set_status(check, mate);
        let live = ply == game.ply_count() && !game.is_over();
        let turn = game.turn();
        let human_turn = live && !self.participant(turn).is_engine();
        let premove_color = (live
            && !human_turn
            && self.mode() == GameMode::VersusEngine
            && self.settings.with(|s| s.premoves))
        .then_some(!turn);
        drop(game);
        self.ui.board.set_movable(
            human_turn && turn == Color::White,
            human_turn && turn == Color::Black,
        );
        self.ui.board.set_premove_color(premove_color);
        self.refresh_review_marker();
    }

    fn refresh_side_panels(&self) {
        let settings = self.settings.get();
        let style = settings.piece_style;
        let game = self.game.borrow();
        let ply = self.view_ply.get();
        let flipped = self.ui.board.is_flipped();
        let bottom_color = if flipped { Color::Black } else { Color::White };
        let captured = CapturedPieces::from_moves(&game.moves()[..ply.min(game.ply_count())]);
        let balance = material_balance(game.position_at(ply).board());
        let turn = game.position_at(ply).turn();
        let over = game.is_over();
        let engine_detail = self
            .engine
            .borrow()
            .as_ref()
            .map(|link| {
                link.plan
                    .as_ref()
                    .map(|plan| format!("{} · {}", link.strength.difficulty.label(), plan.description))
                    .unwrap_or_else(|| "Starting…".to_owned())
            })
            .unwrap_or_default();
        for (bar, color) in [(&self.ui.bottom_bar, bottom_color), (&self.ui.top_bar, !bottom_color)] {
            match self.participant(color) {
                Participant::Human(name) => {
                    let detail = match self.mode() {
                        GameMode::VersusEngine => color_name(color).to_owned(),
                        GameMode::TwoPlayers => String::new(),
                        GameMode::Practice => "Practice".to_owned(),
                    };
                    bar.set_player(&name, &detail, AvatarKind::Human(color), style);
                }
                Participant::Engine(record) => {
                    let logo = record.definition().and_then(|definition| definition.logo);
                    let name = record
                        .definition()
                        .map(|definition| definition.name.to_owned())
                        .unwrap_or_else(|| record.display_name.clone());
                    bar.set_player(&name, &engine_detail, AvatarKind::Engine { name: &name, logo }, style);
                }
            }
            let advantage = match color {
                Color::White => balance,
                Color::Black => -balance,
            };
            bar.set_captured(captured.captured_by(color), !color, advantage, style);
            bar.set_active(!over && color == turn);
        }
        let annotations: Vec<Option<MoveClass>> = match self.review.borrow().as_ref() {
            Some(review) => review.moves.iter().map(|m| Some(m.class)).collect(),
            None => Vec::new(),
        };
        let first_number = game.position_at(0).fullmoves().get();
        let black_first = game.position_at(0).turn() == Color::Black;
        self.ui
            .move_list
            .set_moves(game.moves(), first_number, black_first, ply, settings.notation, &annotations);
        let has_moves = game.ply_count() > 0;
        drop(game);
        self.ui
            .analysis
            .root
            .set_visible(settings.show_analysis && self.analysis_relevant() && !self.review_active());
        self.ui.review_button.set_sensitive(has_moves && self.review_job.borrow().is_none());
        self.refresh_review_panel();
        self.refresh_status();
        self.refresh_actions();
        self.refresh_thinking();
    }

    fn refresh_status(&self) {
        let game = self.game.borrow();
        let ply = self.view_ply.get();
        let live = ply == game.ply_count();
        let (icon, title, detail) = if let (Some(result), true) = (game.result(), live) {
            ("kg-checkmate-symbolic", result.headline(), result.summary())
        } else if !live {
            let move_number = ply.div_ceil(2);
            (
                "document-open-recent-symbolic",
                "Reviewing".to_owned(),
                format!("Move {move_number} — position {ply} of {}. Press End to return to the game.", game.ply_count()),
            )
        } else {
            let turn = game.turn();
            let check = game.current().is_check();
            let title = if check {
                format!("{} is in check", color_name(turn))
            } else {
                format!("{} to move", color_name(turn))
            };
            let detail = match self.participant(turn) {
                Participant::Engine(record) => format!("{} is thinking…", record.definition().map(|d| d.name).unwrap_or(&record.display_name)),
                Participant::Human(_) => match game.available_claim() {
                    Some(crate::chess::DrawClaim::ThreefoldRepetition) => "Threefold repetition — you may claim a draw.".to_owned(),
                    Some(crate::chess::DrawClaim::FiftyMoveRule) => "Fifty moves without progress — you may claim a draw.".to_owned(),
                    None if self.mode() == GameMode::Practice => "Practice mode: you control both sides.".to_owned(),
                    None if self.mode() == GameMode::TwoPlayers => "Local two-player game.".to_owned(),
                    None => "Your move.".to_owned(),
                },
            };
            (if check { "dialog-warning-symbolic" } else { "media-playback-start-symbolic" }, title, detail)
        };
        self.ui.status_icon.set_icon_name(Some(icon));
        self.ui.status_title.set_label(&title);
        self.ui.status_detail.set_label(&detail);
        let subtitle = match self.mode() {
            GameMode::VersusEngine => {
                let engine = [Color::White, Color::Black]
                    .into_iter()
                    .find_map(|color| match self.participant(color) {
                        Participant::Engine(record) => Some(record.definition().map(|d| d.name.to_owned()).unwrap_or(record.display_name)),
                        Participant::Human(_) => None,
                    })
                    .unwrap_or_default();
                format!("Playing against {engine}")
            }
            GameMode::TwoPlayers => "Two players".to_owned(),
            GameMode::Practice => "Practice".to_owned(),
        };
        let subtitle = match self.clock.borrow().as_ref() {
            Some(clock) => format!("{subtitle} · {}", clock.control().label()),
            None => subtitle,
        };
        self.ui.title.set_subtitle(&subtitle);
    }

    fn refresh_actions(&self) {
        let game = self.game.borrow();
        let ply = self.view_ply.get();
        let over = game.is_over();
        let has_human = !self.participant(Color::White).is_engine() || !self.participant(Color::Black).is_engine();
        self.ui.first.set_sensitive(ply > 0);
        self.ui.previous.set_sensitive(ply > 0);
        self.ui.next.set_sensitive(ply < game.ply_count());
        self.ui.last.set_sensitive(ply < game.ply_count());
        self.ui.resign.set_sensitive(!over && has_human && self.mode() != GameMode::Practice);
        self.ui.offer_draw.set_sensitive(!over && self.mode() != GameMode::Practice);
        self.ui.claim_draw.set_sensitive(!over && game.available_claim().is_some());
        self.ui.claim_draw.set_visible(game.available_claim().is_some() && !over);
        self.ui.takeback.set_sensitive(game.ply_count() > 0);
    }

    fn refresh_all(&self, hint: Option<AnimationHint>) {
        self.refresh_board(hint);
        self.refresh_side_panels();
        self.refresh_clocks();
    }

    pub fn go_to_ply(&self, ply: usize) {
        let total = self.game.borrow().ply_count();
        let target = ply.min(total);
        let current = self.view_ply.get();
        if target == current {
            return;
        }
        let hint = {
            let game = self.game.borrow();
            if target == current + 1 {
                game.moves().get(current).map(|played| AnimationHint::Forward(played.mv))
            } else if target + 1 == current {
                game.moves().get(target).map(|played| AnimationHint::Backward(played.mv))
            } else {
                None
            }
        };
        self.view_ply.set(target);
        self.refresh_board(hint);
        self.refresh_side_panels();
        if target == current + 1 {
            let captured = self.game.borrow().moves().get(current).is_some_and(|played| played.captured.is_some());
            self.sounds.play(if captured { Sound::Capture } else { Sound::Move });
        }
        if self.wants_background_analysis() {
            self.schedule_engine();
        }
    }

    pub fn step(&self, delta: i32) {
        let current = self.view_ply.get() as i64;
        let target = (current + delta as i64).max(0) as usize;
        self.go_to_ply(target);
    }

    pub fn go_to_live(&self) {
        let total = self.game.borrow().ply_count();
        self.go_to_ply(total);
    }

    pub fn flip_board(&self) {
        let flipped = !self.ui.board.is_flipped();
        self.ui.board.set_flipped(flipped);
        self.refresh_side_panels();
        self.refresh_clocks();
        self.refresh_thinking();
    }

    pub fn takeback(&self) {
        let plies = {
            let game = self.game.borrow();
            if game.ply_count() == 0 {
                return;
            }
            match self.mode() {
                GameMode::VersusEngine => {
                    if self.is_engine_turn() {
                        1
                    } else if game.ply_count() >= 2 {
                        2
                    } else {
                        1
                    }
                }
                _ => 1,
            }
        };
        self.cancel_search();
        let was_over = self.game.borrow().is_over();
        self.game.borrow_mut().undo_plies(plies);
        let ply = self.game.borrow().ply_count();
        self.view_ply.set(ply);
        self.ui.game_over.dismiss();
        if self.mode() == GameMode::VersusEngine && self.is_engine_turn() && ply > 0 {
            self.game.borrow_mut().undo_plies(1);
            self.view_ply.set(self.game.borrow().ply_count());
        }
        if was_over && self.clock.borrow().is_some() {
            let turn = self.game.borrow().turn();
            if let Some(clock) = self.clock.borrow_mut().as_mut()
                && clock.flagged().is_none()
            {
                clock.start(turn, Instant::now());
            }
            self.start_clock_timer();
        } else if let Some(clock) = self.clock.borrow_mut().as_mut()
            && clock.running_side().is_some()
        {
            let turn = self.game.borrow().turn();
            let now = Instant::now();
            clock.pause(now);
            clock.start(turn, now);
        }
        self.refresh_all(None);
        self.schedule_engine();
    }

    fn human_resigning_color(&self) -> Option<Color> {
        match self.mode() {
            GameMode::VersusEngine => [Color::White, Color::Black]
                .into_iter()
                .find(|color| !self.participant(*color).is_engine()),
            GameMode::TwoPlayers => Some(self.game.borrow().turn()),
            GameMode::Practice => None,
        }
    }

    pub fn confirm_resign(&self) {
        let Some(color) = self.human_resigning_color() else { return };
        if self.game.borrow().is_over() {
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some("Resign this game?"),
            Some(&format!("{} will lose the game.", color_name(color))),
        );
        dialog.add_responses(&[("cancel", "Cancel"), ("resign", "Resign")]);
        dialog.set_response_appearance("resign", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = self.weak();
        dialog.connect_response(None, move |_, response| {
            if response == "resign"
                && let Some(controller) = weak.upgrade()
            {
                let result = controller.game.borrow_mut().resign(color);
                if result.is_some() {
                    controller.refresh_all(None);
                    controller.on_game_over(true);
                }
            }
        });
        dialog.present(Some(&self.ui.window));
    }

    pub fn offer_draw(&self) {
        if self.game.borrow().is_over() {
            return;
        }
        match self.mode() {
            GameMode::VersusEngine => {
                let ply = self.game.borrow().ply_count();
                let eval = self.engine_eval.get();
                let accepts = match eval {
                    Some(eval) => eval <= -60 || (ply >= 60 && eval.abs() <= 25),
                    None => false,
                };
                if accepts {
                    self.game.borrow_mut().agree_draw();
                    self.toast("The engine accepts your draw offer");
                    self.refresh_all(None);
                    self.on_game_over(true);
                } else {
                    self.toast("The engine declines the draw offer");
                }
            }
            GameMode::TwoPlayers => {
                let offering = self.game.borrow().turn();
                let dialog = adw::AlertDialog::new(
                    Some("Draw offered"),
                    Some(&format!("{} offers a draw. Does {} accept?", color_name(offering), color_name(!offering))),
                );
                dialog.add_responses(&[("decline", "Decline"), ("accept", "Accept Draw")]);
                dialog.set_response_appearance("accept", adw::ResponseAppearance::Suggested);
                dialog.set_close_response("decline");
                let weak = self.weak();
                dialog.connect_response(None, move |_, response| {
                    if response == "accept"
                        && let Some(controller) = weak.upgrade()
                        && controller.game.borrow_mut().agree_draw().is_some()
                    {
                        controller.refresh_all(None);
                        controller.on_game_over(true);
                    }
                });
                dialog.present(Some(&self.ui.window));
            }
            GameMode::Practice => {}
        }
    }

    pub fn claim_draw(&self) {
        if self.is_engine_turn() {
            return;
        }
        let result = self.game.borrow_mut().claim_draw();
        if result.is_some() {
            self.refresh_all(None);
            self.on_game_over(true);
        }
    }

    pub fn pgn_text(&self) -> String {
        let game = self.game.borrow();
        crate::chess::write_pgn(&game, &self.headers.borrow())
    }

    pub fn fen_text(&self) -> String {
        self.game.borrow().fen_at(self.view_ply.get())
    }

    pub fn copy_to_clipboard(&self, text: &str, what: &str) {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(text);
            self.toast(&format!("{what} copied to the clipboard"));
        }
    }

    pub fn load_pgn(&self, text: &str) {
        match crate::chess::parse_pgn(text) {
            Ok(parsed) => {
                let mut config = self.config();
                config.mode = GameMode::Practice;
                config.clock_enabled = false;
                config.start_fen.clear();
                let game = parsed.game;
                let summary = format!("Loaded {} vs {}", parsed.headers.white, parsed.headers.black);
                self.start_with_game(config, game, Some(parsed.headers));
                self.toast(&summary);
            }
            Err(error) => self.toast(&error.to_string()),
        }
    }

    pub fn load_fen(&self, fen: &str) -> Result<(), String> {
        let game = Game::from_fen(fen).map_err(|error| error.to_string())?;
        let mut config = self.config();
        if config.mode == GameMode::VersusEngine {
            config.color = match game.turn() {
                Color::White => ColorChoice::White,
                Color::Black => ColorChoice::Black,
            };
        }
        config.start_fen = fen.trim().to_owned();
        self.start_with_game(config, game, None);
        Ok(())
    }

    pub fn has_unfinished_game(&self) -> bool {
        let game = self.game.borrow();
        !game.is_over() && game.ply_count() > 0 && self.mode() != GameMode::Practice
    }

    pub fn current_engine_id(&self) -> Option<String> {
        self.engine.borrow().as_ref().map(|link| link.record.id.clone())
    }

    pub fn engine_record_changed(&self, id: &str) {
        if id == "stockfish" {
            self.drop_analyzer();
        }
        let is_current = self.current_engine_id().as_deref() == Some(id);
        if is_current && (self.game.borrow().is_over() || self.mode() == GameMode::Practice) {
            self.drop_engine();
        }
    }
}

impl Controller {
    fn review_active(&self) -> bool {
        self.review.borrow().is_some() || self.review_job.borrow().is_some()
    }

    fn stop_review_job(&self) {
        if let Some(mut job) = self.review_job.take() {
            job.session.shutdown();
        }
    }

    pub fn close_review(&self) {
        self.stop_review_job();
        self.review.replace(None);
        self.ui.board.set_review_marker(None);
        self.ui.board.set_arrow(None);
        self.ui.review.hide();
        self.refresh_side_panels();
        if self.wants_background_analysis() {
            self.schedule_engine();
        }
    }

    fn invalidate_review(&self) {
        let ply_count = self.game.borrow().ply_count();
        let stale_review = self
            .review
            .borrow()
            .as_ref()
            .is_some_and(|review| review.moves.len() != ply_count);
        let stale_job = self
            .review_job
            .borrow()
            .as_ref()
            .is_some_and(|job| job.ply_count != ply_count);
        if stale_review || stale_job {
            self.stop_review_job();
            self.review.replace(None);
            self.ui.board.set_review_marker(None);
            self.ui.board.set_arrow(None);
            self.ui.review.hide();
        }
    }

    pub fn start_review(&self) {
        let ply_count = self.game.borrow().ply_count();
        if ply_count == 0 {
            self.toast("Play or load a game before starting a review");
            return;
        }
        let Some(record) = self.stockfish_record() else {
            let toast = adw::Toast::builder()
                .title("Game Review uses Stockfish — install it to review your games")
                .button_label("Engines")
                .action_name("win.engines")
                .timeout(8)
                .build();
            self.ui.toasts.add_toast(toast);
            return;
        };
        self.stop_review_job();
        self.review.replace(None);
        self.cancel_search();
        self.ui.game_over.dismiss();
        self.cancel_analysis();
        let movetime = self.settings.with(|s| s.review_depth.movetime_ms());
        let config = self.full_strength_config(&record, 2);
        let (positions, evals) = {
            let game = self.game.borrow();
            let mut positions = Vec::with_capacity(ply_count + 1);
            let mut evals = Vec::with_capacity(ply_count + 1);
            for ply in 0..=ply_count {
                let position = game.position_at(ply);
                match PositionEval::terminal(position) {
                    Some(eval) => {
                        positions.push(None);
                        evals.push(Some(eval));
                    }
                    None => {
                        let moves: Vec<String> = game.moves()[..ply].iter().map(|m| m.uci.clone()).collect();
                        positions.push(Some(position_command(
                            game.initial_fen(),
                            game.starts_from_standard_position(),
                            &moves,
                        )));
                        evals.push(None);
                    }
                }
            }
            (positions, evals)
        };
        let engine_name = record
            .definition()
            .map(|d| d.name.to_owned())
            .unwrap_or_else(|| record.display_name.clone());
        match EngineSession::start(&config) {
            Ok((session, events)) => {
                let generation = self.review_generation.get() + 1;
                self.review_generation.set(generation);
                self.review_job.replace(Some(ReviewJob {
                    session,
                    generation,
                    engine_name: engine_name.clone(),
                    positions,
                    evals,
                    index: None,
                    movetime,
                    best: None,
                    second: None,
                    ply_count,
                }));
                let weak = self.weak();
                glib::spawn_future_local(async move {
                    while let Ok(event) = events.recv().await {
                        let Some(controller) = weak.upgrade() else { break };
                        let produced = {
                            let mut job = controller.review_job.borrow_mut();
                            match job.as_mut() {
                                Some(job) if job.generation == generation => job.session.handle(event),
                                _ => break,
                            }
                        };
                        for session_event in produced {
                            controller.on_review_event(session_event);
                        }
                    }
                });
                self.ui.review.show_progress(0, ply_count + 1, &engine_name);
                self.refresh_side_panels();
            }
            Err(error) => self.toast(&format!("Could not start {engine_name}: {error}")),
        }
    }

    fn on_review_event(&self, event: SessionEvent) {
        match event {
            SessionEvent::Ready(_) => self.review_next(),
            SessionEvent::Info { search_id, info } => {
                let mut job = self.review_job.borrow_mut();
                let Some(job) = job.as_mut() else { return };
                if job.index.map(|index| index as u64 + 1) != Some(search_id) || info.is_bound {
                    return;
                }
                let Some(score) = info.score else { return };
                match info.multipv {
                    Some(2) => job.second = Some(score),
                    _ => job.best = Some((score, info.pv.first().cloned())),
                }
            }
            SessionEvent::BestMove { search_id, best } => {
                {
                    let mut job = self.review_job.borrow_mut();
                    let Some(job) = job.as_mut() else { return };
                    let Some(index) = job.index.filter(|index| *index as u64 + 1 == search_id) else {
                        return;
                    };
                    let (score, pv_move) = job.best.take().unwrap_or((Score::Centipawns(0), None));
                    let best = Some(best).filter(|m| m != "(none)" && m != "0000").or(pv_move);
                    job.evals[index] = Some(PositionEval {
                        best,
                        score,
                        second: job.second.take(),
                    });
                    job.index = None;
                }
                self.review_next();
            }
            SessionEvent::Failed(message) => {
                self.stop_review_job();
                self.toast(&format!("Game review stopped: {message}"));
                self.ui.review.hide();
                self.refresh_side_panels();
            }
        }
    }

    fn review_next(&self) {
        let (next, done, total, name) = {
            let job = self.review_job.borrow();
            let Some(job) = job.as_ref() else { return };
            if job.index.is_some() {
                return;
            }
            let done = job.evals.iter().filter(|eval| eval.is_some()).count();
            let next = job.evals.iter().position(|eval| eval.is_none());
            (next, done, job.evals.len(), job.engine_name.clone())
        };
        self.ui.review.show_progress(done, total, &name);
        match next {
            Some(index) => {
                let outcome = {
                    let mut guard = self.review_job.borrow_mut();
                    let Some(job) = guard.as_mut() else { return };
                    let Some(position) = job.positions[index].clone() else { return };
                    job.index = Some(index);
                    job.best = None;
                    job.second = None;
                    let request = SearchRequest {
                        id: index as u64 + 1,
                        position,
                        go: format!("go movetime {}", job.movetime),
                    };
                    job.session.search(request)
                };
                if let Err(error) = outcome {
                    self.toast(&error);
                    self.stop_review_job();
                    self.ui.review.hide();
                }
            }
            None => self.finish_review(),
        }
    }

    fn finish_review(&self) {
        let Some(mut job) = self.review_job.take() else { return };
        job.session.shutdown();
        let evals: Vec<PositionEval> = job.evals.into_iter().flatten().collect();
        let review = classify_game(&self.game.borrow(), &evals);
        let blunders = review.counts[0][MoveClass::Blunder.index()] + review.counts[1][MoveClass::Blunder.index()];
        let brilliant = review.counts[0][MoveClass::Brilliant.index()] + review.counts[1][MoveClass::Brilliant.index()];
        self.review.replace(Some(review));
        if self.view_ply.get() == 0 {
            self.view_ply.set(1);
            self.refresh_board(None);
        }
        self.refresh_side_panels();
        self.refresh_review_marker();
        self.toast(&format!(
            "Review complete: {brilliant} brilliant, {blunders} blunder{}",
            if blunders == 1 { "" } else { "s" }
        ));
    }

    fn refresh_review_panel(&self) {
        if self.review_job.borrow().is_some() {
            return;
        }
        match self.review.borrow().as_ref() {
            Some(review) => {
                let names = {
                    let headers = self.headers.borrow();
                    (headers.white.clone(), headers.black.clone())
                };
                self.ui.review.show_review(review, &names.0, &names.1, self.view_ply.get());
            }
            None => self.ui.review.hide(),
        }
    }

    fn refresh_review_marker(&self) {
        let review = self.review.borrow();
        let ply = self.view_ply.get();
        let Some(item) = review.as_ref().and_then(|review| review.move_at(ply)) else {
            self.ui.board.set_review_marker(None);
            self.ui.board.set_arrow(None);
            return;
        };
        let game = self.game.borrow();
        let Some(played) = game.moves().get(ply - 1) else { return };
        let square = Game::destination_of(&played.mv);
        let scale = self.ui.board.imp_device_scale();
        let pixels = (self.ui.board.width().max(64) as f64 / 8.0 * 0.38 * scale).ceil() as u32;
        let texture = review_icon_texture(item.class, pixels.max(16));
        self.ui.board.set_review_marker(texture.map(|texture| (square, texture)));
        let arrow = (!item.played_best)
            .then_some(item.best_uci.as_deref())
            .flatten()
            .and_then(|uci| uci.parse::<UciMove>().ok())
            .and_then(|uci| uci.to_move(game.position_at(ply - 1)).ok())
            .and_then(|mv| mv.from().map(|from| (from, Game::destination_of(&mv))));
        self.ui.board.set_arrow(arrow);
    }
}

fn compact_number(value: u64) -> String {
    match value {
        0..1_000 => value.to_string(),
        1_000..1_000_000 => format!("{:.1}k", value as f64 / 1_000.0),
        1_000_000..1_000_000_000 => format!("{:.1}M", value as f64 / 1_000_000.0),
        _ => format!("{:.1}G", value as f64 / 1_000_000_000.0),
    }
}
