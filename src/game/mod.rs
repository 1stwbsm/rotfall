//! The simulation: world state and the per-frame update order.
pub mod app;
pub mod bot;
pub mod combat;
pub mod data;
pub mod fx;
pub mod hud;
pub mod map;
pub mod nav;
pub mod net;
pub mod player;
pub mod save;
pub mod structures;
pub mod touch;
pub mod view;
pub mod zombie;

use crate::audio::{Audio, Cmd, Sfx, Track};
use crate::ragdoll::RagdollWorld;
use crate::util::Rng;
use combat::{Explosion, Hazard, Projectile};
use data::*;
use fx::{Ev, Fx};
use glam::{vec3, Vec2, Vec3};
use map::{Map, WorldCollider, HALF};
use nav::Nav;
use player::Player;
use structures::Structure;
use zombie::Zombie;

pub const TILE: f32 = 2.0;
pub const TILE_DIM: i32 = (2.0 * HALF / TILE) as i32;

pub fn tile_of(p: Vec2) -> (i32, i32) {
    (((p.x + HALF) / TILE).floor() as i32, ((p.y + HALF) / TILE).floor() as i32)
}

pub fn tile_index(t: (i32, i32)) -> Option<usize> {
    if t.0 < 0 || t.1 < 0 || t.0 >= TILE_DIM || t.1 >= TILE_DIM {
        None
    } else {
        Some((t.1 * TILE_DIM + t.0) as usize)
    }
}

pub fn tile_min(t: (i32, i32)) -> Vec2 {
    Vec2::new(t.0 as f32 * TILE - HALF, t.1 as f32 * TILE - HALF)
}

pub fn tile_center(t: (i32, i32)) -> Vec2 {
    tile_min(t) + Vec2::splat(TILE * 0.5)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Build { timer: f32 },
    Assault,
}

pub struct Pickup {
    pub kind: u8, // 0 scrap, 1 ammo, 2 med
    pub pos: Vec3,
    pub vel: Vec3,
    pub value: u32,
    pub ttl: f32,
    pub spin: f32,
}

pub struct Barrel {
    pub pos: Vec3,
    pub hp: f32,
    pub burn: f32,
}

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub kills: u32,
    pub headshots: u32,
    pub best_combo: u32,
    pub built: u32,
    pub wave_reached: u32,
    pub scrap_earned: u32,
    pub deaths: u32,
    pub time: f32,
}

pub struct Message {
    pub text: String,
    pub color: [f32; 3],
    pub ttl: f32,
    pub big: bool,
}

pub struct World {
    pub map: Map,
    pub nav: Nav,
    pub ragdolls: RagdollWorld,
    pub fx: Fx,
    pub rng: Rng,
    pub audio: Audio,
    pub time: f32,
    pub players: Vec<Player>,
    /// Which player this machine controls and draws the HUD for.
    pub local: usize,
    pub seed: u64,
    pub permadeath: bool,
    /// The wave timer is frozen until the host starts the match.
    pub hold_timer: bool,
    /// Set at every wave clear; whoever owns the world writes the autosave.
    pub autosave_due: bool,
    /// Dead-reckoned state for the client side of a network game.
    pub is_client: bool,
    pub zombies: Vec<Zombie>,
    pub structures: Vec<Structure>,
    /// Structure index per tile, -1 when empty.
    pub tiles: Vec<i32>,
    pub blocked_tiles: Vec<bool>,
    pub projectiles: Vec<Projectile>,
    pub hazards: Vec<Hazard>,
    pub pickups: Vec<Pickup>,
    pub barrels: Vec<Barrel>,
    pub explosions: Vec<Explosion>,
    pub wave: u32,
    pub phase: Phase,
    pub spawn_budget: f32,
    pub spawn_timer: f32,
    pub spawn_dirs: Vec<Vec2>,
    pub bosses_left: u32,
    pub scrap: u32,
    pub stats: Stats,
    pub trauma: f32,
    pub combo: u32,
    pub combo_timer: f32,
    pub messages: Vec<Message>,
    pub night: f32,
    pub game_over: bool,
    pub game_over_timer: f32,
    pub core_alert: f32,
    pub listener: Vec3,
    pub nav_dirty: bool,
    pub nav_timer: f32,
    pub next_zombie_id: u32,
    pub boss_track: bool,
    pub groan_timer: f32,
    sfx_budget: [u8; crate::audio::sfx::SFX_COUNT],
    pub zhash_head: Vec<i32>,
    pub zhash_next: Vec<i32>,
    pub hit_scratch: Vec<(f32, u32, f32)>,
    pub query_scratch: Vec<u32>,
    pub invincible: bool,
    pub sfx_gain: f32,
}

