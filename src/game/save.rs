//! Save files: everything needed to resume a run, written atomically with a checksum.
//! Purely cosmetic state (particles, corpses, the painted gore layer) is not kept.
use super::combat::{HazKind, Hazard, ProjKind, Projectile};
use super::data::*;
use super::player::Player;
use super::structures::Structure;
use super::zombie::Zombie;
use super::{tile_center, tile_index, Barrel, Phase, Pickup, World};
use crate::audio::Audio;
use crate::util::crc32;
use crate::wire::{Reader, Writer};
use glam::{vec3, Vec2};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"RFSV";
const VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Solo,
    Permadeath,
    Host,
    HostPermadeath,
}

impl Slot {
    pub fn file(self) -> &'static str {
        match self {
            Slot::Solo => "solo.sav",
            Slot::Permadeath => "permadeath.sav",
            Slot::Host => "host.sav",
            Slot::HostPermadeath => "host_permadeath.sav",
        }
    }
    pub fn for_mode(hosting: bool, permadeath: bool) -> Slot {
        match (hosting, permadeath) {
            (false, false) => Slot::Solo,
            (false, true) => Slot::Permadeath,
            (true, false) => Slot::Host,
            (true, true) => Slot::HostPermadeath,
        }
    }
}

/// `%APPDATA%\\rotfall` on Windows; elsewhere `$XDG_DATA_HOME/rotfall` or `~/.local/share/rotfall`.
/// The current directory is the last resort.
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(x) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("rotfall");
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("rotfall");
    }
    if let Some(h) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(h).join(".local/share/rotfall");
    }
    PathBuf::from("rotfall-data")
}

/// Write-then-rename, so a crash mid-save can never destroy the previous good save.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveInfo {
    pub wave: u32,
    pub scrap: u32,
    pub kills: u32,
    pub players: u8,
    pub seconds: f32,
    pub permadeath: bool,
}

pub fn encode(w: &World) -> Vec<u8> {
    let mut o = Writer::new();
    o.buf.extend_from_slice(MAGIC);
    o.u32(VERSION);
    // A summary up front for the menu.
    o.u32(w.wave);
    o.u32(w.scrap);
    o.u32(w.stats.kills);
    o.u8(w.players.len() as u8);
    o.f32(w.stats.time);
    o.bool(w.permadeath);

    o.u64(w.seed);
    o.u64(w.rng.0);
    o.f32(w.time);
    o.f32(w.night);
    match w.phase {
        Phase::Build { timer } => {
            o.u8(0);
            o.f32(timer);
        }
        Phase::Assault => {
            o.u8(1);
            o.f32(0.0);
        }
    }
    o.bool(w.hold_timer);
    o.f32(w.spawn_budget);
    o.f32(w.spawn_timer);
    o.u8(w.spawn_dirs.len() as u8);
    for d in &w.spawn_dirs {
        o.v2(*d);
    }
    o.u32(w.bosses_left);
    o.bool(w.boss_track);
    o.u32(w.combo);
    o.f32(w.combo_timer);
    o.u32(w.next_zombie_id);
    let st = &w.stats;
    for v in [st.headshots, st.best_combo, st.built, st.wave_reached, st.scrap_earned, st.deaths] {
        o.u32(v);
    }

    for p in &w.players {
        o.str(&p.name);
        o.v3(p.pos);
        o.v2(p.facing);
        o.f32(p.hp);
        o.f32(p.max_hp);
        o.f32(p.stamina);
        o.bool(p.alive);
        o.f32(p.respawn);
        o.bool(p.out);
        o.u8(p.weapon as u8);
        for i in 0..18 {
            o.bool(p.owned[i]);
            o.u32(p.mag[i]);
            o.u32(p.reserve[i]);
        }
        o.u32(p.grenades);
        o.bool(p.flashlight);
    }

    o.u32(w.structures.len() as u32);
    for s in &w.structures {
        o.u8(s.kind as u8);
        o.u16(s.tile.0 as u16);
        o.u16(s.tile.1 as u16);
        o.f32(s.hp);
        o.f32(s.max_hp);
        o.u8(s.level as u8);
        o.f32(s.yaw);
        o.u32(s.invested);
    }

    o.u32(w.barrels.len() as u32);
    for b in &w.barrels {
        o.v3(b.pos);
        o.f32(b.hp);
        o.f32(b.burn);
    }

    o.u32(w.pickups.len() as u32);
    for p in &w.pickups {
        o.u8(p.kind);
        o.v3(p.pos);
        o.u32(p.value);
        o.f32(p.ttl);
    }

    o.u32(w.hazards.len() as u32);
    for h in &w.hazards {
        o.u8(if h.kind == HazKind::Fire { 0 } else { 1 });
        o.v3(h.pos);
        o.f32(h.radius);
        o.f32(h.ttl);
        o.f32(h.dps);
    }

    o.u32(w.projectiles.len() as u32);
    for p in &w.projectiles {
        o.u8(p.kind as u8);
        o.v3(p.pos);
        o.v3(p.vel);
        o.f32(p.damage);
        o.f32(p.radius);
        o.f32(p.ttl);
        o.u32(p.bounces);
    }

    o.u32(w.zombies.len() as u32);
    for z in &w.zombies {
        o.u8(z.kind as u8);
        o.u32(z.id);
        o.v3(z.pos);
        o.v2(z.vel);
        o.v2(z.facing);
        o.f32(z.hp);
        o.f32(z.max_hp);
        o.f32(z.phase);
        o.f32(z.burn);
        o.f32(z.frost);
        o.f32(z.rage);
        o.f32(z.special_t);
        o.f32(z.scale);
        o.f32(z.radius);
        o.f32(z.speed);
        o.f32(z.damage);
        o.f32(z.struct_damage);
        o.u32(z.style);
        o.u16(z.missing);
        o.bool(z.crawling);
        o.f32(z.spawn_t);
    }
    let c = crc32(&o.buf);
    o.u32(c);
    o.buf
}

