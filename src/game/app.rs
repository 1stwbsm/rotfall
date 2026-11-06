//! Application layer: screens, sessions, input mapping, camera. Independent of the window
//! system, so the same code runs in the game window and in headless capture mode.
use super::bot::Bot;
use super::data::*;
use super::net::{self, Client, ClientState, Command, LanBrowser, Server, ServerConfig};
use super::player::{Player, PlayerInput};
use super::save::{self, SaveInfo, Slot};
use super::view::{self, Camera};
use super::{tile_of, World};
use crate::audio::{Audio, Cmd, Sfx, Track};
use crate::render::{FrameData, MenuView};
use glam::{vec3, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;
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
        }
    }
}

impl Settings {
    fn load(path: &std::path::Path) -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(path) else { return s };
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
                _ => {}
            }
        }
        s
    }

    fn save(&self, path: &std::path::Path) {
        let text = format!(
            "name={}\naddress={}\nserver_name={}\nport={}\nmax_players={}\nhost_permadeath={}\nmusic={}\nsfx={}\nfullscreen={}\n",
            self.name, self.address, self.server_name, self.port, self.max_players,
            u8::from(self.host_permadeath), self.music, self.sfx, u8::from(self.fullscreen)
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
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
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
}

const WEAPON_KEYS: [K; 9] = [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9];
const BUILD_KEYS: [K; 10] = [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9, K::Digit0];
pub const SLOTS: [Slot; 4] = [Slot::Solo, Slot::Permadeath, Slot::Host, Slot::HostPermadeath];

impl App {
    pub fn new(seed: u64, audio: Audio) -> App {
        Self::with_dir(seed, audio, save::data_dir())
    }

    /// `dir` holds saves, settings and records.
    pub fn with_dir(seed: u64, audio: Audio, dir: PathBuf) -> App {
        let settings = Settings::load(&dir.join("config.txt"));
        audio.send(Cmd::Music(Track::Menu));
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
            let _ = std::fs::remove_file(self.data_dir.join(Slot::Permadeath.file()));
            self.refresh_saves();
            self.world.announce("PERMADEATH - ONE LIFE", [1.0, 0.25, 0.15], 4.0, true);
        }
    }

    fn player_name(&self) -> String {
        let n: String = self.settings.name.chars().take(16).collect::<String>().trim().to_ascii_uppercase();
        if n.is_empty() { "SURVIVOR".to_string() } else { n }
    }

    fn load_world(&mut self, slot: Slot) -> Result<World, String> {
        let bytes = std::fs::read(self.data_dir.join(slot.file())).map_err(|_| "NO SAVED GAME".to_string())?;
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
        self.audio.send(Cmd::Music(Track::Menu));
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
                        _ => Page::Main,
                    };
                    self.focus = None;
                    self.menu_error = None;
                    self.store_settings();
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
        if self.screen != Screen::Title {
            if self.world.game_over {
                self.on_game_over();
                if self.screen == Screen::Playing && self.world.game_over_timer > 1.5 {
                    self.screen = Screen::GameOver;
                    self.armory_open = false;
                    self.build_sel = None;
                    self.panel_open = false;
                }
            }
            self.update_camera(if self.screen == Screen::Paused && !self.is_multiplayer() { 0.0 } else { dt }, w, h);
            self.world.listener = self.cam.focus;
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
                let _ = std::fs::remove_file(self.data_dir.join(slot.file()));
                self.save_erased = true;
            }
            self.refresh_saves();
        }
    }

    /// Advances whichever kind of simulation this session runs.
    fn step_world(&mut self, dt: f32, input: PlayerInput) {
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
        if me.out {
            if let Some(p) = self.world.players.iter().find(|p| p.alive && p.connected) {
                return p;
            }
        }
        me
    }

    fn update_camera(&mut self, dt: f32, w: f32, h: f32) {
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

    fn play_input(&mut self, dt: f32, raw: &RawInput, w: f32, h: f32) {
        let in_menu = self.armory_open || self.help_open;
        let world_cursor = self.cam.cursor_to_plane(raw.mouse.0, raw.mouse.1, w, h, 1.2);
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

        let mut input = PlayerInput {
            move_dir: mv.normalize_or_zero(),
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
                logo: super::hud::logo_rect(size),
                dim: 1.0,
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
    use crate::render::FrameData;

    const SIZE: (u32, u32) = (1600, 900);

    struct Driver {
        app: App,
        raw: RawInput,
        frame: FrameData,
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
        d.run(150);
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
        d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO);
        assert!(d.app.world.game_over, "one death ends it");
        d.run(120);
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
        d.app.world.hurt_player(0, 1000.0, glam::Vec3::ZERO);
        assert!(!d.app.world.game_over);
        d.run(60 * 9);
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

    #[test]
    fn every_menu_page_draws_without_panicking() {
        let mut d = Driver::new("pages");
        d.app.lan = None;
        for p in [Page::Main, Page::NewGame, Page::Multiplayer, Page::Host, Page::Join, Page::Records, Page::Options, Page::Manual] {
            d.app.page = p;
            d.raw.mouse = (300.0, 400.0);
            d.run(3);
            assert_eq!(d.app.page, p, "page {p:?} changed by itself");
        }
    }
}