pub const ZHASH_CELL: f32 = 2.0;
pub const ZHASH_DIM: usize = (2.0 * HALF / ZHASH_CELL) as usize + 2;

#[inline]
pub fn zhash_cell(p: Vec2) -> usize {
    let cx = ((p.x + HALF) / ZHASH_CELL).clamp(0.0, (ZHASH_DIM - 1) as f32) as usize;
    let cz = ((p.y + HALF) / ZHASH_CELL).clamp(0.0, (ZHASH_DIM - 1) as f32) as usize;
    cz * ZHASH_DIM + cx
}

/// Total spawn cost of a wave. A walker costs 1, so this is roughly the head count.
pub fn wave_budget(n: f32) -> f32 {
    35.0 + 24.0 * n + 3.2 * n * n
}

pub const FIRST_BUILD_TIME: f32 = 45.0;
pub const BUILD_TIME: f32 = 30.0;

impl World {
    pub fn new(seed: u64, audio: Audio) -> World {
        let map = Map::generate(seed);
        let nav = Nav::new(&map);
        let barrels = map.barrels.iter().map(|&pos| Barrel { pos, hp: 30.0, burn: 0.0 }).collect();
        let tile_count = (TILE_DIM * TILE_DIM) as usize;
        let mut w = World {
            map,
            nav,
            ragdolls: RagdollWorld::new(HALF, 460),
            fx: Fx::new(seed ^ 0xABCDEF),
            rng: Rng::new(seed ^ 0x1234567),
            audio,
            time: 0.0,
            players: vec![Player::new("PLAYER", 0)],
            local: 0,
            seed,
            permadeath: false,
            hold_timer: false,
            autosave_due: false,
            is_client: false,
            zombies: Vec::with_capacity(800),
            structures: Vec::with_capacity(256),
            tiles: vec![-1; tile_count],
            blocked_tiles: vec![false; tile_count],
            projectiles: Vec::with_capacity(128),
            hazards: Vec::with_capacity(64),
            pickups: Vec::with_capacity(256),
            barrels,
            explosions: Vec::with_capacity(16),
            wave: 0,
            phase: Phase::Build { timer: FIRST_BUILD_TIME },
            spawn_budget: 0.0,
            spawn_timer: 0.0,
            spawn_dirs: Vec::new(),
            bosses_left: 0,
            scrap: 700,
            stats: Stats::default(),
            trauma: 0.0,
            combo: 0,
            combo_timer: 0.0,
            messages: Vec::new(),
            night: 0.25,
            game_over: false,
            game_over_timer: 0.0,
            core_alert: 0.0,
            listener: Vec3::ZERO,
            nav_dirty: true,
            nav_timer: 0.0,
            next_zombie_id: 1,
            boss_track: false,
            groan_timer: 1.0,
            sfx_budget: [0; crate::audio::sfx::SFX_COUNT],
            zhash_head: vec![-1; ZHASH_DIM * ZHASH_DIM],
            zhash_next: Vec::with_capacity(800),
            hit_scratch: Vec::with_capacity(64),
            query_scratch: Vec::with_capacity(64),
            invincible: false,
            sfx_gain: 1.0,
        };
        w.place_starting_base();
        w.announce("FORTIFY THE REACTOR", [1.0, 0.8, 0.3], 4.0, true);
        w
    }

    fn place_starting_base(&mut self) {
        self.add_structure_at_center(SKind::Core);
        // Two sentries to survive the first minute without touching the menus.
        for at in [vec3(-6.0, 0.0, 6.0), vec3(6.0, 0.0, -6.0)] {
            let t = tile_of(Vec2::new(at.x, at.z));
            self.add_structure(SKind::SentryMg, t);
        }
    }

    pub fn snd(&mut self, id: Sfx, pos: Vec3, gain: f32) {
        self.fx.record(|| Ev::Snd { id: id as u8, pos, gain });
        let d = pos - self.listener;
        let dist2 = d.x * d.x + d.z * d.z;
        let att = 1.0 / (1.0 + dist2 / 360.0);
        if att < 0.04 {
            return;
        }
        if self.sfx_budget[id as usize] >= 3 {
            return;
        }
        self.sfx_budget[id as usize] += 1;
        let pan = (d.x / 32.0).clamp(-0.85, 0.85);
        let pitch = self.rng.range(0.93, 1.07);
        self.audio.send(Cmd::Play { id, gain: gain * att * self.sfx_gain, pan, pitch });
    }