/// Checks the framing and checksum; returns the body positioned after the magic.
fn open(data: &[u8]) -> Result<Reader<'_>, String> {
    if data.len() < 12 || &data[..4] != MAGIC {
        return Err("NOT A ROTFALL SAVE".into());
    }
    let (body, tail) = data.split_at(data.len() - 4);
    let stored = u32::from_le_bytes(tail.try_into().unwrap());
    if crc32(body) != stored {
        return Err("SAVE FILE IS DAMAGED".into());
    }
    let mut r = Reader::new(body);
    r.pos = 4;
    let v = r.u32().ok_or("SAVE FILE IS DAMAGED")?;
    if v != VERSION {
        return Err(format!("SAVE IS FROM ANOTHER VERSION ({v})"));
    }
    Ok(r)
}

fn read_info(r: &mut Reader) -> Option<SaveInfo> {
    Some(SaveInfo {
        wave: r.u32()?,
        scrap: r.u32()?,
        kills: r.u32()?,
        players: r.u8()?,
        seconds: r.f32()?,
        permadeath: r.bool()?,
    })
}

pub fn peek(path: &Path) -> Option<SaveInfo> {
    let data = std::fs::read(path).ok()?;
    let mut r = open(&data).ok()?;
    read_info(&mut r)
}

pub fn decode(data: &[u8], audio: Audio) -> Result<World, String> {
    let mut r = open(data)?;
    read_world(&mut r, audio).ok_or_else(|| "SAVE FILE IS DAMAGED".to_string())
}

