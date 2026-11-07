//! Application layer: screens, sessions, input mapping, camera. Independent of the window
//! system, so the same code runs in the game window and in headless capture mode.
use super::bot::Bot;
use super::data::*;
use super::net::{self, Client, ClientState, Command, LanBrowser, Server, ServerConfig};
use super::player::{Player, PlayerInput};
use super::save::{self, SaveInfo, Slot};
use super::view::{self, Camera};
use super::{tile_of, World};
use super::touch::TouchState;
use crate::audio::{Audio, Cmd, OstRender, Sfx, Track};
use crate::zip::ZipWriter;
use crate::render::{FrameData, MenuView};
use glam::{vec3, Vec2};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
use winit::keyboard::KeyCode as K;

#[derive(Default)]
pub struct RawInput {
    pub down: HashSet<K>,
    pub pressed: Vec<K>,
    /// Text typed this frame, for menu fields.
    pub typed: Vec<char>,
    pub mouse: (f32, f32),
    pub lmb: bool,
    pub rmb: bool,
    pub lmb_pressed: bool,
    pub rmb_pressed: bool,
    pub wheel: i32,
    pub touches: Vec<super::touch::TouchEv>,
}

impl RawInput {
    pub fn hit(&self, k: K) -> bool {
        self.pressed.contains(&k)
    }
    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.typed.clear();
        self.lmb_pressed = false;
        self.rmb_pressed = false;
        self.wheel = 0;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Title,
    Playing,
    Paused,
    GameOver,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Main,
    NewGame,
    Multiplayer,
    Host,
    Join,
    Connecting,
    Records,
    Options,
    Manual,
    Extras,
    Gallery,
    Soundtrack,
}

/// Who owns the simulation on this machine.
pub enum Session {
    Solo,
    Host(Box<Server>),
    Client(Box<Client>),
}

/// Requests made from the UI, carried out at the top of the next update.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    NewGame { permadeath: bool },
    Continue(Slot),
    StartHost,
    Join,
    ToTitle { save: bool },
    LoadLast,
    Quit,
}

/// Two right clicks closer together than this, in seconds, reset the camera.
const DOUBLE_CLICK: f32 = 0.35;
/// Camera rotation per pixel of mouse movement, in radians.
const CAM_TURN: f32 = 0.006;

/// Remembered between runs.
#[derive(Clone, Debug)]
pub struct Settings {
    pub name: String,
    pub address: String,
    pub server_name: String,
    pub port: String,
    pub max_players: u8,
    pub host_permadeath: bool,
    pub host_continue: bool,
    pub music: f32,
    pub sfx: f32,
    pub fullscreen: bool,
    /// "auto", or "on" and "off" once the player has decided.
    pub touch: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            name: "SURVIVOR".into(),
            address: "127.0.0.1".into(),
            server_name: "ROTFALL SERVER".into(),
            port: net::DEFAULT_PORT.to_string(),
            max_players: 4,
            host_permadeath: false,
            host_continue: false,
            music: 0.8,
            sfx: 0.9,
            fullscreen: false,
            touch: "auto".into(),
        }
    }
}

impl Settings {
    fn load(path: &std::path::Path) -> Settings {
        let mut s = Settings::default();
        let Some(text) = crate::storage::read_text(path) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "name" => s.name = v.into(),
                "address" => s.address = v.into(),
                "server_name" => s.server_name = v.into(),
                "port" => s.port = v.into(),
                "max_players" => s.max_players = v.parse::<u8>().unwrap_or(4).clamp(1, net::MAX_PLAYERS_LIMIT),
                "host_permadeath" => s.host_permadeath = v == "1",
                "music" => s.music = v.parse::<f32>().unwrap_or(0.8).clamp(0.0, 1.0),
                "sfx" => s.sfx = v.parse::<f32>().unwrap_or(0.9).clamp(0.0, 1.0),
                "fullscreen" => s.fullscreen = v == "1",
                "touch" => s.touch = v.into(),
                _ => {}
            }
        }
        s
    }

    fn save(&self, path: &std::path::Path) {
        let text = format!(
            "name={}\naddress={}\nserver_name={}\nport={}\nmax_players={}\nhost_permadeath={}\nmusic={}\nsfx={}\nfullscreen={}\ntouch={}\n",
            self.name, self.address, self.server_name, self.port, self.max_players,
            u8::from(self.host_permadeath), self.music, self.sfx, u8::from(self.fullscreen), self.touch
        );
        let _ = save::write_atomic(path, text.as_bytes());
    }
}

/// One finished run on the records page.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub mode: String,
    pub wave: u32,
    pub kills: u32,
    pub seconds: u32,
    pub name: String,
}

pub fn load_records(path: &std::path::Path) -> Vec<Record> {
    let Some(text) = crate::storage::read_text(path) else { return Vec::new() };
    text.lines().filter_map(|l| {
        let p: Vec<&str> = l.splitn(5, '|').collect();
        Some(Record { mode: (*p.first()?).to_string(), wave: p.get(1)?.parse().ok()?, kills: p.get(2)?.parse().ok()?, seconds: p.get(3)?.parse().ok()?, name: (*p.get(4)?).to_string() })
    }).collect()
}

pub fn add_record(records: &mut Vec<Record>, r: Record, path: &std::path::Path) {
    records.push(r);
    records.sort_by(|a, b| (b.wave, b.kills).cmp(&(a.wave, a.kills)));
    records.truncate(12);
    let text: String = records.iter().map(|r| format!("{}|{}|{}|{}|{}\n", r.mode, r.wave, r.kills, r.seconds, r.name)).collect();
    let _ = save::write_atomic(path, text.as_bytes());
}

pub struct App {
    pub world: World,
    pub session: Session,
    pub cam: Camera,
    pub screen: Screen,
    pub page: Page,
    pub armory_open: bool,
    pub help_open: bool,
    pub panel_open: bool,
    pub build_sel: Option<usize>,
    pub hover_tile: (i32, i32),
    pub ui_over: bool,
    pub ui_note: Option<(String, f32)>,
    pub action: Option<Action>,
    pub quit: bool,
    pub show_fps: bool,
    pub fps: f32,
    pub frame_ms: f32,
    pub settings: Settings,
    pub password: String,
    pub focus: Option<u8>,
    pub menu_error: Option<String>,
    pub lan: Option<LanBrowser>,
    pub saves: [Option<SaveInfo>; 4],
    pub records: Vec<Record>,
    pub menu_t: f32,
    was_previewing: bool,
    ui_hot: u64,
    ui_mouse: (f32, f32),
    /// Right mouse: dragging the camera, where the pointer last was, and the last press for double clicks.
    cam_drag: bool,
    rmb_mouse: (f32, f32),
    rmb_clock: f32,
    rmb_press: Option<(f32, (f32, f32))>,
    pub data_dir: PathBuf,
    /// Weapon chosen in the armory, applied through the normal input path.
    pub queued_select: Option<usize>,
    pub want_fullscreen: Option<bool>,
    /// Set when the run's end has been written to the records.
    pub run_recorded: bool,
    pub save_erased: bool,
    audio: Audio,
    pub bot: Bot,
    seed: u64,
    last_placed: Option<(i32, i32)>,
    pub autoplay: bool,
    /// Set when a new map exists, so the window layer can repaint the ground.
    pub ground_dirty: bool,
    inputs: Vec<PlayerInput>,
    /// What is playing, and the name card announcing it.
    pub music_now: Option<Track>,
    pub toast: Option<(Track, f32)>,
    /// A track being auditioned in the Extras menu.
    pub preview: Option<Track>,
    pub gallery_i: usize,
    /// Soundtrack files waiting to render, the one in progress, and what happened last.
    pub ost_queue: VecDeque<Track>,
    pub ost: Option<OstRender>,
    pub ost_status: String,
    /// The browser's "download all": one archive, as pages may not start eleven downloads.
    pub ost_zip: Option<ZipWriter>,
    pub touch: TouchState,
}

const WEAPON_KEYS: [K; 9] = [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9];
const BUILD_KEYS: [K; 10] = [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9, K::Digit0];
pub const SLOTS: [Slot; 4] = [Slot::Solo, Slot::Permadeath, Slot::Host, Slot::HostPermadeath];

impl App {
    pub fn menu_sound(&self, id: Sfx, gain: f32) {
        self.audio.send(Cmd::Play { id, gain, pan: 0.0, pitch: 1.0 });
    }

    /// Plays what the interface asked for this frame, and notices the pointer reaching a new
    /// control. A control appearing under a still pointer is not the player doing anything, and
    /// touchscreens have no hover at all.
    pub fn menu_sounds(&mut self, hot: u64, mouse: (f32, f32), sounds: &[(Sfx, f32, f32)]) {
        let moved = mouse != self.ui_mouse;
        self.ui_mouse = mouse;
        if hot != self.ui_hot {
            self.ui_hot = hot;
            if hot != 0 && moved && sounds.is_empty() && !self.touch.enabled {
                self.menu_sound(Sfx::MenuHover, 0.5);
            }
        }
        for &(id, gain, pitch) in sounds {
            self.audio.send(Cmd::Play { id, gain, pan: 0.0, pitch });
        }
    }

    pub fn new(seed: u64, audio: Audio) -> App {
        Self::with_dir(seed, audio, save::data_dir())
    }

    /// `dir` holds saves, settings and records.
    pub fn with_dir(seed: u64, audio: Audio, dir: PathBuf) -> App {
        let settings = Settings::load(&dir.join("config.txt"));
        audio.send(Cmd::MusicVolume(settings.music));
        audio.send(Cmd::SfxVolume(settings.sfx));
        let want_fullscreen = settings.fullscreen.then_some(true);
        let mut app = App {
            world: World::new(seed, Audio::silent()),
            session: Session::Solo,
            cam: Camera::new(),
            screen: Screen::Title,
            page: Page::Main,
            armory_open: false,
            help_open: false,
            panel_open: false,
            build_sel: None,
            hover_tile: (0, 0),
            ui_over: false,
            ui_note: None,
            action: None,
            quit: false,
            show_fps: false,
            fps: 0.0,
            frame_ms: 0.0,
            settings,
            password: String::new(),
            focus: None,
            menu_error: None,
            lan: None,
            saves: [None, None, None, None],
            records: load_records(&dir.join("records.txt")),
            menu_t: 0.0,
            was_previewing: false,
            ui_hot: 0,
            ui_mouse: (0.0, 0.0),
            cam_drag: false,
            rmb_mouse: (0.0, 0.0),
            rmb_clock: 0.0,
            rmb_press: None,
            data_dir: dir,
            queued_select: None,
            want_fullscreen,
            run_recorded: false,
            save_erased: false,
            audio,
            bot: Bot::new(),
            seed,
            last_placed: None,
            autoplay: false,
            ground_dirty: false,
            inputs: Vec::new(),
            music_now: None,
            toast: None,
            preview: None,
            gallery_i: 0,
            ost_queue: VecDeque::new(),
            ost: None,
            ost_status: String::new(),
            ost_zip: None,
            touch: TouchState::default(),
        };
        app.refresh_saves();
        app
    }