    pub fn snd_ui(&mut self, id: Sfx, gain: f32) {
        self.fx.record(|| Ev::SndUi { id: id as u8, gain });
        self.audio.send(Cmd::Play { id, gain, pan: 0.0, pitch: 1.0 });
    }

    pub fn announce(&mut self, text: &str, color: [f32; 3], ttl: f32, big: bool) {
        self.fx.record(|| Ev::Msg { text: text.to_string(), color, ttl, big });
        if big {
            self.messages.retain(|m| !m.big);
        }
        self.messages.push(Message { text: text.to_string(), color, ttl, big });
        if self.messages.len() > 6 {
            self.messages.remove(0);
        }
    }

    /// Throws ragdolls around, here and on every client.
    pub fn blast(&mut self, pos: Vec3, radius: f32, force: f32) {
        self.fx.record(|| Ev::Blast { pos, radius, force });
        self.ragdolls.blast(pos, radius, force);
    }

    /// The player this machine draws the HUD and camera for.
    #[inline]
    pub fn me(&self) -> &Player {
        &self.players[self.local]
    }

    pub fn shake(&mut self, amount: f32) {
        self.fx.record(|| Ev::Shake { amount });
        self.trauma = (self.trauma + amount).min(1.0);
    }

    pub fn core(&self) -> Option<&Structure> {
        self.structures.iter().find(|s| s.kind == SKind::Core)
    }

    #[cfg(test)]
    pub fn wave_track_for_test(&self) -> Track {
        self.wave_track()
    }

    pub fn wave_track(&self) -> Track {
        // Permadeath has one death track, and it plays for every kind of death.
        let me = self.players.get(self.local);
        if self.permadeath && (self.game_over || me.is_some_and(|p| !p.alive)) {
            return Track::RestWell;
        }
        if self.game_over {
            return Track::GameOver;
        }
        if let Some(p) = me {
            if !p.alive {
                return match p.death_cause {
                    player::DeathCause::Regular => Track::Rot,
                    player::DeathCause::Zombie => Track::OneOfMany,
                    player::DeathCause::Explosion => Track::Smitherines,
                };
            }
        }
        match self.phase {
            Phase::Build { .. } => Track::Build,
            Phase::Assault => {
                if self.boss_track {
                    Track::Boss
                } else {
                    match self.wave % 3 {
                        1 => Track::CombatA,
                        2 => Track::CombatB,
                        _ => Track::CombatC,
                    }
                }
            }
        }
    }

    /// Advance the simulation with one player's input (the single-player case).
    pub fn update(&mut self, dt: f32, input: &player::PlayerInput) {
        self.update_all(dt, std::slice::from_ref(input));
    }

    /// Advance the simulation. `inputs` is indexed by player; missing entries mean idle.
    pub fn update_all(&mut self, dt: f32, inputs: &[player::PlayerInput]) {
        // A dedicated server with nobody on it holds still rather than losing the game.
        if !self.players.iter().any(|p| p.connected) {
            return;
        }
        self.time += dt;
        self.stats.time += dt;
        self.sfx_budget = [0; crate::audio::sfx::SFX_COUNT];
        self.trauma = (self.trauma - dt * 1.3).max(0.0);
        self.core_alert = (self.core_alert - dt).max(0.0);
        for m in &mut self.messages {
            m.ttl -= dt;
        }
        self.messages.retain(|m| m.ttl > 0.0);
        if self.combo_timer > 0.0 {
            self.combo_timer -= dt;
            if self.combo_timer <= 0.0 {
                self.combo = 0;
            }
        }

        let target_night = match self.phase {
            Phase::Build { .. } => 0.25,
            Phase::Assault => 1.0,
        };
        self.night += (target_night - self.night) * (dt * 0.5).min(1.0);

        if self.game_over {
            self.game_over_timer += dt;
        }

        self.update_waves(dt);
        self.rebuild_zombie_hash();

        // Rebuild the flow field regularly, and immediately after construction changes.
        self.nav_timer -= dt;
        if self.nav_dirty || self.nav_timer <= 0.0 {
            self.recompute_nav();
        }

        for i in 0..self.players.len() {
            if self.players[i].connected {
                let input = inputs.get(i).copied().unwrap_or_default();
                self.update_player(i, dt, &input);
            }
        }
        self.update_zombies(dt);
        self.zombie_status_fx(dt);
        self.update_structures(dt);
        self.update_projectiles(dt);
        self.update_hazards(dt);
        self.update_barrels(dt);
        self.process_explosions();
        self.reap_zombies();
        self.update_pickups(dt);

        {
            let collider = WorldCollider { map: &self.map, blocked_cells: &self.blocked_tiles };
            self.ragdolls.step(dt, &collider);
        }
        self.track_player_bodies(dt);
        self.bleed_ragdolls();
        self.fx.update(dt);
        self.remove_dead_structures();
        self.ambient_groans(dt);
        self.update_music();
    }