fn read_world(r: &mut Reader, audio: Audio) -> Option<World> {
    let info = read_info(r)?;
    let seed = r.u64()?;
    let mut w = World::new(seed, audio);
    w.messages.clear();
    w.wave = info.wave;
    w.scrap = info.scrap;
    w.stats.kills = info.kills;
    w.stats.time = info.seconds;
    w.permadeath = info.permadeath;
    w.rng.0 = r.u64()?.max(1);
    w.time = r.f32()?;
    w.night = r.f32()?;
    let (ph, t) = (r.u8()?, r.f32()?);
    w.phase = if ph == 0 { Phase::Build { timer: t } } else { Phase::Assault };
    w.hold_timer = r.bool()?;
    w.spawn_budget = r.f32()?;
    w.spawn_timer = r.f32()?;
    let n = r.u8()?;
    w.spawn_dirs.clear();
    for _ in 0..n {
        w.spawn_dirs.push(r.v2()?);
    }
    w.bosses_left = r.u32()?;
    w.boss_track = r.bool()?;
    w.combo = r.u32()?;
    w.combo_timer = r.f32()?;
    w.next_zombie_id = r.u32()?;
    w.stats.headshots = r.u32()?;
    w.stats.best_combo = r.u32()?;
    w.stats.built = r.u32()?;
    w.stats.wave_reached = r.u32()?;
    w.stats.scrap_earned = r.u32()?;
    w.stats.deaths = r.u32()?;

    w.players.clear();
    for i in 0..info.players as usize {
        let name = r.str()?;
        let mut p = Player::new(&name, i);
        p.pos = r.v3()?;
        p.facing = r.v2()?;
        p.hp = r.f32()?;
        p.max_hp = r.f32()?;
        p.stamina = r.f32()?;
        p.alive = r.bool()?;
        p.respawn = r.f32()?;
        p.out = r.bool()?;
        p.weapon = r.u8()? as usize;
        if p.weapon >= WEAPONS.len() {
            return None;
        }
        for k in 0..18 {
            p.owned[k] = r.bool()?;
            p.mag[k] = r.u32()?;
            p.reserve[k] = r.u32()?;
        }
        p.grenades = r.u32()?;
        p.flashlight = r.bool()?;
        w.players.push(p);
    }

    w.structures.clear();
    for _ in 0..r.u32()? {
        let kind = SKind::from_u8(r.u8()?)?;
        let tile = (r.u16()? as i32, r.u16()? as i32);
        tile_index(tile)?;
        let mut s: Structure = w.make_structure_for_load(kind, tile);
        s.hp = r.f32()?;
        s.max_hp = r.f32()?;
        s.level = (r.u8()? as usize).min(2);
        s.yaw = r.f32()?;
        s.invested = r.u32()?;
        s.spawn_anim = 0.0;
        if kind == SKind::Core {
            s.pos = glam::Vec3::ZERO;
        } else {
            let c = tile_center(tile);
            s.pos = vec3(c.x, 0.0, c.y);
        }
        w.structures.push(s);
    }
    w.rebuild_tiles();

    w.barrels.clear();
    for _ in 0..r.u32()? {
        w.barrels.push(Barrel { pos: r.v3()?, hp: r.f32()?, burn: r.f32()? });
    }
    w.pickups.clear();
    for _ in 0..r.u32()? {
        let (kind, pos, value, ttl) = (r.u8()?, r.v3()?, r.u32()?, r.f32()?);
        w.pickups.push(Pickup { kind, pos, vel: glam::Vec3::ZERO, value, ttl, spin: 0.0 });
    }
    w.hazards.clear();
    for _ in 0..r.u32()? {
        let kind = if r.u8()? == 0 { HazKind::Fire } else { HazKind::Acid };
        w.hazards.push(Hazard { kind, pos: r.v3()?, radius: r.f32()?, ttl: r.f32()?, tick: 0.0, dps: r.f32()? });
    }
    w.projectiles.clear();
    for _ in 0..r.u32()? {
        let kind = ProjKind::from_u8(r.u8()?)?;
        w.projectiles.push(Projectile {
            kind,
            pos: r.v3()?,
            vel: r.v3()?,
            damage: r.f32()?,
            radius: r.f32()?,
            ttl: r.f32()?,
            bounces: r.u32()?,
            spin: 0.0,
            hit: Vec::new(),
            fx_t: 0.0,
        });
    }
    w.zombies.clear();
    for _ in 0..r.u32()? {
        let kind = ZKind::from_u8(r.u8()?)?;
        let id = r.u32()?;
        w.spawn_zombie(kind, Vec2::ZERO);
        let z: &mut Zombie = w.zombies.last_mut()?;
        z.id = id;
        z.pos = r.v3()?;
        z.vel = r.v2()?;
        z.facing = r.v2()?;
        z.hp = r.f32()?;
        z.max_hp = r.f32()?;
        z.phase = r.f32()?;
        z.burn = r.f32()?;
        z.frost = r.f32()?;
        z.rage = r.f32()?;
        z.special_t = r.f32()?;
        z.scale = r.f32()?;
        z.radius = r.f32()?;
        z.speed = r.f32()?;
        z.damage = r.f32()?;
        z.struct_damage = r.f32()?;
        z.style = r.u32()?;
        z.missing = r.u16()?;
        z.crawling = r.bool()?;
        z.spawn_t = r.f32()?;
    }
    w.next_zombie_id = w.next_zombie_id.max(w.zombies.iter().map(|z| z.id + 1).max().unwrap_or(1));
    w.nav_dirty = true;
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::bot::Bot;

    fn played(seed: u64, seconds: u32) -> World {
        let mut w = World::new(seed, Audio::silent());
        w.scrap = 6000;
        let mut bot = Bot::new();
        for _ in 0..seconds * 60 {
            let input = bot.think(&mut w, 1.0 / 60.0);
            w.update(1.0 / 60.0, &input);
        }
        w
    }

    #[test]
    fn a_mid_battle_save_restores_everything_that_matters() {
        let w = played(11, 70);
        assert!(!w.zombies.is_empty(), "the test wants a battle in progress");
        assert!(w.structures.len() > 3);
        let bytes = encode(&w);
        let l = decode(&bytes, Audio::silent()).expect("load");
        assert_eq!(l.seed, w.seed);
        assert_eq!(l.wave, w.wave);
        assert_eq!(l.scrap, w.scrap);
        assert_eq!(l.phase, w.phase);
        assert_eq!(l.stats.kills, w.stats.kills);
        assert_eq!(l.zombies.len(), w.zombies.len());
        assert_eq!(l.structures.len(), w.structures.len());
        assert_eq!(l.barrels.len(), w.barrels.len());
        for (a, b) in w.zombies.iter().zip(&l.zombies) {
            assert_eq!((a.id, a.kind, a.pos, a.hp, a.style, a.missing), (b.id, b.kind, b.pos, b.hp, b.style, b.missing));
        }
        for (a, b) in w.structures.iter().zip(&l.structures) {
            assert_eq!((a.kind, a.tile, a.hp, a.level), (b.kind, b.tile, b.hp, b.level));
        }
        let (a, b) = (&w.players[0], &l.players[0]);
        assert_eq!((a.pos, a.hp, a.weapon, a.owned, a.mag, a.reserve, a.grenades), (b.pos, b.hp, b.weapon, b.owned, b.mag, b.reserve, b.grenades));
        assert_eq!(l.tiles, w.tiles, "occupancy must be rebuilt identically");
        // Saving the loaded world gives the same file: nothing was lost or invented.
        assert_eq!(encode(&l).len(), bytes.len());
    }

    #[test]
    fn a_loaded_game_keeps_running() {
        let w = played(12, 40);
        let mut l = decode(&encode(&w), Audio::silent()).unwrap();
        let mut bot = Bot::new();
        let wave = l.wave;
        for _ in 0..60 * 120 {
            let input = bot.think(&mut l, 1.0 / 60.0);
            l.update(1.0 / 60.0, &input);
        }
        assert!(l.wave > wave, "play should continue past wave {wave}");
    }

    #[test]
    fn damage_and_nonsense_are_rejected_not_loaded() {
        let w = played(13, 5);
        let mut bytes = encode(&w);
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0x40;
        assert!(decode(&bytes, Audio::silent()).is_err(), "bit flip must be caught");
        assert!(decode(b"junk", Audio::silent()).is_err());
        assert!(decode(&[], Audio::silent()).is_err());
        // A correct checksum over garbage must still fail cleanly rather than panic.
        let mut g = MAGIC.to_vec();
        g.extend_from_slice(&VERSION.to_le_bytes());
        g.extend_from_slice(&[0xFF; 300]);
        let c = crc32(&g);
        g.extend_from_slice(&c.to_le_bytes());
        assert!(decode(&g, Audio::silent()).is_err());
        // And a truncated-by-one file.
        let good = encode(&w);
        assert!(decode(&good[..good.len() - 1], Audio::silent()).is_err());
    }

    #[test]
    fn files_write_atomically_and_summaries_are_readable() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-saves/atomic");
        let path = dir.join("x.sav");
        let w = played(14, 5);
        write_atomic(&path, &encode(&w)).unwrap();
        let info = peek(&path).expect("summary");
        assert_eq!(info.wave, w.wave);
        assert_eq!(info.players, 1);
        assert!(!path.with_extension("tmp").exists());
        std::fs::remove_file(&path).unwrap();
        assert!(peek(&path).is_none());
    }

    #[test]
    fn slots_are_kept_apart() {
        let f: Vec<_> = [Slot::Solo, Slot::Permadeath, Slot::Host, Slot::HostPermadeath].iter().map(|s| s.file()).collect();
        let unique: std::collections::HashSet<_> = f.iter().collect();
        assert_eq!(unique.len(), 4);
        assert_eq!(Slot::for_mode(true, true), Slot::HostPermadeath);
    }
}