    pub fn refresh_saves(&mut self) {
        for (i, s) in SLOTS.iter().enumerate() {
            self.saves[i] = save::peek(&self.data_dir.join(s.file()));
        }
    }

    pub fn slot_info(&self, s: Slot) -> Option<&SaveInfo> {
        SLOTS.iter().position(|x| *x == s).and_then(|i| self.saves[i].as_ref())
    }

    pub fn store_settings(&self) {
        self.settings.save(&self.data_dir.join("config.txt"));
    }

    pub fn apply_volume(&self) {
        self.audio.send(Cmd::MusicVolume(self.settings.music));
        self.audio.send(Cmd::SfxVolume(self.settings.sfx));
    }

    pub fn is_multiplayer(&self) -> bool {
        !matches!(self.session, Session::Solo)
    }

    pub fn is_host(&self) -> bool {
        matches!(self.session, Session::Host(_))
    }

    pub fn is_client(&self) -> bool {
        matches!(self.session, Session::Client(_))
    }

    fn current_slot(&self) -> Option<Slot> {
        match self.session {
            Session::Client(_) => None,
            Session::Solo => Some(Slot::for_mode(false, self.world.permadeath)),
            Session::Host(_) => Some(Slot::for_mode(true, self.world.permadeath)),
        }
    }

    /// Writes the run to its slot. Clients have nothing to save, and a finished run stays finished.
    pub fn save_game(&mut self) -> Result<(), String> {
        let slot = self.current_slot().ok_or("ONLY THE HOST CAN SAVE")?;
        if self.world.game_over {
            return Err("THIS RUN IS OVER".into());
        }
        save::write_atomic(&self.data_dir.join(slot.file()), &save::encode(&self.world)).map_err(|e| format!("SAVE FAILED: {e}"))?;
        self.refresh_saves();
        Ok(())
    }

    /// Saves on the way out, for example when the window is closed mid-run.
    pub fn shutdown(&mut self) {
        if self.screen != Screen::Title && !self.autoplay {
            let _ = self.save_game();
        }
        self.store_settings();
    }

    fn reset_view(&mut self) {
        self.armory_open = false;
        self.help_open = false;
        self.panel_open = false;
        self.build_sel = None;
        self.queued_select = None;
        self.run_recorded = false;
        self.save_erased = false;
        self.ground_dirty = true;
        self.bot = Bot::new();
        self.lan = None;
        self.focus = None;
        self.menu_error = None;
    }

    fn start_solo(&mut self, permadeath: bool) {
        self.seed = self.seed.wrapping_add(1);
        self.world = World::new(self.seed, self.audio.clone());
        self.world.permadeath = permadeath;
        self.world.players[0].name = self.player_name();
        self.session = Session::Solo;
        self.reset_view();
        self.screen = Screen::Playing;
        if permadeath {
            // One life from the first second: an old run in this slot is over.
            crate::storage::remove(&self.data_dir.join(Slot::Permadeath.file()));
            self.refresh_saves();
            self.world.announce("PERMADEATH - ONE LIFE", [1.0, 0.25, 0.15], 4.0, true);
        }
    }

    fn player_name(&self) -> String {
        let n: String = self.settings.name.chars().take(16).collect::<String>().trim().to_ascii_uppercase();
        if n.is_empty() { "SURVIVOR".to_string() } else { n }
    }

    fn load_world(&mut self, slot: Slot) -> Result<World, String> {
        let bytes = crate::storage::read(&self.data_dir.join(slot.file())).ok_or_else(|| "NO SAVED GAME".to_string())?;
        save::decode(&bytes, self.audio.clone())
    }

    fn continue_game(&mut self, slot: Slot) {
        match self.load_world(slot) {
            Ok(mut w) => {
                w.local = 0;
                w.players[0].remote = false;
                w.players[0].connected = true;
                self.world = w;
                self.session = Session::Solo;
                self.reset_view();
                self.screen = Screen::Playing;
                self.world.announce("GAME LOADED", [0.5, 1.0, 0.6], 2.5, false);
            }
            Err(e) => self.menu_error = Some(e),
        }
    }