    pub fn update_music(&mut self) {
        let Some(me) = self.players.get(self.local) else { return };
        let (at, low) = (me.pos, me.hp < me.max_hp * 0.35);
        let near = self.zombies.iter().filter(|z| z.pos.distance_squared(at) < 500.0).count() as f32;
        let mut i = (near / 40.0).min(0.85);
        if low {
            i += 0.15;
        }
        if matches!(self.phase, Phase::Assault) {
            i += 0.1 + (self.wave as f32 * 0.02).min(0.2);
        }
        self.audio.send(Cmd::Intensity(i));
        self.audio.send(Cmd::Music(self.wave_track()));
    }

    fn ambient_groans(&mut self, dt: f32) {
        self.groan_timer -= dt;
        if self.groan_timer > 0.0 || self.zombies.is_empty() {
            return;
        }
        let count = self.zombies.len();
        self.groan_timer = self.rng.range(0.25, 1.0) * (1.2 - (count as f32 / 200.0).min(1.0)).max(0.25);
        let i = self.rng.int(count);
        let pos = self.zombies[i].pos;
        let id = match self.rng.int(4) {
            0 => Sfx::Groan1,
            1 => Sfx::Groan2,
            2 => Sfx::Groan3,
            _ => Sfx::Groan4,
        };
        self.snd(id, pos, 0.55);
    }

    fn recompute_nav(&mut self) {
        self.nav_dirty = false;
        self.nav_timer = 0.3;
        self.nav.clear_extra();
        for s in &self.structures {
            let d = sdef(s.kind);
            if d.nav_cost == 0 {
                continue;
            }
            if s.kind == SKind::Core {
                continue; // The reactor is a destination, not an obstacle.
            }
            let min = tile_min(s.tile);
            self.nav.add_cost(min, min + Vec2::splat(TILE), d.nav_cost as u16);
        }
        let mut seeds: Vec<(Vec2, u32)> = Vec::with_capacity(12);
        for p in &self.players {
            if p.alive && p.connected {
                seeds.push((p.pos2(), 0));
            }
        }
        if self.core().is_some() {
            // The horde favours the player a little over the reactor at equal distance.
            for (x, z) in [(-2.6, 0.0), (2.6, 0.0), (0.0, -2.6), (0.0, 2.6)] {
                seeds.push((Vec2::new(x, z), 40));
            }
        }
        self.nav.recompute(&seeds);
    }

    fn rebuild_zombie_hash(&mut self) {
        self.zhash_head.iter_mut().for_each(|h| *h = -1);
        self.zhash_next.clear();
        for (i, z) in self.zombies.iter().enumerate() {
            let c = zhash_cell(z.pos2());
            self.zhash_next.push(self.zhash_head[c]);
            self.zhash_head[c] = i as i32;
        }
    }

    /// Indices of living zombies whose cell overlaps the square around `p`.
    pub fn query_zombies(&mut self, p: Vec2, r: f32) -> Vec<u32> {
        let mut out = std::mem::take(&mut self.query_scratch);
        out.clear();
        let n = self.zombies.len() as i32;
        let cr = (r / ZHASH_CELL).ceil() as i32;
        let cx = ((p.x + HALF) / ZHASH_CELL).floor() as i32;
        let cz = ((p.y + HALF) / ZHASH_CELL).floor() as i32;
        for z in (cz - cr).max(0)..=(cz + cr).min(ZHASH_DIM as i32 - 1) {
            for x in (cx - cr).max(0)..=(cx + cr).min(ZHASH_DIM as i32 - 1) {
                let mut it = self.zhash_head[z as usize * ZHASH_DIM + x as usize];
                while it >= 0 {
                    if it < n {
                        out.push(it as u32);
                    }
                    it = self.zhash_next.get(it as usize).copied().unwrap_or(-1);
                }
            }
        }
        out
    }

    pub fn give_back_query(&mut self, v: Vec<u32>) {
        self.query_scratch = v;
    }

    // ------------------------------------------------------------- waves
    pub fn call_next_wave(&mut self) {
        self.hold_timer = false;
        if let Phase::Build { timer } = self.phase {
            let bonus = (timer as u32).min(60) * 2;
            if bonus > 0 {
                self.scrap += bonus;
                self.announce(&format!("EARLY START BONUS +{bonus}"), [1.0, 0.8, 0.3], 2.5, false);
            }
            self.phase = Phase::Build { timer: 0.0 };
        }
    }

    fn update_waves(&mut self, dt: f32) {
        if self.game_over {
            return;
        }
        match self.phase {
            Phase::Build { timer } => {
                if self.hold_timer {
                    return;
                }
                let t = timer - dt;
                if t <= 0.0 {
                    self.start_wave();
                } else {
                    self.phase = Phase::Build { timer: t };
                }
            }
            Phase::Assault => {
                self.spawn_tick(dt);
                if self.spawn_budget <= 0.0 && self.bosses_left == 0 && self.zombies.is_empty() {
                    self.finish_wave();
                }
            }
        }
    }

    pub fn start_wave(&mut self) {
        self.wave += 1;
        self.stats.wave_reached = self.wave;
        let n = self.wave as f32;
        self.spawn_budget = wave_budget(n);
        self.spawn_timer = 0.0;
        self.phase = Phase::Assault;
        self.bosses_left = if self.wave % 5 == 0 { self.wave / 5 } else { 0 };
        self.boss_track = self.bosses_left > 0;
        let count = (2 + self.wave as usize / 4).min(4);
        self.spawn_dirs.clear();
        let base = self.rng.int(4);
        for i in 0..count {
            self.spawn_dirs.push([Vec2::X, Vec2::NEG_Y, Vec2::NEG_X, Vec2::Y][(base + i) % 4]);
        }
        let text = if self.bosses_left > 0 {
            self.snd_ui(Sfx::Boss, 1.0);
            format!("WAVE {} - TITAN INCOMING", self.wave)
        } else {
            format!("WAVE {}", self.wave)
        };
        self.announce(&text, [1.0, 0.25, 0.15], 3.5, true);
        self.snd_ui(Sfx::Horn, 0.7);
    }

    fn finish_wave(&mut self) {
        let bonus = 80 + 20 * self.wave;
        self.scrap += bonus;
        self.stats.scrap_earned += bonus;
        self.phase = Phase::Build { timer: BUILD_TIME };
        self.boss_track = false;
        self.announce(&format!("WAVE {} SURVIVED  +{bonus}", self.wave), [0.5, 1.0, 0.6], 4.0, true);
        self.snd_ui(Sfx::Clear, 0.8);
        // Refill the grenades, and let the survivors catch their breath.
        for p in &mut self.players {
            p.grenades = (p.grenades + 2).min(6);
            if p.alive {
                p.hp = (p.hp + 30.0).min(p.max_hp);
            }
        }
        self.autosave_due = true;
        for p in &mut self.pickups {
            p.ttl = p.ttl.max(6.0);
        }
    }

    fn spawn_tick(&mut self, dt: f32) {
        let n = self.wave as f32;
        let cap = ((180.0 + n * 30.0) as usize).min(800);
        self.spawn_timer -= dt;
        if self.spawn_timer > 0.0 || self.spawn_dirs.is_empty() {
            return;
        }
        self.spawn_timer = 0.42 / (1.0 + n * 0.11);
        if self.bosses_left > 0 && self.spawn_budget < wave_budget(n) * 0.6 {
            self.bosses_left -= 1;
            let at = self.edge_spawn_point();
            self.spawn_zombie(ZKind::Titan, at);
            self.snd_ui(Sfx::Boss, 0.9);
            self.shake(0.5);
        }
        if self.spawn_budget <= 0.0 || self.zombies.len() >= cap {
            return;
        }
        // Spawn in small packs so that hordes visibly surge.
        let pack = 2 + self.rng.int(3 + self.wave as usize / 2);
        let at = self.edge_spawn_point();
        for _ in 0..pack {
            if self.spawn_budget <= 0.0 {
                break;
            }
            let kind = self.roll_kind();
            let jitter = self.rng.dir2() * self.rng.range(0.0, 3.0);
            let p = at + jitter;
            self.spawn_budget -= zdef(kind).cost;
            self.spawn_zombie(kind, p);
        }
    }