    fn start_host(&mut self) {
        let port: u16 = match self.settings.port.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                self.menu_error = Some("PORT MUST BE A NUMBER UP TO 65535".into());
                return;
            }
        };
        let name = self.player_name();
        let perm = self.settings.host_permadeath;
        let mut world = if self.settings.host_continue {
            match self.load_world(Slot::for_mode(true, perm)) {
                Ok(mut w) => {
                    // The host takes back their slot; everyone else is away until they rejoin.
                    for p in &mut w.players {
                        p.connected = false;
                        p.remote = true;
                    }
                    let i = w.players.iter().position(|p| p.name == name).unwrap_or_else(|| {
                        let i = w.players.len();
                        w.players.push(Player::new(&name, i));
                        i
                    });
                    w.local = i;
                    w.players[i].connected = true;
                    w.players[i].remote = false;
                    w
                }
                Err(e) => {
                    self.menu_error = Some(e);
                    return;
                }
            }
        } else {
            self.seed = self.seed.wrapping_add(1);
            let mut w = World::new(self.seed, self.audio.clone());
            w.players[0].name = name;
            w
        };
        world.hold_timer = true;
        let cfg = ServerConfig {
            name: self.settings.server_name.clone(),
            port,
            max_players: self.settings.max_players,
            password: self.password.clone(),
            permadeath: perm,
        };
        match Server::start(cfg, &mut world) {
            Ok(server) => {
                self.world = world;
                self.session = Session::Host(Box::new(server));
                self.reset_view();
                self.screen = Screen::Playing;
                self.panel_open = true;
                self.store_settings();
            }
            Err(e) => {
                self.menu_error = Some(if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("PORT {port} IS ALREADY IN USE")
                } else {
                    format!("COULD NOT OPEN PORT {port}")
                });
            }
        }
    }

    fn start_join(&mut self) {
        let addr = self.settings.address.trim().to_string();
        if addr.is_empty() {
            self.menu_error = Some("ENTER AN ADDRESS".into());
            return;
        }
        self.menu_error = None;
        self.session = Session::Client(Box::new(Client::connect(&addr, &self.player_name(), &self.password)));
        self.page = Page::Connecting;
        self.store_settings();
    }

    fn poll_connecting(&mut self) {
        let Session::Client(c) = &mut self.session else {
            self.page = Page::Join;
            return;
        };
        if let Some(w) = c.poll_welcome() {
            let mut world = World::new(w.seed, self.audio.clone());
            world.is_client = true;
            world.local = w.index;
            world.permadeath = w.permadeath;
            while world.players.len() <= w.index {
                let i = world.players.len();
                world.players.push(Player::new("", i));
            }
            world.players[w.index].name = self.player_name();
            world.messages.clear();
            self.world = world;
            self.reset_view();
            self.screen = Screen::Playing;
            self.panel_open = true;
        } else if let ClientState::Closed(why) = &c.state {
            self.menu_error = Some(why.clone());
            self.session = Session::Solo;
            self.page = Page::Join;
        }
    }

    fn back_to_title(&mut self, save_first: bool) {
        if save_first {
            let _ = self.save_game();
        }
        self.store_settings();
        self.session = Session::Solo; // Closes any sockets.
        self.seed = self.seed.wrapping_add(1);
        self.world = World::new(self.seed, Audio::silent());
        self.reset_view();
        self.autoplay = false;
        self.screen = Screen::Title;
        self.page = Page::Main;
        self.audio.send(Cmd::Intensity(0.0));
        self.refresh_saves();
    }

    fn run_action(&mut self, a: Action) {
        match a {
            Action::NewGame { permadeath } => self.start_solo(permadeath),
            Action::Continue(slot) => self.continue_game(slot),
            Action::StartHost => self.start_host(),
            Action::Join => self.start_join(),
            Action::ToTitle { save } => self.back_to_title(save),
            Action::LoadLast => {
                if let Some(slot) = self.current_slot() {
                    if !self.world.permadeath && matches!(self.session, Session::Solo) {
                        self.continue_game(slot);
                        if let Some(e) = self.menu_error.take() {
                            self.note(&e);
                        }
                    }
                }
            }
            Action::Quit => self.quit = true,
        }
    }

    /// Run a demo game from the start at the given wave; used by capture and bench modes.
    pub fn start_autoplay(&mut self, wave: u32, scrap: u32) {
        self.start_solo(false);
        self.autoplay = true;
        self.world.wave = wave.saturating_sub(1);
        self.world.scrap = scrap;
        self.world.phase = super::Phase::Build { timer: 3.0 };
        self.world.invincible = true;
    }

    pub fn update(&mut self, dt: f32, raw: &RawInput, size: (u32, u32)) {
        let (w, h) = (size.0 as f32, size.1.max(1) as f32);
        if let Some(n) = &mut self.ui_note {
            n.1 -= dt;
            if n.1 <= 0.0 {
                self.ui_note = None;
            }
        }
        if raw.hit(K::F3) {
            self.show_fps = !self.show_fps;
        }
        if raw.hit(K::KeyM) && self.focus.is_none() {
            self.settings.music = if self.settings.music > 0.0 { 0.0 } else { 0.8 };
            self.apply_volume();
        }
        if raw.hit(K::F11) {
            self.settings.fullscreen = !self.settings.fullscreen;
            self.want_fullscreen = Some(self.settings.fullscreen);
        }
        if let Some(a) = self.action.take() {
            match a {
                Action::NewGame { .. } | Action::Continue(_) | Action::StartHost | Action::Join | Action::LoadLast => {
                    self.menu_sound(Sfx::MenuConfirm, 0.9);
                    self.cam.yaw = 0.0;
                    self.cam.yaw_home = false;
                }
                Action::ToTitle { .. } => self.menu_sound(Sfx::MenuBack, 0.8),
                Action::Quit => {}
            }
            self.run_action(a);
        }

        match self.screen {
            Screen::Title => {
                self.menu_t += dt;
                match self.page {
                    Page::Connecting => self.poll_connecting(),
                    Page::Join => {
                        self.lan.get_or_insert_with(LanBrowser::new).poll();
                    }
                    _ => self.lan = None,
                }
                if self.screen == Screen::Title && raw.hit(K::Escape) && self.page != Page::Main {
                    if self.page == Page::Connecting {
                        self.session = Session::Solo;
                    }
                    self.page = match self.page {
                        Page::Host | Page::Join | Page::Connecting => Page::Multiplayer,
                        Page::Gallery | Page::Soundtrack => Page::Extras,
                        _ => Page::Main,
                    };
                    self.focus = None;
                    self.menu_error = None;
                    self.store_settings();
                    self.menu_sound(Sfx::MenuBack, 0.8);
                }
                if self.screen == Screen::Title && self.page == Page::Main && raw.hit(K::Enter) {
                    self.action = Some(match self.slot_info(Slot::Solo) {
                        Some(_) => Action::Continue(Slot::Solo),
                        None => Action::NewGame { permadeath: false },
                    });
                }
            }
            Screen::Paused => {
                if raw.hit(K::Escape) {
                    self.screen = Screen::Playing;
                    self.menu_sound(Sfx::MenuBack, 0.8);
                }
                // A network game has other people in it, so it cannot stop for you.
                if self.is_multiplayer() {
                    let input = PlayerInput { aim: self.world.me().aim, ..Default::default() };
                    self.step_world(dt, input);
                }
            }
            Screen::GameOver => {
                let input = PlayerInput { aim: self.world.me().aim, ..Default::default() };
                self.step_world(dt, input);
            }
            Screen::Playing => {
                if self.autoplay {
                    let input = self.bot.think(&mut self.world, dt);
                    self.step_world(dt, input);
                } else {
                    self.play_input(dt, raw, w, h);
                }
            }
        }
        self.update_music_state(dt);
        self.advance_soundtrack_export();
        if self.screen != Screen::Title {
            if self.world.game_over {
                self.on_game_over();
                // Permadeath lingers on the body before the numbers come up.
                let linger = if self.world.permadeath { 4.0 } else { 1.5 };
                if self.screen == Screen::Playing && self.world.game_over_timer > linger {
                    self.screen = Screen::GameOver;
                    self.armory_open = false;
                    self.build_sel = None;
                    self.panel_open = false;
                }
            }
            self.update_camera(if self.screen == Screen::Paused && !self.is_multiplayer() { 0.0 } else { dt }, w, h);
            self.world.listener = self.cam.focus;
            self.world.listener_right = super::player::rotate_y(glam::Vec3::X, self.cam.yaw);
        }
    }

    /// The run has ended: write it into the records, and in permadeath erase the save.
    fn on_game_over(&mut self) {
        if self.run_recorded || self.autoplay {
            return;
        }
        self.run_recorded = true;
        let st = self.world.stats;
        let mode = match (self.world.permadeath, self.is_multiplayer()) {
            (true, false) => "PERMADEATH",
            (true, true) => "CO-OP PERMADEATH",
            (false, false) => "STANDARD",
            (false, true) => "CO-OP",
        };
        let name = self.player_name();
        let path = self.data_dir.join("records.txt");
        add_record(&mut self.records, Record { mode: mode.into(), wave: st.wave_reached, kills: st.kills, seconds: st.time as u32, name }, &path);
        if self.world.permadeath {
            if let Some(slot) = self.current_slot() {
                crate::storage::remove(&self.data_dir.join(slot.file()));
                self.save_erased = true;
            }
            self.refresh_saves();
        }
    }

    /// Starts whatever should be playing, and announces it with a name card when it changes.
    fn update_music_state(&mut self, dt: f32) {
        if self.page != Page::Soundtrack || self.screen != Screen::Title {
            self.preview = None;
        }
        let want = if self.screen == Screen::Title { self.preview.unwrap_or(Track::Menu) } else { self.world.wave_track() };
        let previewing = self.preview.is_some();
        if self.music_now != Some(want) {
            self.music_now = Some(want);
            if self.screen == Screen::Title {
                self.audio.send(Cmd::Intensity(if previewing { 1.0 } else { 0.0 }));
            }
            // Pressing play on the soundtrack page, or stop, should be heard the moment it is pressed.
            self.audio.send(if previewing || self.was_previewing { Cmd::MusicCut(want) } else { Cmd::Music(want) });
            if self.settings.music > 0.0 {
                self.toast = Some((want, 4.5));
            }
        }
        if let Some(t) = &mut self.toast {
            t.1 -= dt;
            if t.1 <= 0.0 {
                self.toast = None;
            }
        }
        self.was_previewing = previewing;
    }

    /// The whole album: one zip in a browser, separate files in a folder on the desktop.
    pub fn download_all_tracks(&mut self) {
        if cfg!(target_arch = "wasm32") {
            self.start_soundtrack_zip();
        } else {
            self.queue_soundtrack(&Track::ALL);
        }
    }

    pub fn start_soundtrack_zip(&mut self) {
        self.ost_zip = Some(ZipWriter::new());
        self.ost_status.clear();
        self.queue_soundtrack(&Track::ALL);
    }

    /// Queues soundtrack files for rendering and saving.
    pub fn queue_soundtrack(&mut self, tracks: &[Track]) {
        for t in tracks {
            if !self.ost_queue.contains(t) && self.ost.as_ref().is_none_or(|r| r.track != *t) {
                self.ost_queue.push_back(*t);
            }
        }
    }

    /// Renders for a few milliseconds each frame, so the menus keep moving while files are made.
    fn advance_soundtrack_export(&mut self) {
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < 0.008 {
            if self.ost.is_none() {
                self.ost = self.ost_queue.pop_front().map(OstRender::new);
            }
            let Some(r) = &mut self.ost else { break };
            if r.advance(2048) {
                let (name, wav) = (r.file_name(), r.wav());
                self.ost = None;
                match &mut self.ost_zip {
                    Some(zip) => zip.add(&name, &wav),
                    None => {
                        self.ost_status = match crate::storage::deliver(&name, &wav) {
                            Ok(place) => format!("SAVED {name} TO {place}"),
                            Err(e) => format!("COULD NOT SAVE {name}: {e}"),
                        };
                    }
                }
                // Each file is a natural place to give the frame back.
                break;
            }
        }
        if self.ost.is_none() && self.ost_queue.is_empty() {
            if let Some(zip) = self.ost_zip.take() {
                const NAME: &str = "ROTFALL Soundtrack.zip";
                self.ost_status = match crate::storage::deliver(NAME, &zip.finish()) {
                    Ok(place) => format!("SAVED {NAME} TO {place}"),
                    Err(e) => format!("COULD NOT SAVE {NAME}: {e}"),
                };
            }
        }
    }

    /// Advances whichever kind of simulation this session runs.
    fn step_world(&mut self, dt: f32, input: PlayerInput) {
        // The moment of death runs in slow motion, if no other players are relying on the clock.
        let me = self.world.me();
        let dt = if !self.is_multiplayer() && !me.alive && me.death_t < 0.4 { dt * 0.3 } else { dt };
        match &mut self.session {
            Session::Solo => self.world.update(dt, &input),
            Session::Host(server) => {
                server.poll(&mut self.world);
                server.take_inputs(&mut self.inputs, &self.world);
                let local = self.world.local;
                self.inputs[local] = input;
                self.world.update_all(dt, &self.inputs);
                server.after_update(&mut self.world, dt);
            }
            Session::Client(client) => {
                client.poll(&mut self.world);
                if let ClientState::Closed(why) = &client.state {
                    let why = why.clone();
                    self.back_to_title(false);
                    self.page = Page::Multiplayer;
                    self.menu_error = Some(why);
                    return;
                }
                client.send_input(&input, &self.world, dt);
                net::update_client(&mut self.world, dt, &input);
                if let Some(n) = client.note.take() {
                    self.ui_note = Some((n, 1.5));
                }
            }
        }
        if std::mem::take(&mut self.world.autosave_due) && !self.autoplay {
            if self.save_game().is_ok() {
                self.ui_note = Some(("GAME SAVED".into(), 1.5));
            }
        }
    }

    /// Whichever player the camera should follow: yourself, or a teammate once you are out.
    fn camera_player(&self) -> &Player {
        let me = self.world.me();
        // Stay with the body for a moment, before moving on to whoever is left.
        if me.out && me.death_t > 4.0 {
            if let Some(p) = self.world.players.iter().find(|p| p.alive && p.connected) {
                return p;
            }
        }
        me
    }

    fn update_camera(&mut self, dt: f32, w: f32, h: f32) {
        // Death pulls in close on the body.
        let close = !self.world.me().alive && !(self.world.me().out && self.world.me().death_t > 4.0);
        let target_punch = if close { 0.58 } else { 1.0 };
        self.cam.punch += (target_punch - self.cam.punch) * (dt * if close { 1.6 } else { 3.0 }).min(1.0);
        let p = self.camera_player();
        // Look-ahead toward the cursor so that you see what you are aiming at.
        let mut target = p.pos;
        if p.alive {
            let to = vec3(p.aim.x - p.pos.x, 0.0, p.aim.z - p.pos.z);
            // Barely any look-ahead while building: the ground should hold still under the cursor.
            let lead = if self.build_sel.is_some() { 0.06 } else { 0.28 };
            target += to.clamp_length_max(14.0) * lead;
        }
        let shake = self.world.trauma;
        let time = self.world.time;
        self.cam.update(target, dt, w / h, shake, time);
    }

    /// Runs a shared-state request locally, or sends it to the server.
    pub fn do_cmd(&mut self, c: Command) -> Result<(), String> {
        match &mut self.session {
            Session::Client(client) => {
                client.send_command(c);
                Ok(())
            }
            _ => {
                let local = self.world.local;
                self.world.apply_command(local, c)
            }
        }
    }

    /// Hold the right button to swing the camera around; double click to put it back.
    fn camera_input(&mut self, dt: f32, raw: &RawInput) {
        self.rmb_clock += dt;
        let free = !(self.armory_open || self.help_open || self.panel_open || self.ui_over || self.touch.enabled);
        if raw.rmb_pressed && free {
            self.cam_drag = true;
            let again = self.rmb_press.is_some_and(|(at, m)| {
                self.rmb_clock - at < DOUBLE_CLICK && (m.0 - raw.mouse.0).abs() + (m.1 - raw.mouse.1).abs() < 12.0
            });
            if again {
                self.cam.yaw_home = true;
                self.rmb_press = None;
            } else {
                self.rmb_press = Some((self.rmb_clock, raw.mouse));
            }
        }
        if !raw.rmb {
            self.cam_drag = false;
        }
        let dx = raw.mouse.0 - self.rmb_mouse.0;
        self.rmb_mouse = raw.mouse;
        // A hand on a mouse is never perfectly still, and that shouldn't cancel a double click.
        if self.cam_drag && !raw.rmb_pressed && dx.abs() > 1.0 {
            self.cam.yaw -= dx * CAM_TURN;
            self.cam.yaw_home = false;
        }
    }

    fn play_input(&mut self, dt: f32, raw: &RawInput, w: f32, h: f32) {
        self.camera_input(dt, raw);
        let in_menu = self.armory_open || self.help_open;
        let (eye, ray) = self.cam.cursor_ray(raw.mouse.0, raw.mouse.1, w, h);
        let world_cursor = self.world.aim_point(eye, ray);
        let ground_cursor = self.cam.cursor_to_plane(raw.mouse.0, raw.mouse.1, w, h, 0.0);
        self.hover_tile = tile_of(Vec2::new(ground_cursor.x, ground_cursor.z));

        if raw.hit(K::Escape) {
            if self.armory_open || self.help_open || self.panel_open {
                self.armory_open = false;
                self.help_open = false;
                self.panel_open = false;
            } else if self.build_sel.is_some() {
                self.build_sel = None;
            } else {
                self.menu_sound(Sfx::MenuSelect, 0.7);
                self.screen = Screen::Paused;
            }
        }
        if raw.hit(K::Tab) {
            self.armory_open = !self.armory_open;
            self.help_open = false;
            self.build_sel = None;
            self.world.snd_ui(Sfx::Click, 0.5);
        }
        if raw.hit(K::KeyH) {
            self.help_open = !self.help_open;
            self.armory_open = false;
        }
        if raw.hit(K::KeyP) && self.is_multiplayer() {
            self.panel_open = !self.panel_open;
        }
        if raw.hit(K::KeyB) {
            self.armory_open = false;
            self.help_open = false;
            self.build_sel = if self.build_sel.is_some() { None } else { Some(self.default_build_slot()) };
            self.world.snd_ui(Sfx::Click, 0.5);
        }
        if raw.hit(K::Enter) {
            if let Err(e) = self.do_cmd(Command::NextWave) {
                self.note(&e);
            }
        }
        if raw.hit(K::F5) {
            match self.save_game() {
                Ok(()) => self.note("GAME SAVED"),
                Err(e) => self.note(&e),
            }
        }
        if raw.hit(K::F9) {
            if self.world.permadeath {
                self.note("PERMADEATH: THERE IS NO GOING BACK");
            } else if self.is_multiplayer() {
                self.note("QUICKLOAD IS FOR SINGLE PLAYER");
            } else {
                self.action = Some(Action::LoadLast);
            }
        }
        if raw.down.contains(&K::Minus) {
            self.cam.zoom = (self.cam.zoom + dt * 0.8).min(1.6);
        }
        if raw.down.contains(&K::Equal) {
            self.cam.zoom = (self.cam.zoom - dt * 0.8).max(0.65);
        }

        let mut mv = Vec2::ZERO;
        if raw.down.contains(&K::KeyW) { mv.y -= 1.0; }
        if raw.down.contains(&K::KeyS) { mv.y += 1.0; }
        if raw.down.contains(&K::KeyA) { mv.x -= 1.0; }
        if raw.down.contains(&K::KeyD) { mv.x += 1.0; }

        // Forward is up the screen, whichever way the camera has been turned.
        let turned = super::player::rotate_y(glam::vec3(mv.x, 0.0, mv.y), self.cam.yaw);
        let mut input = PlayerInput {
            move_dir: Vec2::new(turned.x, turned.z).normalize_or_zero(),
            aim: world_cursor,
            sprint: raw.down.contains(&K::ShiftLeft) || raw.down.contains(&K::ShiftRight),
            dodge: raw.hit(K::Space),
            grenade: raw.hit(K::KeyG) && !in_menu,
            reload: raw.hit(K::KeyR),
            toggle_light: raw.hit(K::KeyF),
            select: self.queued_select.take(),
            ..Default::default()
        };

        let tile = self.hover_tile;
        if raw.hit(K::KeyX) {
            if let Err(e) = self.do_cmd(Command::Sell(tile)) {
                self.note(&e);
            }
        }
        if raw.hit(K::KeyU) {
            if let Err(e) = self.do_cmd(Command::Upgrade(tile)) {
                self.note(&e);
            }
        }
        if raw.hit(K::KeyT) {
            if let Err(e) = self.do_cmd(Command::Repair(tile)) {
                self.note(&e);
            }
        }

        if let Some(sel) = self.build_sel {
            // Build mode.
            let mut sel = sel as i32;
            sel = (sel - raw.wheel).rem_euclid(BUILDABLE.len() as i32);
            for (i, k) in BUILD_KEYS.iter().enumerate() {
                if raw.hit(*k) {
                    sel = i as i32;
                }
            }
            self.build_sel = Some(sel as usize);
            if raw.rmb_pressed {
                self.build_sel = None;
                self.last_placed = None;
            } else if raw.lmb && !self.ui_over {
                let kind = BUILDABLE[sel as usize];
                // Dragging lays down runs of walls and traps; turrets are one per click.
                let drag_ok = sdef(kind).turret.is_none();
                let fresh = raw.lmb_pressed || (drag_ok && self.last_placed != Some(tile));
                if fresh {
                    match self.do_cmd(Command::Build(kind as u8, tile)) {
                        Ok(()) => self.last_placed = Some(tile),
                        Err(e) => {
                            if raw.lmb_pressed {
                                self.note(&e);
                                self.world.snd_ui(Sfx::Denied, 0.5);
                            }
                        }
                    }
                }
            } else if !raw.lmb {
                self.last_placed = None;
            }
        } else if !in_menu {
            input.fire = raw.lmb && !self.ui_over;
            input.cycle = -raw.wheel;
            if raw.hit(K::KeyQ) {
                input.cycle = -1;
            }
            if raw.hit(K::KeyE) {
                input.cycle = 1;
            }
            let owned: Vec<usize> = (0..WEAPONS.len()).filter(|&i| self.world.me().owned[i]).collect();
            for (i, k) in WEAPON_KEYS.iter().enumerate() {
                if raw.hit(*k) {
                    if let Some(&w) = owned.get(i) {
                        input.select = Some(w);
                    }
                }
            }
        }
        if self.touch_play() {
            let building = self.build_sel.is_some();
            let cursor_aim = input.aim;
            self.apply_touch(&mut input);
            if building {
                input.aim = cursor_aim;
                input.fire = false;
            }
        }
        self.step_world(dt, input);
    }

    pub fn note(&mut self, e: &str) {
        self.ui_note = Some((e.to_string(), 1.5));
    }

    fn default_build_slot(&self) -> usize {
        BUILDABLE.iter().position(|&k| k == SKind::SentryMg).unwrap_or(0)
    }

    pub fn draw(&mut self, frame: &mut FrameData, raw: &RawInput, size: (u32, u32)) {
        frame.begin();
        if self.screen == Screen::Title {
            let (w, h) = (size.0 as f32, size.1.max(1) as f32);
            frame.menu = Some(MenuView {
                time: self.menu_t,
                mouse: [raw.mouse.0 / w * 2.0 - 1.0, raw.mouse.1 / h * 2.0 - 1.0],
                logo: match (self.page, super::hud::gallery_rect(self.gallery_i, size)) {
                    (Page::Gallery, r) => r,
                    _ => super::hud::logo_rect(size),
                },
                dim: if self.page == Page::Gallery { 2.0 } else { 1.0 },
                gallery: (self.page == Page::Gallery).then_some(self.gallery_i),
            });
        } else {
            let ghost = self.build_ghost();
            view::draw_world(&mut self.world, &self.cam, frame, ghost);
        }
        super::hud::draw(self, frame, raw, size);
    }

    fn build_ghost(&self) -> Option<(SKind, (i32, i32), bool)> {
        if self.screen != Screen::Playing || self.ui_over {
            return None;
        }
        let sel = self.build_sel?;
        let kind = BUILDABLE[sel];
        let tile = self.hover_tile;
        let ok = self.world.can_place(kind, tile).is_ok() && self.world.scrap >= sdef(kind).cost;
        Some((kind, tile, ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::player::DeathCause;
    use crate::game::touch::{TouchEv, TouchPhase};
    use crate::render::FrameData;

    const SIZE: (u32, u32) = (1600, 900);

    /// Keeps the base's sentries out of a test about the player's own shots.
    fn hold_fire(d: &mut Driver) {
        for s in &mut d.app.world.structures {
            s.cooldown = 999.0;
        }
    }

    /// Fires at a point on a zombie for half a second, tracking it as it walks.
    fn shoot_at(d: &mut Driver, point: impl Fn(&crate::game::zombie::Zombie, f32) -> glam::Vec3) {
        d.raw.lmb = true;
        d.raw.lmb_pressed = true;
        for _ in 0..30 {
            hold_fire(d);
            let Some(z) = d.app.world.zombies.first() else { break };
            let at = point(z, d.app.world.time);
            d.raw.mouse = d.app.cam.world_to_screen(at, SIZE.0 as f32, SIZE.1 as f32).unwrap();
            d.step();
        }
        d.raw.lmb = false;
        d.raw.down.clear();
    }

    fn lone_zombie(d: &mut Driver, kind: crate::game::data::ZKind, off: Vec2) {
        d.app.world.zombies.clear();
        let at = d.app.world.me().pos2() + off;
        d.app.world.spawn_zombie(kind, at);
        d.app.world.zombies[0].spawn_t = 0.0;
        d.app.world.stats.headshots = 0;
        hold_fire(d);
        d.step();
    }

    #[test]
    fn hovering_over_a_giants_head_hits_it() {
        use crate::game::data::ZKind;
        // Off to one side, where projecting the cursor onto a flat plane used to aim well past it.
        for off in [Vec2::new(10.0, 5.0), Vec2::new(-11.0, -3.0), Vec2::new(3.0, -11.0)] {
            let mut d = started("giant_head");
            lone_zombie(&mut d, ZKind::Titan, off);
            let hp = d.app.world.zombies[0].hp;
            shoot_at(&mut d, |z, t| z.pose(t)[crate::ragdoll::HEAD]);
            assert!(d.app.world.zombies[0].hp < hp, "every shot missed from {off:?}");
            assert!(d.app.world.stats.headshots > 0, "hit, but not in the head, from {off:?}");
        }
    }

    #[test]
    fn body_shots_land_but_only_heads_are_critical() {
        use crate::game::data::ZKind;
        let mut d = started("body_shot");
        lone_zombie(&mut d, ZKind::Titan, Vec2::new(9.0, 4.0));
        let hp = d.app.world.zombies[0].hp;
        shoot_at(&mut d, |z, _| z.pos + glam::Vec3::Y * 1.0 * z.scale);
        assert!(d.app.world.zombies[0].hp < hp, "the body shots missed");
        assert_eq!(d.app.world.stats.headshots, 0, "a shot at the waist is no headshot");
    }

    #[test]
    fn shots_at_the_ground_stop_at_the_ground() {
        use crate::game::data::ZKind;
        let mut d = started("ground_shot");
        lone_zombie(&mut d, ZKind::Titan, Vec2::new(14.0, 0.0));
        let (me, hp) = (d.app.world.me().pos, d.app.world.zombies[0].hp);
        // Well short of the target: the bullets strike the ground on the way.
        d.raw.lmb = true;
        d.raw.lmb_pressed = true;
        for _ in 0..10 {
            hold_fire(&mut d);
            d.raw.mouse = d.app.cam.world_to_screen(me + glam::vec3(4.0, 0.0, 0.0), SIZE.0 as f32, SIZE.1 as f32).unwrap();
            d.step();
        }
        d.raw.lmb = false;
        assert_eq!(d.app.world.zombies[0].hp, hp, "the bullets should have stopped at the cursor");
        let aim = d.app.world.me().aim;
        assert!(aim.y.abs() < 0.01 && d.app.world.me().aim_dir().y < -0.1, "the gun should tip down, aim {aim}");
    }

    #[test]
    fn the_gun_points_exactly_at_the_cursor_from_any_side() {
        let mut d = started("exact_aim");
        for yaw in [0.0, 1.3, -2.4] {
            d.app.cam.yaw = yaw;
            d.run(40);
            let me = d.app.world.me().pos;
            let target = me + glam::vec3(6.0, 0.0, 3.0);
            d.raw.mouse = d.app.cam.world_to_screen(target, SIZE.0 as f32, SIZE.1 as f32).unwrap();
            d.step();
            let p = d.app.world.me();
            assert!(p.aim.distance(target) < 0.25, "aimed at {} not {target} with the camera at {yaw}", p.aim);
            // Following the shot from the muzzle lands where the cursor is.
            let (from, dir) = (p.muzzle(), p.shot_dir());
            let hit = from + dir * (-from.y / dir.y);
            assert!(hit.distance(p.aim) < 0.05, "shot lands at {hit}, aimed at {}", p.aim);
        }
    }

    #[test]
    fn right_mouse_turns_the_camera_and_a_double_click_resets_it() {
        let mut d = started("cam_turn");
        d.raw.mouse = (800.0, 450.0);
        d.step();
        // A plain click does nothing to it.
        d.raw.rmb = true;
        d.raw.rmb_pressed = true;
        d.step();
        d.raw.rmb = false;
        d.run(40);
        assert_eq!(d.app.cam.yaw, 0.0);
        // Holding and dragging does.
        d.raw.rmb = true;
        d.raw.rmb_pressed = true;
        d.step();
        for k in 1..=30 {
            d.raw.mouse = (800.0 + k as f32 * 8.0, 450.0);
            d.step();
        }
        d.raw.rmb = false;
        d.step();
        let turned = d.app.cam.yaw;
        assert!(turned.abs() > 0.5, "yaw {turned}");
        // Moving the mouse without the button doesn't.
        d.raw.mouse = (500.0, 450.0);
        d.run(5);
        assert_eq!(d.app.cam.yaw, turned);

        // Forward stays up the screen.
        d.run(30);
        let me = d.app.world.me().pos;
        let before = d.app.cam.world_to_screen(me, SIZE.0 as f32, SIZE.1 as f32).unwrap();
        let start = d.app.world.me().pos;
        d.raw.down.insert(K::KeyW);
        d.run(15);
        d.raw.down.clear();
        let moved = d.app.world.me().pos - start;
        let after = d.app.cam.world_to_screen(start + moved, SIZE.0 as f32, SIZE.1 as f32).unwrap();
        assert!(after.1 < before.1 - 5.0 && (after.0 - before.0).abs() < (before.1 - after.1) * 0.5, "W went from {before:?} to {after:?}");

        // Two quick clicks bring it home.
        for _ in 0..2 {
            d.raw.rmb = true;
            d.raw.rmb_pressed = true;
            d.step();
            d.raw.rmb = false;
            d.step();
        }
        d.run(60);
        assert_eq!(d.app.cam.yaw, 0.0, "should be back to the default view");

        // Two slow ones don't.
        d.app.cam.yaw = 1.0;
        for _ in 0..2 {
            d.raw.rmb = true;
            d.raw.rmb_pressed = true;
            d.step();
            d.raw.rmb = false;
            d.run(40);
        }
        assert_eq!(d.app.cam.yaw, 1.0);
    }

    #[test]
    fn right_mouse_leaves_the_camera_alone_in_menus() {
        let mut d = started("cam_in_menu");
        d.tap(K::Tab);
        assert!(d.app.armory_open);
        d.raw.mouse = (800.0, 450.0);
        d.raw.rmb = true;
        d.raw.rmb_pressed = true;
        d.step();
        d.raw.mouse = (1000.0, 450.0);
        d.run(5);
        d.raw.rmb = false;
        assert_eq!(d.app.cam.yaw, 0.0);
    }

    struct Driver {
        app: App,
        raw: RawInput,
        frame: FrameData,
    }

    /// Everything played since the last call, in order.
    fn played(rx: &std::sync::mpsc::Receiver<Cmd>) -> Vec<Sfx> {
        rx.try_iter().filter_map(|c| if let Cmd::Play { id, .. } = c { Some(id) } else { None }).collect()
    }

    #[test]
    fn menus_make_the_right_sounds_at_the_right_times() {
        let (audio, rx) = Audio::capture();
        let mut d = Driver { app: App::with_dir(5, audio, test_dir("menu_sounds")), raw: RawInput::default(), frame: FrameData::new() };
        let (w, h) = (SIZE.0 as f32, SIZE.1 as f32);
        d.raw.mouse = (w - 5.0, h * 0.5);
        d.run(3);
        played(&rx);

        // Find NEW GAME by walking the pointer down the column, listening for a hover.
        let x = crate::game::hud::logo_rect(SIZE)[0] + 40.0;
        let mut item = None;
        for y in (0..SIZE.1).step_by(4) {
            d.raw.mouse = (x, y as f32);
            d.step();
            if played(&rx).contains(&Sfx::MenuHover) {
                item = Some(y as f32 + 6.0);
                break;
            }
        }
        let y = item.expect("the pointer never crossed a menu item");

        // Within one control, or with the pointer still, there is nothing new to announce.
        d.raw.mouse = (x, y);
        d.step();
        played(&rx);
        d.raw.mouse = (x + 3.0, y);
        d.run(5);
        assert_eq!(played(&rx), vec![], "hover repeated on one control");

        // Pressing goes deeper. The next page's control under a still pointer stays quiet.
        d.click(x + 3.0, y);
        assert_eq!(d.app.page, Page::NewGame);
        assert_eq!(played(&rx), vec![Sfx::MenuSelect]);
        d.run(5);
        assert_eq!(played(&rx), vec![]);

        // Escape is the same gesture as a back button.
        d.tap(K::Escape);
        assert_eq!(d.app.page, Page::Main);
        assert_eq!(played(&rx), vec![Sfx::MenuBack]);

        // Starting a run gets the big one, and only that.
        d.app.page = Page::NewGame;
        d.step();
        played(&rx);
        let mut hit = None;
        for y in (0..SIZE.1).step_by(4) {
            d.raw.mouse = (x, y as f32);
            d.step();
            if played(&rx).contains(&Sfx::MenuHover) {
                hit = Some(y as f32 + 6.0);
                break;
            }
        }
        d.click(x, hit.expect("no item on the new game page"));
        d.step();
        let sounds = played(&rx);
        assert_eq!(d.app.screen, Screen::Playing);
        assert!(sounds.contains(&Sfx::MenuConfirm) && !sounds.contains(&Sfx::MenuSelect), "{sounds:?}");
    }

    impl Driver {
        fn new(name: &str) -> Self {
            Driver { app: App::with_dir(5, Audio::silent(), test_dir(name)), raw: RawInput::default(), frame: FrameData::new() }
        }
        /// Like `new`, but keeps what an earlier driver wrote.
        fn new_keep(name: &str) -> Self {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-saves").join(name);
            Driver { app: App::with_dir(5, Audio::silent(), dir), raw: RawInput::default(), frame: FrameData::new() }
        }
        fn step(&mut self) {
            self.app.update(1.0 / 60.0, &self.raw, SIZE);
            self.app.draw(&mut self.frame, &self.raw, SIZE);
            self.raw.end_frame();
            self.frame.decals.clear();
        }
        fn run(&mut self, frames: u32) {
            for _ in 0..frames {
                self.step();
            }
        }
        fn tap(&mut self, k: K) {
            self.raw.pressed.push(k);
            self.step();
        }
        fn click(&mut self, x: f32, y: f32) {
            self.raw.mouse = (x, y);
            self.step(); // Hover first, as a real mouse does.
            self.raw.lmb = true;
            self.raw.lmb_pressed = true;
            self.step();
            self.raw.lmb = false;
            self.step();
        }
        /// Puts the cursor over a world position.
        fn aim_at(&mut self, x: f32, z: f32) {
            let p = self.app.cam.world_to_screen(vec3(x, 0.0, z), SIZE.0 as f32, SIZE.1 as f32).unwrap();
            self.raw.mouse = p;
            self.step();
        }
    }

    /// A private, empty data directory: tests must never touch the player's real saves.
    fn test_dir(name: &str) -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-saves").join(name);
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn started(name: &str) -> Driver {
        let mut d = Driver::new(name);
        d.run(5);
        assert_eq!(d.app.screen, Screen::Title);
        d.tap(K::Enter);
        d.run(2);
        assert_eq!(d.app.screen, Screen::Playing);
        d.run(30);
        d
    }

    #[test]
    fn cursor_projection_round_trips() {
        let mut d = started("cursor_projection_round_trips");
        d.run(60);
        let p = vec3(7.0, 0.0, -3.0);
        let (x, y) = d.app.cam.world_to_screen(p, 1600.0, 900.0).unwrap();
        let q = d.app.cam.cursor_to_plane(x, y, 1600.0, 900.0, 0.0);
        assert!(p.distance(q) < 0.01, "{p} -> {q}");
    }

    #[test]
    fn keyboard_moves_the_player_the_way_the_screen_shows() {
        let mut d = started("keyboard_moves_the_player_the_way_the_screen_shows");
        let start = d.app.world.me().pos;
        d.raw.down.insert(K::KeyD);
        d.run(30);
        d.raw.down.clear();
        let after = d.app.world.me().pos;
        assert!(after.x > start.x + 1.0, "D should move right: {start} -> {after}");
        let (sx0, _) = d.app.cam.world_to_screen(start, 1600.0, 900.0).unwrap();
        let (sx1, _) = d.app.cam.world_to_screen(after, 1600.0, 900.0).unwrap();
        assert!(sx1 > sx0, "moving +X must be rightward on screen");
        let z0 = d.app.world.me().pos.z;
        d.raw.down.insert(K::KeyW);
        d.run(30);
        assert!(d.app.world.me().pos.z < z0 - 1.0, "W should move up the screen");
    }

    #[test]
    fn mouse_aims_and_fires_the_sidearm() {
        let mut d = started("mouse_aims_and_fires_the_sidearm");
        let p = d.app.world.me().pos;
        d.aim_at(p.x + 8.0, p.z);
        d.run(3);
        assert!(d.app.world.me().facing.x > 0.8, "facing {:?}", d.app.world.me().facing);
        let mag = d.app.world.me().mag[SIDEARM];
        d.raw.lmb = true;
        d.raw.lmb_pressed = true;
        d.run(30);
        d.raw.lmb = false;
        assert!(d.app.world.me().mag[SIDEARM] < mag, "holding LMB should fire");
    }

    #[test]
    fn build_mode_places_drags_and_sells_with_the_mouse() {
        let mut d = started("build_mode_places_drags_and_sells_with_the_mouse");
        d.tap(K::KeyB);
        assert!(d.app.build_sel.is_some());
        d.tap(K::Digit1); // Wood wall.
        assert_eq!(d.app.build_sel, Some(0));
        let before = d.app.world.structures.len();
        let scrap = d.app.world.scrap;
        for _ in 0..60 {
            d.aim_at(11.0, 1.0); // Let the camera settle.
        }
        d.raw.lmb = true;
        d.raw.lmb_pressed = true;
        d.step();
        // Drag across several tiles.
        for k in 0..4 {
            d.aim_at(11.0, 1.0 + k as f32 * 2.0);
        }
        d.raw.lmb = false;
        d.step();
        let built = d.app.world.structures.len() - before;
        assert!(built >= 3, "dragging should lay a run of walls, built {built}");
        assert!(d.app.world.scrap < scrap);
        // Shooting must be off while building.
        let mag = d.app.world.me().mag[SIDEARM];
        d.raw.lmb = true;
        d.raw.lmb_pressed = true;
        d.run(10);
        d.raw.lmb = false;
        assert_eq!(d.app.world.me().mag[SIDEARM], mag, "no gunfire in build mode");
        // Sell what the cursor is on.
        let n = d.app.world.structures.len();
        for _ in 0..60 {
            d.aim_at(11.0, 1.0);
        }
        d.tap(K::KeyX);
        assert_eq!(d.app.world.structures.len(), n - 1);
        d.raw.rmb_pressed = true;
        d.step();
        assert!(d.app.build_sel.is_none(), "RMB leaves build mode");
    }

    #[test]
    fn armory_buys_by_clicking() {
        let mut d = started("armory_buys_by_clicking");
        d.app.world.scrap = 900;
        d.tap(K::Tab);
        assert!(d.app.armory_open);
        // Second card in the first row is the SMG. Geometry mirrors hud::armory at 1600x900.
        let s = 1.5;
        let cw = (1600.0f32 - 80.0).min(1500.0 * s) / 3.0;
        let rows = 6.0;
        let total_h = 56.0 * s * rows + 112.0 * s;
        let x0 = (1600.0 - cw * 3.0) * 0.5;
        let y0 = (900.0 - total_h) * 0.5;
        d.click(x0 + cw * 1.3, y0 + 30.0 * s + 20.0);
        assert!(d.app.world.me().owned[1], "the SMG should have been bought");
        assert_eq!(d.app.world.me().weapon, 1, "and equipped");
        assert_eq!(d.app.world.scrap, 600);
        d.tap(K::Tab);
        assert!(!d.app.armory_open);
    }

    #[test]
    fn pause_freezes_the_game_and_menu_buttons_work() {
        let mut d = started("pause_freezes_the_game_and_menu_buttons_work");
        d.tap(K::Escape);
        assert_eq!(d.app.screen, Screen::Paused);
        let t = d.app.world.time;
        d.run(20);
        assert_eq!(d.app.world.time, t, "paused means paused");
        d.tap(K::Escape);
        assert_eq!(d.app.screen, Screen::Playing);
        d.run(2);
        assert!(d.app.world.time > t);
    }

    #[test]
    fn enter_calls_the_wave_and_game_over_can_restart() {
        let mut d = started("enter_calls_the_wave_and_game_over_can_restart");
        assert_eq!(d.app.world.wave, 0);
        d.tap(K::Enter);
        d.run(2);
        assert_eq!(d.app.world.wave, 1, "ENTER starts the wave early");
        let i = d.app.world.structures.iter().position(|s| s.kind == SKind::Core).unwrap();
        d.app.world.structures[i].hp = 0.0;
        d.run(220);
        assert_eq!(d.app.screen, Screen::GameOver);
        d.app.action = Some(Action::NewGame { permadeath: false });
        d.step();
        assert!(d.app.ground_dirty, "a new map needs its ground repainted");
        d.run(1);
        assert_eq!(d.app.screen, Screen::Playing);
        assert_eq!(d.app.world.wave, 0);
        assert!(!d.app.world.game_over);
    }

    #[test]
    fn the_title_screen_runs_no_simulation() {
        let mut d = Driver::new("title_idle");
        d.run(120);
        assert_eq!(d.app.screen, Screen::Title);
        assert_eq!(d.app.world.time, 0.0);
        assert!(d.frame.menu.is_some(), "the menu art should be requested");
    }

    #[test]
    fn saves_continue_and_quit_keep_the_run() {
        let mut d = started("save_continue");
        d.app.world.scrap = 1234;
        d.app.world.wave = 6;
        d.tap(K::F5);
        d.app.refresh_saves();
        assert_eq!(d.app.slot_info(Slot::Solo).unwrap().scrap, 1234);
        d.app.world.scrap = 1;
        d.tap(K::F9);
        d.run(2);
        assert_eq!(d.app.world.scrap, 1234, "quickload restores the save");
        assert_eq!(d.app.world.wave, 6);
        // Quitting with the window's close button saves too.
        d.app.world.scrap = 777;
        d.app.shutdown();
        let mut e = Driver::new_keep("save_continue");
        e.run(1);
        assert_eq!(e.app.slot_info(Slot::Solo).unwrap().scrap, 777);
        e.tap(K::Enter);
        e.run(2);
        assert_eq!(e.app.screen, Screen::Playing, "ENTER on the title continues");
        assert_eq!(e.app.world.scrap, 777);
    }

    #[test]
    fn a_wave_clear_autosaves() {
        let mut d = started("autosave");
        d.app.world.autosave_due = true;
        d.run(2);
        assert!(d.app.slot_info(Slot::Solo).is_some());
    }

    #[test]
    fn permadeath_ends_the_run_and_erases_the_save() {
        let mut d = Driver::new("perma");
        d.run(2);
        d.app.action = Some(Action::NewGame { permadeath: true });
        d.run(2);
        assert!(d.app.world.permadeath);
        d.tap(K::F5);
        d.app.refresh_saves();
        assert!(d.app.slot_info(Slot::Permadeath).is_some(), "a permadeath run can be suspended");
        // No loading your way out of trouble.
        d.tap(K::F9);
        d.run(2);
        assert!(d.app.ui_note.as_ref().unwrap().0.contains("NO GOING BACK"));
        d.app.world.scrap = 4242;
        d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO, crate::game::player::DeathCause::Regular);
        assert!(d.app.world.game_over, "one death ends it");
        d.run(60 * 6);
        assert_eq!(d.app.screen, Screen::GameOver);
        assert!(d.app.save_erased);
        assert!(d.app.slot_info(Slot::Permadeath).is_none(), "the save must be gone");
        assert!(!d.app.data_dir.join(Slot::Permadeath.file()).exists());
        // And the attempt is on the board.
        assert_eq!(d.app.records[0].mode, "PERMADEATH");
        let reloaded = load_records(&d.app.data_dir.join("records.txt"));
        assert_eq!(reloaded.len(), 1);
    }

    #[test]
    fn standard_mode_death_only_costs_time() {
        let mut d = started("standard_death");
        d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO, crate::game::player::DeathCause::Regular);
        assert!(!d.app.world.game_over);
        d.run(60 * 10);
        assert!(d.app.world.me().alive, "redeployed");
    }

    #[test]
    fn hosting_and_joining_through_the_app_layer() {
        let mut host = Driver::new("mp_host");
        host.app.settings.name = "HOSTY".into();
        host.app.settings.port = "0".into();
        host.app.action = Some(Action::StartHost);
        host.run(2);
        assert_eq!(host.app.screen, Screen::Playing, "{:?}", host.app.menu_error);
        assert!(host.app.world.hold_timer, "the match waits for the host");
        let port = match &host.app.session {
            Session::Host(s) => s.port,
            _ => panic!("should be hosting"),
        };
        let mut guest = Driver::new("mp_guest");
        guest.app.settings.name = "GUEST".into();
        guest.app.settings.address = format!("127.0.0.1:{port}");
        guest.app.action = Some(Action::Join);
        guest.run(1);
        for _ in 0..300 {
            guest.step();
            host.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
            if guest.app.screen == Screen::Playing && guest.app.world.players.len() >= 2 {
                break;
            }
        }
        assert_eq!(guest.app.screen, Screen::Playing, "{:?}", guest.app.menu_error);
        assert_eq!(guest.app.world.local, 1);
        assert_eq!(host.app.world.players[1].name, "GUEST");
        // A guest cannot start the match, but the host can.
        guest.tap(K::Enter);
        for _ in 0..20 {
            guest.step();
            host.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(host.app.world.hold_timer, "guests wait for the host");
        assert!(guest.app.ui_note.is_some(), "and are told why");
        host.tap(K::Enter);
        for _ in 0..30 {
            guest.step();
            host.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!host.app.world.hold_timer);
        assert_eq!(host.app.world.wave, 1);
        assert_eq!(guest.app.world.wave, 1, "the guest sees the wave start");
        // Armory purchases and building go through the server from a guest.
        host.app.world.scrap = 2000;
        for _ in 0..10 {
            guest.step();
            host.step();
        }
        guest.tap(K::KeyB);
        guest.tap(K::Digit1);
        for _ in 0..30 {
            guest.aim_at(15.0, 15.0);
            host.step();
        }
        guest.raw.lmb = true;
        guest.raw.lmb_pressed = true;
        guest.step();
        guest.raw.lmb = false;
        for _ in 0..30 {
            guest.step();
            host.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(host.app.world.structures.iter().any(|s| s.kind == SKind::WoodWall), "the guest's wall should exist on the server");
        assert!(guest.app.world.structures.iter().any(|s| s.kind == SKind::WoodWall), "and on the guest");
        // Saving is for the host.
        guest.tap(K::F5);
        assert!(guest.app.ui_note.as_ref().unwrap().0.contains("ONLY THE HOST"));
        host.tap(K::F5);
        host.app.refresh_saves();
        assert_eq!(host.app.slot_info(Slot::Host).unwrap().players, 2);
        // When the host leaves, the guest lands back on the menu with an explanation.
        drop(host);
        for _ in 0..50 {
            guest.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
            if guest.app.screen == Screen::Title {
                break;
            }
        }
        assert_eq!(guest.app.screen, Screen::Title);
        assert!(guest.app.menu_error.is_some());
    }

    #[test]
    fn host_errors_are_reported_not_fatal() {
        let mut d = Driver::new("bad_port");
        d.app.settings.port = "not a port".into();
        d.app.action = Some(Action::StartHost);
        d.run(2);
        assert_eq!(d.app.screen, Screen::Title);
        assert!(d.app.menu_error.as_ref().unwrap().contains("PORT"));
        // A port already taken by another server.
        let busy = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        d.app.settings.port = busy.local_addr().unwrap().port().to_string();
        d.app.action = Some(Action::StartHost);
        d.run(2);
        assert_eq!(d.app.screen, Screen::Title);
        assert!(d.app.menu_error.as_ref().unwrap().contains("IN USE"));
    }

    #[test]
    fn joining_a_dead_address_returns_to_the_menu() {
        let mut d = Driver::new("bad_join");
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        d.app.settings.address = format!("127.0.0.1:{free}");
        d.app.action = Some(Action::Join);
        for _ in 0..500 {
            d.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
            if d.app.page == Page::Join {
                break;
            }
        }
        assert_eq!(d.app.page, Page::Join);
        assert!(d.app.menu_error.is_some());
    }

    #[test]
    fn settings_survive_a_restart() {
        let mut d = Driver::new("settings");
        d.app.settings.name = "ZED".into();
        d.app.settings.music = 0.25;
        d.app.store_settings();
        let e = Driver::new_keep("settings");
        assert_eq!(e.app.settings.name, "ZED");
        assert_eq!(e.app.settings.music, 0.25);
    }

    fn touch(d: &mut Driver, id: u64, phase: TouchPhase, x: f32, y: f32) {
        d.raw.touches.push(TouchEv { id, phase, x, y });
    }

    /// The window layer's frame order: touches first, then the game.
    fn touch_step(d: &mut Driver) {
        d.app.begin_frame(&mut d.raw, SIZE);
        d.step();
    }

    #[test]
    fn touch_mode_turns_on_for_small_screens_and_asks() {
        let mut d = Driver::new("touch_detect");
        d.app.detect_device((1920.0, 1080.0), false, false);
        assert!(!d.app.touch.enabled, "a desktop stays a desktop");
        d.app.detect_device((1920.0, 1080.0), true, false);
        assert!(!d.app.touch.enabled, "a big touch laptop stays a desktop too");
        d.app.detect_device((390.0, 844.0), true, false);
        assert!(d.app.touch.enabled && d.app.touch.prompt, "a phone gets controls and the notice");
        d.app.answer_touch_prompt(false);
        assert!(!d.app.touch.enabled && !d.app.touch.prompt);
        // Disable sticks across restarts.
        let mut e = Driver::new_keep("touch_detect");
        e.app.detect_device((390.0, 844.0), true, false);
        assert!(!e.app.touch.enabled && !e.app.touch.prompt, "disabled means disabled");
        e.app.settings.touch = "auto".into();
        e.app.detect_device((800.0, 1200.0), true, false);
        assert!(e.app.touch.enabled, "a tablet counts");
        e.app.answer_touch_prompt(true);
        let mut f = Driver::new_keep("touch_detect");
        f.app.detect_device((390.0, 844.0), true, false);
        assert!(f.app.touch.enabled && !f.app.touch.prompt, "Ok is only asked once");
    }

    #[test]
    fn sticks_move_aim_and_fire() {
        let mut d = started("touch_play");
        d.app.touch.enabled = true;
        let start = d.app.world.me().pos;
        touch(&mut d, 1, TouchPhase::Start, 400.0, 600.0);
        touch_step(&mut d);
        touch(&mut d, 1, TouchPhase::Move, 600.0, 600.0);
        for _ in 0..30 {
            touch_step(&mut d);
        }
        assert!(d.app.world.me().pos.x > start.x + 1.0, "the left stick walks");
        let mag = d.app.world.me().mag[SIDEARM];
        touch(&mut d, 2, TouchPhase::Start, 1200.0, 600.0);
        touch_step(&mut d);
        touch(&mut d, 2, TouchPhase::Move, 1200.0, 450.0);
        for _ in 0..40 {
            touch_step(&mut d);
        }
        assert!(d.app.world.me().mag[SIDEARM] < mag, "pushing the right stick fires");
        assert!(d.app.world.me().facing.y < -0.8, "up on the stick aims up the screen: {:?}", d.app.world.me().facing);
        touch(&mut d, 2, TouchPhase::End, 1200.0, 450.0);
        touch(&mut d, 1, TouchPhase::End, 600.0, 600.0);
        touch_step(&mut d);
        assert!(d.app.touch.left.is_none() && d.app.touch.right.is_none());
    }

    #[test]
    fn on_screen_buttons_press_keys_and_menus_take_taps() {
        let mut d = started("touch_buttons");
        d.app.touch.enabled = true;
        let b = crate::game::touch::buttons(&d.app, SIZE);
        let armory = b.iter().find(|x| x.label == "ARMORY").unwrap().rect;
        touch(&mut d, 7, TouchPhase::Start, armory[0] + 5.0, armory[1] + 5.0);
        touch(&mut d, 7, TouchPhase::End, armory[0] + 5.0, armory[1] + 5.0);
        touch_step(&mut d);
        assert!(d.app.armory_open, "the ARMORY button opens the armory");
        assert!(d.app.touch.left.is_none(), "and it is not also a stick");
        // In a menu, a quick tap in a single frame still lands as a click where it was aimed.
        let s = 1.5;
        let cw = (1600.0f32 - 80.0).min(1500.0 * s) / 3.0;
        let total_h = 56.0 * s * 6.0 + 112.0 * s;
        let (x0, y0) = ((1600.0 - cw * 3.0) * 0.5, (900.0 - total_h) * 0.5);
        d.app.world.scrap = 900;
        let (x, y) = (x0 + cw * 1.3, y0 + 30.0 * s + 20.0);
        touch(&mut d, 8, TouchPhase::Start, x, y);
        touch(&mut d, 8, TouchPhase::End, x, y);
        for _ in 0..4 {
            touch_step(&mut d);
        }
        assert!(d.app.world.me().owned[1], "a tap buys the gun under the finger");
    }

    #[test]
    fn touch_buttons_stay_clear_of_the_hud_on_small_screens() {
        let mut d = started("touch_layout");
        d.app.touch.enabled = true;
        for size in [(948u32, 375u32), (1280, 590), (1600, 900), (2400, 1080)] {
            let s = crate::game::hud::ui_scale(&d.app, size);
            let (w, h) = (size.0 as f32, size.1 as f32);
            // The weapon panel and vitals along the bottom, and the build bar.
            let panel_top = h - 14.0 * s - 62.0 * s - 24.0 * s;
            let bar_top = h - 200.0 * s;
            d.app.build_sel = None;
            for b in crate::game::touch::buttons(&d.app, size) {
                let [x, y, bw, bh] = b.rect;
                assert!(x >= 0.0 && x + bw <= w && y >= 0.0, "{} is off a {size:?} screen", b.label);
                assert!(y + bh <= panel_top, "{} overlaps the bottom panels on {size:?}: {} vs {panel_top}", b.label, y + bh);
            }
            d.app.build_sel = Some(0);
            for b in crate::game::touch::buttons(&d.app, size) {
                let [x, y, bw, bh] = b.rect;
                assert!(x + bw <= w, "{} is off a {size:?} screen", b.label);
                assert!(y + bh <= bar_top, "{} overlaps the build bar on {size:?}", b.label);
            }
        }
    }

    #[test]
    fn soundtrack_page_cuts_and_stop_cuts_back() {
        let (audio, rx) = Audio::capture();
        let mut d = Driver { app: App::with_dir(5, audio, test_dir("preview_cut")), raw: RawInput::default(), frame: FrameData::new() };
        d.run(3);
        let _ = rx.try_iter().count();
        d.app.page = Page::Soundtrack;
        d.app.preview = Some(Track::Boss);
        d.step();
        assert!(rx.try_iter().any(|c| matches!(c, Cmd::MusicCut(Track::Boss))), "play should cut");
        d.app.preview = Some(Track::Rot);
        d.step();
        assert!(rx.try_iter().any(|c| matches!(c, Cmd::MusicCut(Track::Rot))), "changing tracks should cut");
        d.app.preview = None;
        d.step();
        assert!(rx.try_iter().any(|c| matches!(c, Cmd::MusicCut(Track::Menu))), "stop should cut");
    }

    /// Frames until a blast's victim is back on their feet, pressing Space this often.
    fn frames_down(name: &str, every: Option<u32>) -> u32 {
        let mut d = started(name);
        let at = d.app.world.me().pos;
        d.app.world.knock_down(0, at + glam::Vec3::X, 26.0);
        let mut frames = 0;
        while d.app.world.me().down_t > 0.0 && frames < 600 {
            if every.is_some_and(|e| frames % e == 0) {
                d.raw.pressed.push(K::Space);
            }
            d.step();
            frames += 1;
        }
        assert!(d.app.world.me().alive);
        frames
    }

    #[test]
    fn mashing_space_gets_you_up_faster_but_not_instantly() {
        let lying = frames_down("mash_none", None);
        let mashing = frames_down("mash_fast", Some(8));
        let frantic = frames_down("mash_frantic", Some(1));
        println!("frames down: {lying} still, {mashing} at 7.5 presses/s, {frantic} on every frame");
        assert!(mashing * 2 < lying, "mashing should at least halve it: {mashing} vs {lying}");
        // The blast still has to land: nobody gets to skip being knocked over.
        assert!(frantic as f32 / 60.0 >= 0.55, "{frantic} frames is no knockdown at all");
    }

    #[test]
    fn holding_space_does_not_count_as_mashing() {
        let mut d = started("mash_hold");
        let at = d.app.world.me().pos;
        d.app.world.knock_down(0, at + glam::Vec3::X, 26.0);
        let start = d.app.world.me().down_t;
        d.raw.down.insert(K::Space);
        d.run(30);
        d.raw.down.clear();
        assert!((start - 0.5 - d.app.world.me().down_t).abs() < 0.05, "only time should have passed");
    }

    #[test]
    fn a_blast_knocks_you_down_helpless_until_you_get_up() {
        let mut d = started("knockdown");
        let at = d.app.world.me().pos;
        let hp = d.app.world.me().hp;
        d.app.world.explosions.push(crate::game::combat::Explosion { pos: at + glam::Vec3::X, radius: 7.0, damage: 60.0, from_enemy: false, force: 20.0, spares_players: false, barrel: false });
        d.step();
        assert!(d.app.world.me().down_t > 0.0, "thrown down");
        assert!(d.app.world.me().alive);
        assert!(d.app.world.ragdolls.bodies.iter().any(|b| b.owner == 0), "as a ragdoll");
        // No moving or shooting from the ground.
        let mag = d.app.world.me().mag[SIDEARM];
        let before = d.app.world.me().pos;
        d.raw.down.insert(K::KeyD);
        d.raw.lmb = true;
        d.run(30);
        d.raw.down.clear();
        d.raw.lmb = false;
        assert_eq!(d.app.world.me().mag[SIDEARM], mag, "no shooting while down");
        assert!(d.app.world.me().pos.distance(before) < 4.0);
        // Hits land at double strength.
        let was = d.app.world.me().hp;
        d.app.world.hurt_player(0, 5.0, at, DeathCause::Regular);
        assert!((was - d.app.world.me().hp - 10.0).abs() < 0.01, "helpless on the ground");
        assert!(hp > was || hp == was);
        // And eventually back up, with the body gone and a stand-up animation first.
        d.run(60 * 4);
        assert_eq!(d.app.world.me().down_t, 0.0);
        assert!(d.app.world.ragdolls.bodies.iter().all(|b| b.owner != 0));
        assert!(d.app.world.me().alive);
        let start = d.app.world.me().pos;
        d.raw.down.insert(K::KeyD);
        d.run(40);
        d.raw.down.clear();
        assert!(d.app.world.me().pos.x > start.x + 0.5, "free to move again");
    }

    #[test]
    fn explosive_barrels_ragdoll_the_player_across_their_whole_blast() {
        // Near the middle of the blast, at its edge, and just outside it.
        for (distance, thrown) in [(1.5f32, true), (4.0, true), (6.5, true), (8.5, false)] {
            let mut d = started("barrel_knock");
            d.app.world.barrels.clear();
            let at = d.app.world.me().pos + glam::Vec3::X * distance;
            d.app.world.barrels.push(crate::game::Barrel { pos: at, hp: 0.0, burn: 0.0 });
            d.step();
            assert!(d.app.world.me().alive, "a barrel alone should not kill from full health at {distance} m");
            let down = d.app.world.me().down_t > 0.0;
            assert_eq!(down, thrown, "at {distance} m");
            assert_eq!(d.app.world.ragdolls.bodies.iter().any(|b| b.owner == 0), thrown, "ragdoll at {distance} m");
        }
    }

    #[test]
    fn each_kind_of_death_has_its_track() {
        let cases = [
            (DeathCause::Regular, Track::Rot),
            (DeathCause::Zombie, Track::OneOfMany),
            (DeathCause::Explosion, Track::Smitherines),
        ];
        for (cause, track) in cases {
            let mut d = started("death_tracks");
            assert_eq!(d.app.music_now, Some(Track::Build));
            d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO, cause);
            d.run(2);
            assert_eq!(d.app.music_now, Some(track), "{cause:?}");
            assert_eq!(d.app.toast.as_ref().map(|t| t.0), Some(track), "the name card should announce it");
            d.run(60 * 12);
            assert!(d.app.world.me().alive);
            assert_eq!(d.app.music_now, Some(Track::Build), "back to the music after redeploying");
        }
    }

    #[test]
    fn permadeath_only_ever_plays_its_own_death_track() {
        for cause in [DeathCause::Regular, DeathCause::Zombie, DeathCause::Explosion] {
            let mut d = Driver::new("perma_track");
            d.run(1);
            d.app.action = Some(Action::NewGame { permadeath: true });
            d.run(2);
            d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO, cause);
            d.run(2);
            assert_eq!(d.app.music_now, Some(Track::RestWell), "{cause:?}");
        }
    }

    #[test]
    fn killed_by_zombies_your_body_rises_when_you_respawn() {
        let mut d = started("rising");
        d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO, DeathCause::Zombie);
        assert_eq!(d.app.world.zombies.len(), 0);
        let bodies = d.app.world.ragdolls.bodies.len();
        assert!(bodies >= 1);
        // Look at the moment of respawn: left alone, the base's sentries soon shoot it down.
        while !d.app.world.me().alive {
            d.step();
        }
        let risen: Vec<_> = d.app.world.zombies.iter().filter(|z| z.style >> 24 == 8).collect();
        assert_eq!(risen.len(), 1, "the old body should be up and walking");
        assert!(risen[0].kind == ZKind::Walker);
        assert!(d.app.world.ragdolls.bodies.iter().all(|b| b.owner != 0), "and no longer a corpse");
        // Any other death leaves the body where it fell.
        let mut e = started("no_rising");
        e.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO, DeathCause::Regular);
        e.run(60 * 12);
        assert!(e.app.world.me().alive);
        assert!(e.app.world.zombies.iter().all(|z| z.style >> 24 != 8));
    }

    #[test]
    fn the_title_screen_names_its_music_and_exports_the_soundtrack() {
        let mut d = Driver::new("ost");
        d.run(2);
        assert_eq!(d.app.music_now, Some(Track::Menu));
        assert_eq!(d.app.toast.as_ref().map(|t| t.0.name()), Some("Descent"));
        d.app.page = Page::Soundtrack;
        d.app.preview = Some(Track::Boss);
        d.run(2);
        assert_eq!(d.app.music_now, Some(Track::Boss), "auditioning a track plays it");
        d.app.page = Page::Extras;
        d.run(2);
        assert_eq!(d.app.music_now, Some(Track::Menu), "leaving the list stops it");
    }

    #[test]
    fn the_album_zip_collects_every_track_while_the_menu_keeps_running() {
        let mut d = Driver::new("zip_queue");
        d.run(1);
        d.app.page = Page::Soundtrack;
        d.app.start_soundtrack_zip();
        assert_eq!(d.app.ost_queue.len(), Track::ALL.len());
        d.run(3);
        assert!(d.app.ost.is_some() || d.app.ost_queue.len() < Track::ALL.len(), "work has started");
        assert!(d.app.ost_zip.is_some(), "and is going into the archive");
        assert_eq!(d.app.screen, Screen::Title);
    }

    #[test]
    fn every_menu_page_draws_without_panicking() {
        let mut d = Driver::new("pages");
        d.app.lan = None;
        for p in [Page::Main, Page::NewGame, Page::Multiplayer, Page::Host, Page::Join, Page::Records, Page::Options, Page::Manual, Page::Extras, Page::Gallery, Page::Soundtrack] {
            d.app.page = p;
            d.raw.mouse = (300.0, 400.0);
            d.run(3);
            assert_eq!(d.app.page, p, "page {p:?} changed by itself");
        }
    }
}