    fn roll_kind(&mut self) -> ZKind {
        let wave = self.wave;
        let total: f32 = SPAWN_KINDS.iter().map(|&k| { let d = zdef(k); if d.first_wave <= wave { d.weight } else { 0.0 } }).sum();
        let mut roll = self.rng.range(0.0, total);
        for &k in &SPAWN_KINDS {
            let d = zdef(k);
            if d.first_wave <= wave {
                roll -= d.weight;
                if roll <= 0.0 {
                    return k;
                }
            }
        }
        ZKind::Walker
    }

    fn edge_spawn_point(&mut self) -> Vec2 {
        for _ in 0..24 {
            let dir = self.spawn_dirs[self.rng.int(self.spawn_dirs.len())];
            let side = dir.perp();
            let p = dir * self.rng.range(50.0, 72.0) + side * self.rng.range(-58.0, 58.0);
            if !self.nav.is_blocked_cell(p) && self.nav.reachable(p) {
                return p;
            }
        }
        // Fall back on the road ends, which are always clear.
        self.spawn_dirs[0] * 76.0
    }

    // ------------------------------------------------------------- pickups
    pub fn drop_pickup(&mut self, kind: u8, pos: Vec3, value: u32) {
        if self.pickups.len() >= 300 {
            self.pickups.remove(0);
        }
        let d = self.rng.dir2() * self.rng.range(1.0, 3.5);
        self.pickups.push(Pickup {
            kind,
            pos: pos + Vec3::Y * 0.6,
            vel: vec3(d.x, self.rng.range(3.0, 6.0), d.y),
            value,
            ttl: if kind == 0 { 40.0 } else { 25.0 },
            spin: self.rng.range(0.0, 6.28),
        });
    }

    fn update_pickups(&mut self, dt: f32) {
        let vacuum = matches!(self.phase, Phase::Build { .. });
        let targets: Vec<(usize, Vec3)> = self.players.iter().enumerate()
            .filter(|(_, p)| p.alive && p.connected)
            .map(|(i, p)| (i, p.pos + Vec3::Y * 0.8))
            .collect();
        let mut collected: Vec<(usize, u8, u32, Vec3)> = Vec::new();
        self.pickups.retain_mut(|p| {
            p.ttl -= dt;
            p.spin += dt * 4.0;
            let nearest = targets.iter().min_by(|a, b| a.1.distance_squared(p.pos).total_cmp(&b.1.distance_squared(p.pos)));
            let magnet = if p.kind == 0 { 9.0 } else { 3.5 };
            let mut pulled = false;
            if let Some(&(who, at)) = nearest {
                let to = at - p.pos;
                let d = to.length();
                if d < magnet || (vacuum && p.kind == 0) {
                    pulled = true;
                    let sp = 26.0 * (1.2 - (d / magnet).min(1.0)).max(0.45);
                    p.vel = to / d.max(0.01) * sp;
                    p.pos += p.vel * dt;
                    if d < 0.9 {
                        collected.push((who, p.kind, p.value, at));
                        return false;
                    }
                }
            }
            if !pulled {
                p.vel.y -= 16.0 * dt;
                p.pos += p.vel * dt;
                if p.pos.y < 0.3 {
                    p.pos.y = 0.3;
                    p.vel.y = p.vel.y.abs() * 0.4;
                    p.vel.x *= 0.5;
                    p.vel.z *= 0.5;
                }
            }
            p.ttl > 0.0
        });
        for (who, kind, value, at) in collected {
            match kind {
                0 => {
                    self.scrap += value;
                    self.stats.scrap_earned += value;
                    self.snd(Sfx::Pickup, at, 0.3);
                }
                1 => {
                    let wi = self.players[who].weapon;
                    self.players[who].add_ammo_fraction(wi, 0.3);
                    self.snd(Sfx::Buy, at, 0.6);
                    if who == self.local {
                        self.announce("AMMO", [1.0, 0.8, 0.3], 1.2, false);
                    }
                }
                _ => {
                    let p = &mut self.players[who];
                    p.hp = (p.hp + value as f32).min(p.max_hp);
                    self.snd(Sfx::Buy, at, 0.6);
                    if who == self.local {
                        self.announce("MEDKIT", [0.5, 1.0, 0.6], 1.2, false);
                    }
                }
            }
        }
    }

    pub fn bleed_ragdolls(&mut self) {
        // Stumps spurt for a few seconds; a budget keeps huge blasts affordable.
        // Each machine simulates its own bodies, so this is not replicated.
        self.fx.muted(true);
        let mut budget = 24;
        let n = self.ragdolls.bodies.len();
        for i in 0..n {
            if budget == 0 {
                break;
            }
            let b = &self.ragdolls.bodies[i];
            for s in b.bleed {
                if s.1 > 0.0 && budget > 0 {
                    budget -= 1;
                    let pos = b.pos[s.0 as usize];
                    let dir = Vec3::Y + (pos - b.center()).normalize_or_zero() * 0.8;
                    self.fx.blood(pos, dir, 1, 3.5 * s.1.min(1.0));
                }
            }
        }
        self.fx.muted(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::new(42, Audio::silent())
    }

    #[test]
    fn map_generation_has_clear_spawn_paths() {
        for seed in [1, 2, 3, 42, 1000] {
            let w = World::new(seed, Audio::silent());
            assert!(w.map.props.len() > 40, "seed {seed} has few props");
            assert!(w.map.solids.len() > 40);
            // The center of the map must be open for building.
            assert!(!w.map.is_blocked(Vec2::new(8.0, 8.0), 1.0));
        }
    }

    #[test]
    fn flow_field_leads_to_the_player_or_reactor() {
        let mut w = world();
        w.players[0].pos = vec3(10.0, 0.0, 10.0);
        w.nav_dirty = true;
        w.update(0.016, &player::PlayerInput::default());
        // Every road end should have a route.
        for p in [Vec2::new(76.0, 0.0), Vec2::new(-76.0, 0.0), Vec2::new(0.0, 76.0), Vec2::new(0.0, -76.0)] {
            assert!(w.nav.reachable(p), "{p:?} unreachable");
            let d = w.nav.direction(p);
            assert!(d.length() > 0.5, "no direction at {p:?}");
            assert!(d.dot((-p).normalize()) > 0.3, "direction at {p:?} heads the wrong way: {d:?}");
        }
    }

    #[test]
    fn walls_change_navigation_cost() {
        let mut w = world();
        w.nav_dirty = true;
        w.update(0.016, &player::PlayerInput::default());
        let probe = Vec2::new(20.0, 0.0);
        let (x, z) = nav::cell_of(probe);
        let before = w.nav.dist[z * nav::DIM + x];
        // A complete ring of walls around the pad.
        for t in -6..=6 {
            for &(a, b) in &[(t, -6), (t, 6), (-6, t), (6, t)] {
                let tile = tile_of(Vec2::new(a as f32 * 2.0 + 1.0, b as f32 * 2.0 + 1.0));
                w.add_structure(SKind::SteelWall, tile);
            }
        }
        // Keep the player inside.
        w.players[0].pos = vec3(2.0, 0.0, 6.0);
        w.nav_dirty = true;
        w.update(0.016, &player::PlayerInput::default());
        let after = w.nav.dist[z * nav::DIM + x];
        assert!(after > before + 100, "enclosure should cost more: {before} -> {after}");
        assert!(w.nav.reachable(probe), "zombies must still path by breaking walls");
    }

    /// Plays many waves headless with the bot. Exercises almost every system together.
    #[test]
    fn bot_survives_or_dies_cleanly_over_many_waves() {
        let mut w = World::new(7, Audio::silent());
        let mut bot = bot::Bot::new();
        w.scrap = 20_000;
        let dt = 1.0 / 60.0;
        for frame in 0..(60 * 240) {
            let input = bot.think(&mut w, dt);
            w.update(dt, &input);
            if frame % 120 == 0 {
                assert_world_ok(&w);
            }
            if w.game_over {
                break;
            }
        }
        assert_world_ok(&w);
        assert!(w.stats.wave_reached >= 3, "reached wave {}", w.stats.wave_reached);
        assert!(w.stats.kills > 50, "only {} kills", w.stats.kills);
        assert!(w.stats.built > 0, "bot should have built something");
    }

    /// Prints how a fair game goes: `cargo test --release balance -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn balance_report() {
        for seed in 1..=6 {
            let mut w = World::new(seed, Audio::silent());
            let mut bot = bot::Bot::new();
            let dt = 1.0 / 60.0;
            let mut frames = 0u32;
            while !w.game_over && frames < 60 * 60 * 30 {
                let input = bot.think(&mut w, dt);
                w.update(dt, &input);
                frames += 1;
            }
            println!(
                "seed {seed}: wave {} kills {} deaths {} built {} time {:.0}s scrap {} peak-combo {}",
                w.stats.wave_reached, w.stats.kills, w.stats.deaths, w.stats.built, w.stats.time, w.scrap, w.stats.best_combo
            );
        }
    }

    fn assert_world_ok(w: &World) {
        for z in &w.zombies {
            assert!(z.pos.is_finite() && z.vel.is_finite() && z.hp.is_finite(), "bad zombie {:?}", z.pos);
            assert!(z.pos.x.abs() <= HALF + 2.0 && z.pos.z.abs() <= HALF + 2.0, "zombie left the map {:?}", z.pos);
        }
        assert!(w.players[0].pos.is_finite());
        for b in &w.ragdolls.bodies {
            assert!(b.pos.iter().all(|p| p.is_finite()));
        }
        for (i, s) in w.structures.iter().enumerate() {
            if s.kind != SKind::Core {
                assert_eq!(w.tiles[tile_index(s.tile).unwrap()], i as i32, "tile map out of sync");
            }
        }
    }

    #[test]
    fn explosions_kill_zombies_and_make_ragdolls() {
        let mut w = world();
        w.phase = Phase::Assault;
        w.wave = 1;
        w.spawn_budget = 1.0;
        w.spawn_dirs = vec![Vec2::X];
        for i in 0..20 {
            w.spawn_zombie(ZKind::Walker, Vec2::new(30.0 + i as f32 * 0.1, 20.0));
        }
        // Freshly spawned zombies are still climbing out of the ground, and immune.
        w.zombies.iter_mut().for_each(|z| z.spawn_t = 0.0);
        w.explosions.push(Explosion { pos: vec3(30.5, 0.5, 20.0), radius: 8.0, damage: 500.0, from_enemy: false, force: 18.0, spares_players: false, barrel: false });
        let input = player::PlayerInput::default();
        w.update(0.016, &input);
        assert!(w.zombies.len() < 20);
        assert!(!w.ragdolls.bodies.is_empty() || w.stats.kills > 0);
    }

    #[test]
    fn sentries_shoot_zombies() {
        let mut w = world();
        w.phase = Phase::Assault;
        w.wave = 1;
        w.spawn_budget = 1.0;
        w.spawn_dirs = vec![Vec2::X];
        w.spawn_zombie(ZKind::Walker, Vec2::new(-7.0, 12.0));
        let input = player::PlayerInput { aim: vec3(5.0, 1.2, 0.0), ..Default::default() };
        for _ in 0..600 {
            w.update(1.0 / 60.0, &input);
            if w.stats.kills > 0 {
                return;
            }
        }
        panic!("sentry failed to kill a nearby walker in 10 seconds");
    }

    #[test]
    fn building_spends_scrap_and_blocks_tiles() {
        let mut w = world();
        let t = tile_of(Vec2::new(11.0, 11.0));
        let before = w.scrap;
        assert!(w.try_build(SKind::WoodWall, t).is_ok());
        assert_eq!(w.scrap, before - sdef(SKind::WoodWall).cost);
        assert!(w.blocked_tiles[tile_index(t).unwrap()]);
        assert!(w.try_build(SKind::WoodWall, t).is_err(), "double build must be refused");
        w.scrap = 0;
        assert!(w.try_build(SKind::SentryMg, tile_of(Vec2::new(15.0, 11.0))).is_err());
        w.sell_at(t);
        assert!(!w.blocked_tiles[tile_index(t).unwrap()]);
        assert!(w.scrap > 0, "selling refunds scrap");
    }

    #[test]
    fn reactor_death_ends_the_game() {
        let mut w = world();
        let i = w.structures.iter().position(|s| s.kind == SKind::Core).unwrap();
        w.structures[i].hp = 0.0;
        w.update(0.016, &player::PlayerInput::default());
        assert!(w.game_over);
    }

    #[test]
    fn every_weapon_fires_without_panicking() {
        let mut w = world();
        w.players[0].pos = vec3(20.0, 0.0, 20.0);
        for i in 0..WEAPONS.len() {
            w.players[0].owned[i] = true;
            w.players[0].reserve[i] = 1000;
            w.players[0].mag[i] = WEAPONS[i].mag;
        }
        w.spawn_zombie(ZKind::Walker, Vec2::new(25.0, 20.0));
        for i in 0..WEAPONS.len() {
            w.players[0].weapon = i;
            w.players[0].reload = 0.0;
            w.players[0].mag[i] = WEAPONS[i].mag;
            let input = player::PlayerInput { aim: vec3(25.0, 1.2, 20.0), fire: true, ..Default::default() };
            for _ in 0..90 {
                w.update(1.0 / 60.0, &input);
            }
        }
        assert_world_ok(&w);
    }
}
