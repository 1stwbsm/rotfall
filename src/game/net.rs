//! Co-op networking over TCP.
//!
//! The host's `World` is authoritative for everything except each client's own movement.
//! Clients move themselves locally, which keeps controls instant at any ping, and tell the
//! server where they are; that trade only suits a co-op game among friends, so positions are
//! bounds-checked but not otherwise policed. Everything else flows from the server: entity
//! snapshots at 30 Hz, plus a list of events (`Ev`) that the client replays as effects.
//! A slow client simply skips snapshots; its queue of events waits, shedding chaff first.
use super::data::*;
use super::fx::Ev;
use super::player::{Player, PlayerInput};
use super::structures::Structure;
use super::combat::{ProjKind, Projectile};
use super::{tile_index, Phase, Pickup, Barrel, World};
use crate::audio::Sfx;
use crate::wire::{Reader, Writer};
use glam::{vec3, Vec2, Vec3};
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub const PROTOCOL: u16 = 2;
pub const DEFAULT_PORT: u16 = 27416;
pub const BEACON_PORT: u16 = 27417;
const SNAP_INTERVAL: f32 = 1.0 / 30.0;
const INPUT_INTERVAL: f32 = 1.0 / 30.0;
/// Every Nth snapshot lists all zombies; the rest only those near the recipient.
const COMPLETE_EVERY: u32 = 8;
const NEAR_RANGE2: f32 = 62.0 * 62.0;
const EVENT_BUDGET: usize = 14_000;
const MAX_FRAME: usize = 4 * 1024 * 1024;
const MAX_CLIENT_FRAME: usize = 8 * 1024;
const TIMEOUT: f32 = 12.0;
const HANDSHAKE_TIMEOUT: f32 = 5.0;
pub const MAX_PLAYERS_LIMIT: u8 = 8;

// Client to server.
const C_HELLO: u8 = 1;
const C_INPUT: u8 = 2;
const C_COMMAND: u8 = 3;
// Server to client.
const S_WELCOME: u8 = 1;
const S_REJECT: u8 = 2;
const S_SNAPSHOT: u8 = 3;
const S_ROSTER: u8 = 4;
const S_NOTE: u8 = 5;

/// Anything a player can ask for that changes shared state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Build(u8, (i32, i32)),
    Sell((i32, i32)),
    Upgrade((i32, i32)),
    Repair((i32, i32)),
    BuyWeapon(u8),
    BuyAmmo(u8),
    BuyGrenades,
    NextWave,
}

impl Command {
    fn write(&self, w: &mut Writer) {
        let tile = |w: &mut Writer, t: (i32, i32)| {
            w.u16(t.0 as u16);
            w.u16(t.1 as u16);
        };
        match *self {
            Command::Build(k, t) => {
                w.u8(0);
                w.u8(k);
                tile(w, t);
            }
            Command::Sell(t) => {
                w.u8(1);
                tile(w, t);
            }
            Command::Upgrade(t) => {
                w.u8(2);
                tile(w, t);
            }
            Command::Repair(t) => {
                w.u8(3);
                tile(w, t);
            }
            Command::BuyWeapon(i) => {
                w.u8(4);
                w.u8(i);
            }
            Command::BuyAmmo(i) => {
                w.u8(5);
                w.u8(i);
            }
            Command::BuyGrenades => w.u8(6),
            Command::NextWave => w.u8(7),
        }
    }

    fn read(r: &mut Reader) -> Option<Command> {
        fn tile(r: &mut Reader) -> Option<(i32, i32)> {
            Some((r.u16()? as i32, r.u16()? as i32))
        }
        Some(match r.u8()? {
            0 => {
                let k = r.u8()?;
                Command::Build(k, tile(r)?)
            }
            1 => Command::Sell(tile(r)?),
            2 => Command::Upgrade(tile(r)?),
            3 => Command::Repair(tile(r)?),
            4 => Command::BuyWeapon(r.u8()?),
            5 => Command::BuyAmmo(r.u8()?),
            6 => Command::BuyGrenades,
            7 => Command::NextWave,
            _ => return None,
        })
    }
}

impl World {
    /// Carries out a command for player `p`. Safe against any values a client might send.
    pub fn apply_command(&mut self, p: usize, c: Command) -> Result<(), String> {
        let r = match c {
            Command::Build(k, t) => match SKind::from_u8(k) {
                Some(kind) if kind != SKind::Core => self.try_build(kind, t),
                _ => Err("CANNOT BUILD THAT"),
            },
            Command::Sell(t) => {
                if self.sell_at(t) {
                    Ok(())
                } else {
                    Err("NOTHING TO SELL")
                }
            }
            Command::Upgrade(t) => self.upgrade_at(t),
            Command::Repair(t) => self.repair_at(t),
            Command::BuyWeapon(i) => self.buy_weapon(p, i as usize),
            Command::BuyAmmo(i) => self.buy_ammo(p, i as usize),
            Command::BuyGrenades => self.buy_grenades(p),
            Command::NextWave => {
                // In a lobby the host decides when the match begins. A dedicated server has
                // no host player, so there anyone may start it.
                if self.hold_timer && p != self.local && self.local < self.players.len() {
                    return Err("THE HOST STARTS THE MATCH".to_string());
                }
                self.call_next_wave();
                Ok(())
            }
        };
        r.map_err(str::to_string)
    }
}

// ------------------------------------------------------------------- events on the wire
fn color3(w: &mut Writer, c: [f32; 3]) {
    w.q(c[0]);
    w.q(c[1]);
    w.q(c[2]);
}

fn rcolor3(r: &mut Reader) -> Option<[f32; 3]> {
    Some([r.q()?, r.q()?, r.q()?])
}

fn color4(w: &mut Writer, c: [f32; 4]) {
    for v in c {
        w.q(v);
    }
}

fn rcolor4(r: &mut Reader) -> Option<[f32; 4]> {
    Some([r.q()?, r.q()?, r.q()?, r.q()?])
}

pub fn write_ev(w: &mut Writer, e: &Ev) {
    match e {
        Ev::Blood { pos, dir, amount, speed } => {
            w.u8(0);
            w.q3(*pos);
            w.q3(*dir);
            w.u8(*amount);
            w.q(*speed);
        }
        Ev::Splat { pos, size } => {
            w.u8(1);
            w.q3(*pos);
            w.q(*size);
        }
        Ev::Muzzle { pos, dir, size, color } => {
            w.u8(2);
            w.q3(*pos);
            w.q3(*dir);
            w.q(*size);
            w.q3(*color);
        }
        Ev::Tracer { from, to, color, width } => {
            w.u8(3);
            w.q3(*from);
            w.q3(*to);
            color3(w, *color);
            w.q(*width);
        }
        Ev::Spark { pos, vel, color, life } => {
            w.u8(4);
            w.q3(*pos);
            w.q3(*vel);
            color3(w, *color);
            w.q(*life);
        }
        Ev::Smoke { pos, vel, size, life, shade, opacity } => {
            w.u8(5);
            w.q3(*pos);
            w.q3(*vel);
            w.q(*size);
            w.q(*life);
            w.q(*shade);
            w.q(*opacity);
        }
        Ev::Fire { pos, vel, size, life } => {
            w.u8(6);
            w.q3(*pos);
            w.q3(*vel);
            w.q(*size);
            w.q(*life);
        }
        Ev::Frost { pos, vel, size, life } => {
            w.u8(7);
            w.q3(*pos);
            w.q3(*vel);
            w.q(*size);
            w.q(*life);
        }
        Ev::Lightning { from, to, color, width } => {
            w.u8(8);
            w.q3(*from);
            w.q3(*to);
            color3(w, *color);
            w.q(*width);
        }
        Ev::Explosion { pos, radius } => {
            w.u8(9);
            w.q3(*pos);
            w.q(*radius);
        }
        Ev::Debris { pos, vel, mesh, scale, tint, life, bleeds } => {
            w.u8(10);
            w.q3(*pos);
            w.q3(*vel);
            w.u16(*mesh);
            w.q3(*scale);
            color3(w, *tint);
            w.q(*life);
            w.bool(*bleeds);
        }
        Ev::Casing { pos, right } => {
            w.u8(11);
            w.q3(*pos);
            w.q3(*right);
        }
        Ev::Gibs { pos, count, speed, tint } => {
            w.u8(12);
            w.q3(*pos);
            w.u8(*count);
            w.q(*speed);
            color3(w, *tint);
        }
        Ev::Decal { x, z, size, color, kind } => {
            w.u8(13);
            w.q(*x);
            w.q(*z);
            w.q(*size);
            for v in color {
                w.q(*v);
            }
            w.u8(*kind);
        }
        Ev::Light { pos, radius, color, life } => {
            w.u8(14);
            w.q3(*pos);
            w.q(*radius);
            w.q3(*color);
            w.q(*life);
        }
        Ev::Glow { pos, vel, size, grow, life, c0, c1 } => {
            w.u8(15);
            w.q3(*pos);
            w.q3(*vel);
            w.q(*size);
            w.q(*grow);
            w.q(*life);
            color4(w, *c0);
            color4(w, *c1);
        }
        Ev::Snd { id, pos, gain } => {
            w.u8(16);
            w.u8(*id);
            w.q3(*pos);
            w.q(*gain);
        }
        Ev::SndUi { id, gain } => {
            w.u8(17);
            w.u8(*id);
            w.q(*gain);
        }
        Ev::Msg { text, color, ttl, big } => {
            w.u8(18);
            w.str(text);
            color3(w, *color);
            w.q(*ttl);
            w.bool(*big);
        }
        Ev::Shake { amount } => {
            w.u8(19);
            w.q(*amount);
        }
        Ev::Blast { pos, radius, force } => {
            w.u8(20);
            w.q3(*pos);
            w.q(*radius);
            w.q(*force);
        }
        Ev::ZombieDied { id, dir, point, force, gore, flags } => {
            w.u8(21);
            w.u32(*id);
            w.q3(*dir);
            w.q3(*point);
            w.q(*force);
            w.q(*gore);
            w.u8(*flags);
        }
        Ev::Severed { id, mask } => {
            w.u8(22);
            w.u32(*id);
            w.u16(*mask);
        }
        Ev::PlayerDied { player, away, cause } => {
            w.u8(23);
            w.u8(*player);
            w.q3(*away);
            w.u8(*cause);
        }
        Ev::PlayerDown { player, from, force } => {
            w.u8(25);
            w.u8(*player);
            w.q3(*from);
            w.q(*force);
        }
        Ev::CorpseRose { player } => {
            w.u8(26);
            w.u8(*player);
        }
        Ev::Recoil { player, kick } => {
            w.u8(24);
            w.u8(*player);
            w.q(*kick);
        }
    }
}

pub fn read_ev(r: &mut Reader) -> Option<Ev> {
    Some(match r.u8()? {
        0 => Ev::Blood { pos: r.q3()?, dir: r.q3()?, amount: r.u8()?, speed: r.q()? },
        1 => Ev::Splat { pos: r.q3()?, size: r.q()? },
        2 => Ev::Muzzle { pos: r.q3()?, dir: r.q3()?, size: r.q()?, color: r.q3()? },
        3 => Ev::Tracer { from: r.q3()?, to: r.q3()?, color: rcolor3(r)?, width: r.q()? },
        4 => Ev::Spark { pos: r.q3()?, vel: r.q3()?, color: rcolor3(r)?, life: r.q()? },
        5 => Ev::Smoke { pos: r.q3()?, vel: r.q3()?, size: r.q()?, life: r.q()?, shade: r.q()?, opacity: r.q()? },
        6 => Ev::Fire { pos: r.q3()?, vel: r.q3()?, size: r.q()?, life: r.q()? },
        7 => Ev::Frost { pos: r.q3()?, vel: r.q3()?, size: r.q()?, life: r.q()? },
        8 => Ev::Lightning { from: r.q3()?, to: r.q3()?, color: rcolor3(r)?, width: r.q()? },
        9 => Ev::Explosion { pos: r.q3()?, radius: r.q()? },
        10 => Ev::Debris {
            pos: r.q3()?,
            vel: r.q3()?,
            mesh: {
                let m = r.u16()?;
                if m as usize >= crate::render::mesh::MESH_COUNT {
                    return None;
                }
                m
            },
            scale: r.q3()?,
            tint: rcolor3(r)?,
            life: r.q()?,
            bleeds: r.bool()?,
        },
        11 => Ev::Casing { pos: r.q3()?, right: r.q3()? },
        12 => Ev::Gibs { pos: r.q3()?, count: r.u8()?, speed: r.q()?, tint: rcolor3(r)? },
        13 => Ev::Decal { x: r.q()?, z: r.q()?, size: r.q()?, color: rcolor4(r)?, kind: r.u8()? },
        14 => Ev::Light { pos: r.q3()?, radius: r.q()?, color: r.q3()?, life: r.q()? },
        15 => Ev::Glow { pos: r.q3()?, vel: r.q3()?, size: r.q()?, grow: r.q()?, life: r.q()?, c0: rcolor4(r)?, c1: rcolor4(r)? },
        16 => Ev::Snd { id: r.u8()?, pos: r.q3()?, gain: r.q()? },
        17 => Ev::SndUi { id: r.u8()?, gain: r.q()? },
        18 => Ev::Msg { text: r.str()?, color: rcolor3(r)?, ttl: r.q()?, big: r.bool()? },
        19 => Ev::Shake { amount: r.q()? },
        20 => Ev::Blast { pos: r.q3()?, radius: r.q()?, force: r.q()? },
        21 => Ev::ZombieDied { id: r.u32()?, dir: r.q3()?, point: r.q3()?, force: r.q()?, gore: r.q()?, flags: r.u8()? },
        22 => Ev::Severed { id: r.u32()?, mask: r.u16()? },
        23 => Ev::PlayerDied { player: r.u8()?, away: r.q3()?, cause: r.u8()? },
        25 => Ev::PlayerDown { player: r.u8()?, from: r.q3()?, force: r.q()? },
        26 => Ev::CorpseRose { player: r.u8()? },
        24 => Ev::Recoil { player: r.u8()?, kick: r.q()? },
        _ => return None,
    })
}

/// Client side: makes an event from the server happen here.
pub fn apply_event(w: &mut World, e: &Ev) {
    match e {
        Ev::Snd { id, pos, gain } => {
            if let Some(s) = Sfx::from_u8(*id) {
                w.snd(s, *pos, *gain);
            }
        }
        Ev::SndUi { id, gain } => {
            if let Some(s) = Sfx::from_u8(*id) {
                w.snd_ui(s, *gain);
            }
        }
        Ev::Msg { text, color, ttl, big } => w.announce(text, *color, *ttl, *big),
        Ev::Shake { amount } => w.trauma = (w.trauma + amount).min(1.0),
        Ev::Blast { pos, radius, force } => w.ragdolls.blast(*pos, *radius, *force),
        Ev::ZombieDied { id, dir, point, force, gore, flags } => {
            if let Some(i) = w.zombies.iter().position(|z| z.id == *id) {
                let mut z = w.zombies.swap_remove(i);
                z.last_dir = *dir;
                z.last_point = *point;
                z.last_force = *force;
                z.last_gore = *gore;
                z.last_crit = flags & 1 != 0;
                z.last_blast = flags & 2 != 0;
                z.last_cold = flags & 4 != 0;
                z.burn = if flags & 8 != 0 { 1.0 } else { 0.0 };
                w.zombie_death_visual(&z);
            }
        }
        Ev::Severed { id, mask } => {
            if let Some(i) = w.zombies.iter().position(|z| z.id == *id) {
                let m = *mask;
                if matches!(m, crate::ragdoll::MASK_L_ARM | crate::ragdoll::MASK_R_ARM | crate::ragdoll::MASK_L_LEG | crate::ragdoll::MASK_R_LEG) {
                    w.sever_visual(i, m);
                }
            }
        }
        Ev::PlayerDied { player, away, cause } => {
            if (*player as usize) < w.players.len() {
                w.player_death_visual(*player as usize, *away, super::player::DeathCause::from_u8(*cause));
            }
        }
        Ev::PlayerDown { player, from, force } => {
            let i = *player as usize;
            if i < w.players.len() && w.players[i].alive {
                w.players[i].down_t = 1.0;
                w.knockdown_visual(i, *from, *force);
            }
        }
        Ev::CorpseRose { player } => {
            let i = *player as i16;
            w.ragdolls.bodies.retain(|b| b.owner != i);
        }
        Ev::Recoil { player, kick } => {
            if *player as usize == w.local {
                w.trauma = (w.trauma + kick).min(1.0);
            }
        }
        other => w.fx.replay(other),
    }
}

// ------------------------------------------------------------------- snapshots
fn angle_of(v: Vec2) -> u8 {
    ((v.y.atan2(v.x) / std::f32::consts::TAU).rem_euclid(1.0) * 255.0) as u8
}

fn vec_of(a: u8) -> Vec2 {
    let t = a as f32 / 255.0 * std::f32::consts::TAU;
    Vec2::new(t.cos(), t.sin())
}

fn write_snapshot(world: &World, player: usize, complete: bool, known: &mut HashSet<u32>, events: &mut Vec<Ev>, clock: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(S_SNAPSHOT);
    w.f32(clock);

    // Events go first: a death must be applied while its zombie is still known.
    // Everything that matters, then cosmetic ones while there is room.
    let count_at = w.buf.len();
    w.u16(0);
    let start = w.buf.len();
    let mut sent = 0u16;
    for e in events.iter().filter(|e| !e.is_chaff()) {
        if sent == u16::MAX {
            break;
        }
        write_ev(&mut w, e);
        sent += 1;
    }
    for e in events.iter().filter(|e| e.is_chaff()) {
        if w.buf.len() - start > EVENT_BUDGET || sent == u16::MAX {
            break;
        }
        write_ev(&mut w, e);
        sent += 1;
    }
    w.buf[count_at..count_at + 2].copy_from_slice(&sent.to_le_bytes());

    w.f32(world.time);
    w.u16(world.wave as u16);
    match world.phase {
        Phase::Build { timer } => {
            w.u8(0);
            w.f32(timer);
        }
        Phase::Assault => {
            w.u8(1);
            w.f32(0.0);
        }
    }
    w.f32(world.spawn_budget);
    w.u32(world.scrap);
    w.u16(world.combo.min(65535) as u16);
    w.unit(world.night);
    let flags = u8::from(world.game_over) | u8::from(world.hold_timer) << 1 | u8::from(world.boss_track) << 2 | u8::from(world.permadeath) << 3 | u8::from(world.core_alert > 0.0) << 4;
    w.u8(flags);
    let st = &world.stats;
    for v in [st.kills, st.headshots, st.best_combo, st.built, st.wave_reached, st.scrap_earned, st.deaths] {
        w.u32(v);
    }
    w.f32(st.time);

    // Players.
    w.u8(world.players.len() as u8);
    for (i, p) in world.players.iter().enumerate() {
        let f = u8::from(p.alive) | u8::from(p.connected) << 1 | u8::from(p.out) << 2 | u8::from(p.roll_t > 0.0) << 3 | u8::from(p.on_depot) << 4 | u8::from(p.down_t > 0.0) << 5;
        w.u8(f);
        w.q2(p.pos2());
        w.q2(p.vel);
        w.u8(angle_of(p.facing));
        w.q(p.hp);
        w.u8(p.weapon as u8);
        w.unit(p.recoil);
        w.unit(p.spin);
        w.q(p.reload);
        w.q(p.respawn);
        w.unit(p.hurt);
        w.u8(p.respawn_seq);
        if i == player {
            let mut owned = 0u32;
            for k in 0..18 {
                owned |= (p.owned[k] as u32) << k;
                w.u16(p.mag[k].min(65535) as u16);
                w.u16(p.reserve[k].min(65535) as u16);
            }
            w.u32(owned);
            w.u8(p.grenades.min(255) as u8);
            w.unit(p.spread);
        }
    }

    // Structures: small enough to send whole, and it handles every kind of change.
    w.u16(world.structures.len() as u16);
    for s in &world.structures {
        w.u8(s.kind as u8);
        w.u8(s.tile.0 as u8);
        w.u8(s.tile.1 as u8);
        w.unit(s.hp / s.max_hp);
        w.u8(angle_of(Vec2::new(s.yaw.cos(), s.yaw.sin())));
        w.u8(s.level as u8 | u8::from(s.recoil > 0.5) << 2 | u8::from(s.flash > 0.5) << 3);
    }

    // Zombies.
    let focus = world.players.get(player).map_or(Vec3::ZERO, |p| p.pos);
    w.bool(complete);
    let count_at = w.buf.len();
    w.u16(0);
    let mut n = 0u16;
    for z in &world.zombies {
        if !complete && z.pos.distance_squared(focus) > NEAR_RANGE2 {
            continue;
        }
        n += 1;
        let new = known.insert(z.id);
        w.u32(z.id);
        let bits = u8::from(z.attack_t > 0.0)
            | u8::from(z.burn > 0.0) << 1
            | u8::from(z.rage > 0.0) << 2
            | u8::from(z.flash > 0.3) << 3
            | u8::from(z.crawling) << 4
            | u8::from(new) << 7;
        w.u8(bits);
        w.q2(z.pos2());
        w.u8(((z.vel.x * 8.0).clamp(-127.0, 127.0) as i8) as u8);
        w.u8(((z.vel.y * 8.0).clamp(-127.0, 127.0) as i8) as u8);
        w.u8(angle_of(z.facing));
        w.unit(z.hp / z.max_hp);
        w.unit(z.frost / 2.5);
        w.unit(z.spawn_t);
        w.u8(missing_bits(z.missing));
        if new {
            w.u8(z.kind as u8);
            w.u32(z.style);
            w.f32(z.scale);
            w.f32(z.radius);
        }
    }
    w.buf[count_at..count_at + 2].copy_from_slice(&n.to_le_bytes());

    // Projectiles and pickups: visual stand-ins on the client.
    let shots = world.projectiles.len().min(120);
    w.u8(shots as u8);
    for p in world.projectiles.iter().take(shots) {
        w.u8(p.kind as u8);
        w.q3(p.pos);
        w.q3(p.vel);
    }
    let near_first: Vec<&Pickup> = {
        let mut v: Vec<&Pickup> = world.pickups.iter().collect();
        if v.len() > 160 {
            v.sort_by(|a, b| a.pos.distance_squared(focus).total_cmp(&b.pos.distance_squared(focus)));
            v.truncate(160);
        }
        v
    };
    w.u8(near_first.len() as u8);
    for p in near_first {
        w.u8(p.kind);
        w.q3(p.pos);
    }
    w.u8(world.barrels.len().min(255) as u8);
    for b in world.barrels.iter().take(255) {
        w.q2(Vec2::new(b.pos.x, b.pos.z));
        w.bool(b.burn > 0.0);
    }

    events.clear();
    w.buf
}

fn missing_bits(m: u16) -> u8 {
    use crate::ragdoll::*;
    u8::from(m & MASK_L_ARM != 0) | u8::from(m & MASK_R_ARM != 0) << 1 | u8::from(m & MASK_L_LEG != 0) << 2 | u8::from(m & MASK_R_LEG != 0) << 3
}

fn missing_mask(b: u8) -> u16 {
    use crate::ragdoll::*;
    let mut m = 0;
    if b & 1 != 0 {
        m |= MASK_L_ARM;
    }
    if b & 2 != 0 {
        m |= MASK_R_ARM;
    }
    if b & 4 != 0 {
        m |= MASK_L_LEG;
    }
    if b & 8 != 0 {
        m |= MASK_R_LEG;
    }
    m
}

/// Applies the entity part of a snapshot. `None` means it was malformed.
fn apply_snapshot(w: &mut World, r: &mut Reader) -> Option<()> {
    let time = r.f32()?;
    if (w.time - time).abs() > 0.5 {
        w.time = time;
    }
    w.wave = r.u16()? as u32;
    let (ph, t) = (r.u8()?, r.f32()?);
    w.phase = if ph == 0 { Phase::Build { timer: t } } else { Phase::Assault };
    w.spawn_budget = r.f32()?;
    w.scrap = r.u32()?;
    w.combo = r.u16()? as u32;
    w.night = r.unit()?;
    let f = r.u8()?;
    w.game_over = f & 1 != 0;
    w.hold_timer = f & 2 != 0;
    w.boss_track = f & 4 != 0;
    w.permadeath = f & 8 != 0;
    if f & 16 != 0 {
        w.core_alert = 1.0;
    }
    let st = &mut w.stats;
    st.kills = r.u32()?;
    st.headshots = r.u32()?;
    st.best_combo = r.u32()?;
    st.built = r.u32()?;
    st.wave_reached = r.u32()?;
    st.scrap_earned = r.u32()?;
    st.deaths = r.u32()?;
    st.time = r.f32()?;

    // Players.
    let n = r.u8()? as usize;
    while w.players.len() < n {
        let i = w.players.len();
        let p = Player::new("", i);
        w.players.push(p);
    }
    w.players.truncate(n.max(w.local + 1));
    let local = w.local;
    for i in 0..n {
        let f = r.u8()?;
        let pos = r.q2()?;
        let vel = r.q2()?;
        let face = vec_of(r.u8()?);
        let hp = r.q()?;
        let weapon = (r.u8()? as usize).min(WEAPONS.len() - 1);
        let recoil = r.unit()?;
        let spin = r.unit()?;
        let reload = r.q()?;
        let respawn = r.q()?;
        let hurt = r.unit()?;
        let seq = r.u8()?;
        let p = &mut w.players[i];
        let was_alive = p.alive;
        p.alive = f & 1 != 0;
        p.connected = f & 2 != 0;
        p.out = f & 4 != 0;
        p.hp = hp;
        p.weapon = weapon;
        p.spin = spin;
        p.reload = reload;
        p.respawn = respawn;
        p.hurt = p.hurt.max(hurt);
        p.net_target = pos;
        let down = f & 32 != 0;
        let was_down = p.down_t > 0.0;
        if !down && was_down {
            w.finish_knockdown(i);
        }
        // A body whose owner is back on their feet elsewhere is just a corpse now.
        if p_alive_again(was_alive, f) {
            w.ragdolls.bodies.iter_mut().filter(|b| b.owner == i as i16).for_each(|b| b.owner = -1);
        }
        let p = &mut w.players[i];
        if down {
            p.down_t = p.down_t.max(1.0);
        }
        if i == local {
            // The client owns its own position, except when the server has just placed it.
            let placed = seq != p.respawn_seq || (p.alive && !was_alive);
            if placed && !down || !p.alive && p.death_t <= 0.0 {
                p.pos.x = pos.x;
                p.pos.z = pos.y;
                p.vel = Vec2::ZERO;
            }
            p.respawn_seq = seq;
            p.on_depot = f & 16 != 0;
            let mut owned_mag = [(0u16, 0u16); 18];
            for m in &mut owned_mag {
                *m = (r.u16()?, r.u16()?);
            }
            let owned = r.u32()?;
            let grenades = r.u8()?;
            let spread = r.unit()?;
            let p = &mut w.players[i];
            for k in 0..18 {
                p.owned[k] = owned & (1 << k) != 0;
                p.mag[k] = owned_mag[k].0 as u32;
                p.reserve[k] = if WEAPONS[k].reserve == u32::MAX { u32::MAX } else { owned_mag[k].1 as u32 };
            }
            p.grenades = grenades as u32;
            p.spread = spread;
        } else {
            p.recoil = recoil;
            p.vel = vel;
            p.facing = face;
            p.roll_t = if f & 8 != 0 { 0.1 } else { 0.0 };
            if (!was_alive || p.pos2().distance(pos) > 6.0) && !down {
                p.pos.x = pos.x;
                p.pos.z = pos.y;
            }
        }
    }

    // Structures.
    let n = r.u16()? as usize;
    let mut changed = n != w.structures.len();
    let mut next: Vec<Structure> = Vec::with_capacity(n);
    for i in 0..n {
        let kind = SKind::from_u8(r.u8()?)?;
        let tile = (r.u8()? as i32, r.u8()? as i32);
        tile_index(tile)?;
        let hp = r.unit()?;
        let yaw_v = vec_of(r.u8()?);
        let b = r.u8()?;
        let level = (b & 3).min(2) as usize;
        let same = w.structures.get(i).is_some_and(|s| s.kind == kind && s.tile == tile);
        changed |= !same;
        let mut s = if same { w.structures[i].clone_light() } else { new_client_structure(w, kind, tile) };
        s.level = level;
        s.max_hp = sdef(kind).hp * 1.35f32.powi(level as i32);
        s.hp = hp * s.max_hp;
        s.yaw = yaw_v.y.atan2(yaw_v.x);
        if b & 4 != 0 {
            s.recoil = 1.0;
        }
        if b & 8 != 0 {
            s.flash = 1.0;
        }
        next.push(s);
    }
    w.structures = next;
    if changed {
        w.rebuild_tiles();
    }

    // Zombies.
    let complete = r.bool()?;
    let n = r.u16()? as usize;
    let index: HashMap<u32, usize> = w.zombies.iter().enumerate().map(|(i, z)| (z.id, i)).collect();
    let mut seen: HashSet<u32> = HashSet::with_capacity(n);
    for _ in 0..n {
        let id = r.u32()?;
        let bits = r.u8()?;
        let pos = r.q2()?;
        let vx = r.u8()? as i8 as f32 / 8.0;
        let vz = r.u8()? as i8 as f32 / 8.0;
        let face = vec_of(r.u8()?);
        let hp = r.unit()?;
        let frost = r.unit()? * 2.5;
        let spawn = r.unit()?;
        let missing = missing_mask(r.u8()?);
        let full = if bits & 0x80 != 0 { Some((ZKind::from_u8(r.u8()?)?, r.u32()?, r.f32()?, r.f32()?)) } else { None };
        seen.insert(id);
        let idx = match index.get(&id) {
            Some(&i) => i,
            None => {
                let Some((kind, ..)) = full else { continue };
                w.spawn_zombie(kind, pos);
                let z = w.zombies.last_mut()?;
                z.id = id;
                z.pos.x = pos.x;
                z.pos.z = pos.y;
                w.zombies.len() - 1
            }
        };
        let z = &mut w.zombies[idx];
        if let Some((kind, style, scale, radius)) = full {
            z.kind = kind;
            z.style = style;
            z.scale = scale;
            z.radius = radius;
        }
        z.net_target = pos;
        z.vel = Vec2::new(vx, vz);
        z.facing = face;
        z.hp = hp;
        z.max_hp = 1.0;
        z.frost = frost;
        z.spawn_t = spawn;
        z.missing = missing;
        z.crawling = bits & 16 != 0;
        z.attack_t = if bits & 1 != 0 { z.attack_t.max(0.01) } else { 0.0 };
        z.burn = if bits & 2 != 0 { 1.0 } else { 0.0 };
        z.rage = if bits & 4 != 0 { 1.0 } else { 0.0 };
        if bits & 8 != 0 {
            z.flash = 1.0;
        }
    }
    if complete {
        w.zombies.retain(|z| seen.contains(&z.id));
    }

    // Projectiles and pickups.
    let n = r.u8()? as usize;
    w.projectiles.clear();
    for _ in 0..n {
        let kind = ProjKind::from_u8(r.u8()?)?;
        w.projectiles.push(Projectile { kind, pos: r.q3()?, vel: r.q3()?, damage: 0.0, radius: 0.0, ttl: 1.0, bounces: 0, spin: 0.0, hit: Vec::new(), fx_t: 0.0 });
    }
    let n = r.u8()? as usize;
    let spins: Vec<f32> = w.pickups.iter().map(|p| p.spin).collect();
    w.pickups.clear();
    for i in 0..n {
        let kind = r.u8()?;
        let pos = r.q3()?;
        w.pickups.push(Pickup { kind, pos, vel: Vec3::ZERO, value: 0, ttl: 1.0, spin: spins.get(i).copied().unwrap_or(0.0) });
    }
    let n = r.u8()? as usize;
    w.barrels.clear();
    for _ in 0..n {
        let p = r.q2()?;
        let burn = r.bool()?;
        w.barrels.push(Barrel { pos: vec3(p.x, 0.0, p.y), hp: 30.0, burn: if burn { 1.0 } else { 0.0 } });
    }
    Some(())
}

fn p_alive_again(was_alive: bool, flags: u8) -> bool {
    !was_alive && flags & 1 != 0
}

fn new_client_structure(w: &mut World, kind: SKind, tile: (i32, i32)) -> Structure {
    let mut s = w.make_structure_for_load(kind, tile);
    s.pos = if kind == SKind::Core {
        Vec3::ZERO
    } else {
        let c = super::tile_center(tile);
        vec3(c.x, 0.0, c.y)
    };
    s
}

/// One frame of the client's half of the simulation.
pub fn update_client(w: &mut World, dt: f32, input: &PlayerInput) {
    w.time += dt;
    w.sfx_budget = [0; crate::audio::sfx::SFX_COUNT];
    w.trauma = (w.trauma - dt * 1.3).max(0.0);
    w.core_alert = (w.core_alert - dt).max(0.0);
    for m in &mut w.messages {
        m.ttl -= dt;
    }
    w.messages.retain(|m| m.ttl > 0.0);
    if w.game_over {
        w.game_over_timer += dt;
    }
    let local = w.local;
    for (i, p) in w.players.iter_mut().enumerate() {
        p.recoil = (p.recoil - dt * 4.0).max(0.0);
        p.hurt = (p.hurt - dt * 2.0).max(0.0);
        p.getup_t = (p.getup_t - dt).max(0.0);
        if i != local && p.down_t <= 0.0 {
            let cur = p.pos2();
            let next = cur + (p.net_target + p.vel * 0.03 - cur) * (dt * 12.0).min(1.0);
            p.walk += (next - cur).length() * 1.55;
            p.pos.x = next.x;
            p.pos.z = next.y;
        }
    }
    if local < w.players.len() {
        if input.toggle_light {
            w.players[local].flashlight = !w.players[local].flashlight;
        }
        let busy = w.players[local].down_t > 0.0 || w.players[local].getup_t > 0.0;
        if w.players[local].alive && !busy {
            let p = &mut w.players[local];
            let aim2 = Vec2::new(input.aim.x - p.pos.x, input.aim.z - p.pos.z);
            if aim2.length_squared() > 0.04 {
                p.facing = aim2.normalize();
            }
            p.aim = input.aim;
            w.player_move(local, dt, input);
        }
    }
    for z in &mut w.zombies {
        z.flash = (z.flash - dt * 6.0).max(0.0);
        z.spawn_t = (z.spawn_t - dt).max(0.0);
        if z.attack_t > 0.0 {
            z.attack_t += dt;
        }
        let cur = z.pos2();
        z.net_target += z.vel * dt;
        let gap = z.net_target - cur;
        let next = if gap.length_squared() > 25.0 { z.net_target } else { cur + gap * (dt * 10.0).min(1.0) };
        let mv = (next - cur).length();
        z.phase += mv * if z.kind == ZKind::Runner { 1.7 } else { 2.6 } / z.scale;
        z.pos.x = next.x;
        z.pos.z = next.y;
    }
    for s in &mut w.structures {
        s.flash = (s.flash - dt * 4.0).max(0.0);
        s.recoil = (s.recoil - dt * 6.0).max(0.0);
        s.spawn_anim = (s.spawn_anim - dt * 3.0).max(0.0);
    }
    for p in &mut w.projectiles {
        p.spin += dt * 20.0;
        match p.kind {
            ProjKind::Grenade | ProjKind::Shell => p.vel.y -= 16.0 * dt,
            ProjKind::Spit => p.vel.y -= 14.0 * dt,
            _ => {}
        }
        p.pos += p.vel * dt;
        p.pos.y = p.pos.y.max(0.1);
    }
    for p in &mut w.pickups {
        p.spin += dt * 4.0;
    }
    {
        let collider = super::map::WorldCollider { map: &w.map, blocked_cells: &w.blocked_tiles };
        w.ragdolls.step(dt, &collider);
    }
    w.track_player_bodies(dt);
    w.bleed_ragdolls();
    w.fx.update(dt);
    w.update_music();
}

// ------------------------------------------------------------------- framed sockets
struct Link {
    stream: TcpStream,
    inbuf: Vec<u8>,
    outbuf: Vec<u8>,
    closed: bool,
    max_frame: usize,
    pub sent: u64,
}

impl Link {
    fn new(stream: TcpStream, max_frame: usize) -> std::io::Result<Link> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Link { stream, inbuf: Vec::with_capacity(8192), outbuf: Vec::with_capacity(8192), closed: false, max_frame, sent: 0 })
    }

    fn pull(&mut self) {
        let mut chunk = [0u8; 16384];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.closed = true;
                    return;
                }
                Ok(n) => {
                    self.inbuf.extend_from_slice(&chunk[..n]);
                    // A peer flooding us is cut off rather than buffered forever.
                    if self.inbuf.len() > self.max_frame * 2 + 4 {
                        self.closed = true;
                        return;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.closed = true;
                    return;
                }
            }
        }
    }

    fn next_frame(&mut self) -> Option<Vec<u8>> {
        if self.inbuf.len() < 4 {
            return None;
        }
        let n = u32::from_le_bytes([self.inbuf[0], self.inbuf[1], self.inbuf[2], self.inbuf[3]]) as usize;
        if n > self.max_frame {
            self.closed = true;
            return None;
        }
        if self.inbuf.len() < n + 4 {
            return None;
        }
        let frame = self.inbuf[4..4 + n].to_vec();
        self.inbuf.drain(..4 + n);
        Some(frame)
    }

    fn queue(&mut self, payload: &[u8]) {
        self.outbuf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        self.outbuf.extend_from_slice(payload);
    }

    fn flush(&mut self) {
        while !self.outbuf.is_empty() {
            match self.stream.write(&self.outbuf) {
                Ok(0) => {
                    self.closed = true;
                    return;
                }
                Ok(n) => {
                    self.sent += n as u64;
                    self.outbuf.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.closed = true;
                    return;
                }
            }
        }
    }
}

fn clean_name(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(16).collect::<String>().to_ascii_uppercase();
    let n = n.trim().to_string();
    if n.is_empty() {
        "SURVIVOR".to_string()
    } else {
        n
    }
}

// ------------------------------------------------------------------- server
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub name: String,
    pub port: u16,
    pub max_players: u8,
    pub password: String,
    pub permadeath: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { name: "ROTFALL SERVER".into(), port: DEFAULT_PORT, max_players: 4, password: String::new(), permadeath: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RosterEntry {
    pub name: String,
    pub connected: bool,
    pub ping_ms: u16,
    pub is_host: bool,
    pub index: u8,
}

struct Conn {
    link: Link,
    player: Option<usize>,
    input: PlayerInput,
    known: HashSet<u32>,
    events: Vec<Ev>,
    last_heard: Instant,
    created: Instant,
    ping_ms: u16,
    addr: String,
}

pub struct Server {
    listener: TcpListener,
    conns: Vec<Conn>,
    pub cfg: ServerConfig,
    clock: Instant,
    snap_t: f32,
    snap_no: u32,
    roster_t: f32,
    beacon: Option<UdpSocket>,
    beacon_t: f32,
    banned: Vec<String>,
    pub log: Vec<String>,
    pub port: u16,
    rate_t: f32,
    rate_last: u64,
    /// Outgoing bytes per second, summed over all clients.
    pub rate: f32,
}

impl Server {
    pub fn start(cfg: ServerConfig, world: &mut World) -> std::io::Result<Server> {
        let listener = TcpListener::bind(("0.0.0.0", cfg.port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let beacon = UdpSocket::bind(("0.0.0.0", 0)).ok().and_then(|s| {
            s.set_nonblocking(true).ok()?;
            let _ = s.set_broadcast(true);
            Some(s)
        });
        world.fx.rec = Some(Vec::with_capacity(1024));
        world.permadeath = cfg.permadeath;
        let mut cfg = cfg;
        cfg.name = clean_name(&cfg.name);
        cfg.max_players = cfg.max_players.clamp(1, MAX_PLAYERS_LIMIT);
        Ok(Server {
            listener,
            conns: Vec::new(),
            cfg,
            clock: Instant::now(),
            snap_t: 0.0,
            snap_no: 0,
            roster_t: 0.0,
            beacon,
            beacon_t: 0.0,
            banned: Vec::new(),
            log: vec![format!("LISTENING ON PORT {port}")],
            port,
            rate_t: 0.0,
            rate_last: 0,
            rate: 0.0,
        })
    }

    fn note(&mut self, s: String) {
        eprintln!("server: {s}");
        self.log.push(s);
        if self.log.len() > 8 {
            self.log.remove(0);
        }
    }

    /// Handles new connections and everything clients have sent.
    pub fn poll(&mut self, world: &mut World) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    if self.conns.len() >= MAX_PLAYERS_LIMIT as usize * 2 {
                        continue; // Too many half-open connections: just drop this one.
                    }
                    if let Ok(link) = Link::new(stream, MAX_CLIENT_FRAME) {
                        self.conns.push(Conn {
                            link,
                            player: None,
                            input: PlayerInput::default(),
                            known: HashSet::new(),
                            events: Vec::new(),
                            last_heard: Instant::now(),
                            created: Instant::now(),
                            ping_ms: 0,
                            addr: addr.ip().to_string(),
                        });
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        for ci in 0..self.conns.len() {
            self.conns[ci].link.pull();
            while let Some(frame) = self.conns[ci].link.next_frame() {
                self.conns[ci].last_heard = Instant::now();
                if self.handle(ci, &frame, world).is_none() {
                    // A malformed message means a broken or hostile client.
                    self.conns[ci].link.closed = true;
                    break;
                }
            }
        }
        // Drop dead connections, leaving their player slots open for a reconnect.
        let mut i = 0;
        while i < self.conns.len() {
            let c = &self.conns[i];
            let timed_out = c.last_heard.elapsed().as_secs_f32() > TIMEOUT || (c.player.is_none() && c.created.elapsed().as_secs_f32() > HANDSHAKE_TIMEOUT);
            if c.link.closed || timed_out {
                let c = self.conns.swap_remove(i);
                if let Some(p) = c.player {
                    if let Some(pl) = world.players.get_mut(p) {
                        pl.connected = false;
                        let name = pl.name.clone();
                        world.nav_dirty = true;
                        world.announce(&format!("{name} LEFT"), [0.9, 0.7, 0.3], 3.0, false);
                        self.note(format!("{name} DISCONNECTED"));
                    }
                    self.roster_t = 0.0;
                }
            } else {
                i += 1;
            }
        }
    }

    fn reject(&mut self, ci: usize, reason: &str) {
        let mut w = Writer::new();
        w.u8(S_REJECT);
        w.str(reason);
        let c = &mut self.conns[ci];
        c.link.queue(&w.buf);
        c.link.flush();
        c.link.closed = true;
    }

    fn handle(&mut self, ci: usize, frame: &[u8], world: &mut World) -> Option<()> {
        let mut r = Reader::new(frame);
        match r.u8()? {
            C_HELLO => {
                if self.conns[ci].player.is_some() {
                    return None;
                }
                let version = r.u16()?;
                let name = clean_name(&r.str()?);
                let password = r.str()?;
                if version != PROTOCOL {
                    self.reject(ci, "VERSION MISMATCH");
                    return Some(());
                }
                if password != self.cfg.password {
                    self.reject(ci, "WRONG PASSWORD");
                    return Some(());
                }
                if self.banned.contains(&name) {
                    self.reject(ci, "KICKED FROM THIS SERVER");
                    return Some(());
                }
                let online = world.players.iter().filter(|p| p.connected).count();
                if online >= self.cfg.max_players as usize {
                    self.reject(ci, "SERVER FULL");
                    return Some(());
                }
                // A returning name reclaims its slot and inventory.
                let idx = match world.players.iter().position(|p| p.name == name && !p.connected) {
                    Some(i) => i,
                    None => {
                        if world.players.iter().any(|p| p.name == name) {
                            self.reject(ci, "THAT NAME IS TAKEN");
                            return Some(());
                        }
                        if world.players.len() >= u8::MAX as usize {
                            self.reject(ci, "SERVER FULL");
                            return Some(());
                        }
                        let i = world.players.len();
                        world.players.push(Player::new(&name, i));
                        i
                    }
                };
                let p = &mut world.players[idx];
                p.connected = true;
                p.remote = true;
                let at = Player::spawn_point(idx);
                p.pos = at;
                p.vel = Vec2::ZERO;
                p.respawn_seq = p.respawn_seq.wrapping_add(1);
                world.nav_dirty = true;
                world.announce(&format!("{name} JOINED"), [0.5, 1.0, 0.6], 3.0, false);
                let addr = self.conns[ci].addr.clone();
                self.note(format!("{name} JOINED FROM {addr}"));
                self.conns[ci].player = Some(idx);
                let mut w = Writer::new();
                w.u8(S_WELCOME);
                w.u8(idx as u8);
                w.u64(world.seed);
                w.bool(world.permadeath);
                w.str(&self.cfg.name);
                self.conns[ci].link.queue(&w.buf);
                self.roster_t = 0.0;
            }
            C_INPUT => {
                let idx = self.conns[ci].player?;
                let echo = r.f32()?;
                let seq = r.u8()?;
                let pos = r.q2()?;
                let vel = r.q2()?;
                let aim = r.q3()?;
                let f = r.u8()?;
                let select = r.u8()?;
                let cycle = r.u8()? as i8 as i32;
                let c = &mut self.conns[ci];
                let rtt = self.clock.elapsed().as_secs_f32() - echo;
                if (0.0..30.0).contains(&rtt) {
                    c.ping_ms = (rtt * 1000.0) as u16;
                }
                let p = world.players.get_mut(idx)?;
                // Ignore stale positions sent before the client heard about a respawn.
                if p.alive && p.down_t <= 0.0 && p.getup_t <= 0.0 && seq == p.respawn_seq {
                    let lim = super::map::HALF - 1.0;
                    let q = pos.clamp(Vec2::splat(-lim), Vec2::splat(lim));
                    p.pos.x = q.x;
                    p.pos.z = q.y;
                    p.vel = vel;
                    p.walk += vel.length() * INPUT_INTERVAL * 1.55;
                    p.roll_t = if f & 8 != 0 { 0.1 } else { 0.0 };
                }
                let i = &mut c.input;
                i.aim = aim;
                i.fire = f & 1 != 0;
                i.sprint = f & 2 != 0;
                // One-shot actions latch until the simulation has seen them.
                i.reload |= f & 4 != 0;
                i.grenade |= f & 16 != 0;
                if select != 255 {
                    i.select = Some(select as usize);
                }
                if cycle != 0 {
                    i.cycle = cycle;
                }
            }
            C_COMMAND => {
                let idx = self.conns[ci].player?;
                let cmd = Command::read(&mut r)?;
                if let Err(e) = world.apply_command(idx, cmd) {
                    let mut w = Writer::new();
                    w.u8(S_NOTE);
                    w.str(&e);
                    self.conns[ci].link.queue(&w.buf);
                }
            }
            _ => return None,
        }
        Some(())
    }

    /// Fills in network players' inputs and clears their one-shot flags.
    pub fn take_inputs(&mut self, inputs: &mut Vec<PlayerInput>, world: &World) {
        inputs.resize(world.players.len(), PlayerInput::default());
        for c in &mut self.conns {
            if let Some(p) = c.player {
                if p < inputs.len() {
                    inputs[p] = c.input;
                    c.input.reload = false;
                    c.input.grenade = false;
                    c.input.select = None;
                    c.input.cycle = 0;
                }
            }
        }
    }

    /// Call after the world has stepped: distributes events and sends what is due.
    pub fn after_update(&mut self, world: &mut World, dt: f32) {
        let new = world.fx.rec.as_mut().map(std::mem::take).unwrap_or_default();
        for c in &mut self.conns {
            if c.player.is_none() {
                continue;
            }
            for e in &new {
                if let Ev::ZombieDied { id, .. } = e {
                    c.known.remove(id);
                }
            }
            c.events.extend_from_slice(&new);
            // A stalled client sheds cosmetics rather than growing without limit.
            if c.events.len() > 6000 {
                c.events.retain(|e| !e.is_chaff());
            }
        }
        self.snap_t -= dt;
        if self.snap_t <= 0.0 {
            self.snap_t += SNAP_INTERVAL;
            if self.snap_t < 0.0 {
                self.snap_t = SNAP_INTERVAL;
            }
            self.snap_no += 1;
            let complete = self.snap_no % COMPLETE_EVERY == 0;
            let clock = self.clock.elapsed().as_secs_f32();
            for c in &mut self.conns {
                let Some(p) = c.player else { continue };
                // Only send when the last one has left: slow links skip frames, not fall behind.
                if c.link.outbuf.len() > 64 * 1024 {
                    continue;
                }
                let snap = write_snapshot(world, p, complete, &mut c.known, &mut c.events, clock);
                c.link.queue(&snap);
            }
        }
        self.roster_t -= dt;
        if self.roster_t <= 0.0 {
            self.roster_t = 1.0;
            let mut w = Writer::new();
            w.u8(S_ROSTER);
            let roster = self.roster(world);
            w.u8(self.cfg.max_players);
            w.u8(roster.len() as u8);
            for e in &roster {
                w.str(&e.name);
                w.bool(e.connected);
                w.u16(e.ping_ms);
                w.bool(e.is_host);
                w.u8(e.index);
            }
            for c in &mut self.conns {
                if c.player.is_some() {
                    c.link.queue(&w.buf);
                }
            }
        }
        for c in &mut self.conns {
            c.link.flush();
        }
        self.beacon_t -= dt;
        if self.beacon_t <= 0.0 {
            self.beacon_t = 1.0;
            if let Some(b) = &self.beacon {
                let online = world.players.iter().filter(|p| p.connected).count();
                let msg = format!(
                    "ROTFALL|{}|{}|{}|{}|{}|{}|{}|{}",
                    PROTOCOL, self.port, online, self.cfg.max_players, world.wave,
                    u8::from(self.cfg.permadeath), u8::from(!self.cfg.password.is_empty()), self.cfg.name
                );
                let _ = b.send_to(msg.as_bytes(), ("255.255.255.255", BEACON_PORT));
                let _ = b.send_to(msg.as_bytes(), ("127.0.0.1", BEACON_PORT));
            }
        }
        self.rate_t += dt;
        if self.rate_t >= 1.0 {
            let total: u64 = self.conns.iter().map(|c| c.link.sent).sum();
            self.rate = total.saturating_sub(self.rate_last) as f32 / self.rate_t;
            self.rate_last = total;
            self.rate_t = 0.0;
        }
    }

    pub fn roster(&self, world: &World) -> Vec<RosterEntry> {
        world.players.iter().enumerate().map(|(i, p)| {
            let conn = self.conns.iter().find(|c| c.player == Some(i));
            RosterEntry {
                name: p.name.clone(),
                connected: p.connected,
                ping_ms: conn.map_or(0, |c| c.ping_ms),
                is_host: i == world.local,
                index: i as u8,
            }
        }).collect()
    }

    /// Removes a player for the rest of this server's life.
    pub fn kick(&mut self, player: usize, world: &mut World) -> bool {
        let Some(name) = world.players.get(player).map(|p| p.name.clone()) else { return false };
        let Some(ci) = self.conns.iter().position(|c| c.player == Some(player)) else { return false };
        self.banned.push(name.clone());
        self.reject(ci, "KICKED BY THE HOST");
        self.note(format!("KICKED {name}"));
        true
    }
}

/// The address to share with people on the same network.
pub fn local_ip() -> Option<String> {
    static IP: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    IP.get_or_init(|| {
        // Connecting a UDP socket sends nothing; it just makes the OS choose a route.
        let s = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
        s.connect(("192.0.2.1", 9)).ok()?;
        Some(s.local_addr().ok()?.ip().to_string())
    })
    .clone()
}

// ------------------------------------------------------------------- client
#[derive(Debug, PartialEq, Clone)]
pub enum ClientState {
    Connecting,
    /// Connected; waiting for the server's welcome.
    Handshaking,
    Playing,
    Closed(String),
}

pub struct Welcome {
    pub index: usize,
    pub seed: u64,
    pub permadeath: bool,
    pub server_name: String,
}

pub struct Client {
    link: Option<Link>,
    connecting: Option<Receiver<Result<TcpStream, String>>>,
    pub state: ClientState,
    name: String,
    password: String,
    send_t: f32,
    pending: PlayerInput,
    last_clock: f32,
    pub roster: Vec<RosterEntry>,
    pub max_players: u8,
    pub server_name: String,
    /// A message from the server for this player alone, such as a refused purchase.
    pub note: Option<String>,
    last_heard: Instant,
}

impl Client {
    /// Begins connecting without blocking; `addr` may be `host` or `host:port`.
    pub fn connect(addr: &str, name: &str, password: &str) -> Client {
        let target = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{DEFAULT_PORT}") };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = (|| {
                let addrs: Vec<SocketAddr> = target.to_socket_addrs().map_err(|_| "ADDRESS NOT FOUND".to_string())?.collect();
                let mut last = "NO ADDRESS".to_string();
                for a in addrs {
                    match TcpStream::connect_timeout(&a, Duration::from_secs(5)) {
                        Ok(s) => return Ok(s),
                        Err(e) => last = match e.kind() {
                            ErrorKind::ConnectionRefused => "NOTHING IS LISTENING THERE".to_string(),
                            ErrorKind::TimedOut => "CONNECTION TIMED OUT".to_string(),
                            _ => "COULD NOT CONNECT".to_string(),
                        },
                    }
                }
                Err(last)
            })();
            let _ = tx.send(result);
        });
        Client {
            link: None,
            connecting: Some(rx),
            state: ClientState::Connecting,
            name: name.to_string(),
            password: password.to_string(),
            send_t: 0.0,
            pending: PlayerInput::default(),
            last_clock: 0.0,
            roster: Vec::new(),
            max_players: 0,
            server_name: String::new(),
            note: None,
            last_heard: Instant::now(),
        }
    }

    fn finish_connect(&mut self) {
        let Some(rx) = &self.connecting else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.connecting = None;
        match result.and_then(|s| Link::new(s, MAX_FRAME).map_err(|_| "COULD NOT CONNECT".to_string())) {
            Ok(mut link) => {
                let mut w = Writer::new();
                w.u8(C_HELLO);
                w.u16(PROTOCOL);
                w.str(&self.name);
                w.str(&self.password);
                link.queue(&w.buf);
                link.flush();
                self.link = Some(link);
                self.state = ClientState::Handshaking;
                self.last_heard = Instant::now();
            }
            Err(e) => self.state = ClientState::Closed(e),
        }
    }

    /// While handshaking: returns the welcome when it arrives.
    pub fn poll_welcome(&mut self) -> Option<Welcome> {
        self.finish_connect();
        if self.state != ClientState::Handshaking {
            return None;
        }
        let link = self.link.as_mut()?;
        link.pull();
        link.flush();
        let timed_out = self.last_heard.elapsed().as_secs_f32() > HANDSHAKE_TIMEOUT + 3.0;
        let frame = link.next_frame();
        let closed = link.closed;
        let Some(frame) = frame else {
            if closed || timed_out {
                self.state = ClientState::Closed("THE SERVER DID NOT ANSWER".into());
            }
            return None;
        };
        let mut r = Reader::new(&frame);
        match r.u8() {
            Some(S_WELCOME) => {
                let parsed = (|| Some(Welcome { index: r.u8()? as usize, seed: r.u64()?, permadeath: r.bool()?, server_name: r.str()? }))();
                match parsed {
                    Some(w) => {
                        self.server_name = w.server_name.clone();
                        self.state = ClientState::Playing;
                        self.last_heard = Instant::now();
                        Some(w)
                    }
                    None => {
                        self.state = ClientState::Closed("BAD REPLY FROM SERVER".into());
                        None
                    }
                }
            }
            Some(S_REJECT) => {
                self.state = ClientState::Closed(r.str().unwrap_or_else(|| "REFUSED".into()));
                None
            }
            _ => {
                self.state = ClientState::Closed("NOT A ROTFALL SERVER".into());
                None
            }
        }
    }

    /// Applies everything received from the server.
    pub fn poll(&mut self, world: &mut World) {
        let Some(link) = self.link.as_mut() else { return };
        link.pull();
        let mut bad = false;
        while let Some(frame) = link.next_frame() {
            self.last_heard = Instant::now();
            let mut r = Reader::new(&frame);
            match r.u8() {
                Some(S_SNAPSHOT) => {
                    match apply_snapshot_and_events(world, &mut r) {
                        Some(c) => self.last_clock = c,
                        None => bad = true,
                    }
                }
                Some(S_ROSTER) => {
                    let parsed = (|| {
                        let max = r.u8()?;
                        let n = r.u8()? as usize;
                        let mut v = Vec::with_capacity(n);
                        for _ in 0..n {
                            v.push(RosterEntry { name: r.str()?, connected: r.bool()?, ping_ms: r.u16()?, is_host: r.bool()?, index: r.u8()? });
                        }
                        Some((max, v))
                    })();
                    match parsed {
                        Some((max, v)) => {
                            for e in &v {
                                if let Some(p) = world.players.get_mut(e.index as usize) {
                                    p.name = e.name.clone();
                                }
                            }
                            self.max_players = max;
                            self.roster = v;
                        }
                        None => bad = true,
                    }
                }
                Some(S_NOTE) => self.note = r.str(),
                Some(S_REJECT) => {
                    let why = r.str().unwrap_or_else(|| "DISCONNECTED".into());
                    self.state = ClientState::Closed(why);
                    return;
                }
                _ => bad = true,
            }
            if bad {
                self.state = ClientState::Closed("BAD DATA FROM SERVER".into());
                return;
            }
        }
        if link.closed {
            self.state = ClientState::Closed("LOST CONNECTION TO THE SERVER".into());
        } else if self.last_heard.elapsed().as_secs_f32() > TIMEOUT {
            self.state = ClientState::Closed("THE SERVER STOPPED RESPONDING".into());
        }
    }

    /// Sends this frame's input, batching one-shot actions between packets.
    pub fn send_input(&mut self, input: &PlayerInput, world: &World, dt: f32) {
        let i = &mut self.pending;
        i.reload |= input.reload;
        i.grenade |= input.grenade;
        if input.select.is_some() {
            i.select = input.select;
        }
        if input.cycle != 0 {
            i.cycle = input.cycle;
        }
        self.send_t -= dt;
        if self.send_t > 0.0 {
            return;
        }
        self.send_t = INPUT_INTERVAL;
        let Some(link) = self.link.as_mut() else { return };
        let Some(p) = world.players.get(world.local) else { return };
        let mut w = Writer::new();
        w.u8(C_INPUT);
        w.f32(self.last_clock);
        w.u8(p.respawn_seq);
        w.q2(p.pos2());
        w.q2(p.vel);
        w.q3(input.aim);
        let flags = u8::from(input.fire) | u8::from(input.sprint) << 1 | u8::from(self.pending.reload) << 2 | u8::from(p.roll_t > 0.0) << 3 | u8::from(self.pending.grenade) << 4;
        w.u8(flags);
        w.u8(self.pending.select.map_or(255, |s| s.min(254) as u8));
        w.u8(self.pending.cycle.clamp(-1, 1) as i8 as u8);
        self.pending = PlayerInput::default();
        link.queue(&w.buf);
        link.flush();
    }

    pub fn send_command(&mut self, c: Command) {
        let Some(link) = self.link.as_mut() else { return };
        let mut w = Writer::new();
        w.u8(C_COMMAND);
        c.write(&mut w);
        link.queue(&w.buf);
        link.flush();
    }
}

fn apply_snapshot_and_events(world: &mut World, r: &mut Reader) -> Option<f32> {
    let clock = r.f32()?;
    let n = r.u16()? as usize;
    for _ in 0..n {
        let e = read_ev(r)?;
        apply_event(world, &e);
    }
    apply_snapshot(world, r)?;
    Some(clock)
}

// ------------------------------------------------------------------- LAN discovery
#[derive(Clone, Debug)]
pub struct LanServer {
    pub addr: String,
    pub name: String,
    pub players: u8,
    pub max: u8,
    pub wave: u32,
    pub permadeath: bool,
    pub password: bool,
    seen: Instant,
}

pub struct LanBrowser {
    sock: Option<UdpSocket>,
    pub servers: Vec<LanServer>,
}

impl LanBrowser {
    pub fn new() -> LanBrowser {
        let sock = UdpSocket::bind(("0.0.0.0", BEACON_PORT)).ok().and_then(|s| {
            s.set_nonblocking(true).ok()?;
            Some(s)
        });
        LanBrowser { sock, servers: Vec::new() }
    }

    pub fn poll(&mut self) {
        if let Some(s) = &self.sock {
            let mut buf = [0u8; 512];
            while let Ok((n, from)) = s.recv_from(&mut buf) {
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let parts: Vec<&str> = text.splitn(9, '|').collect();
                if parts.len() != 9 || parts[0] != "ROTFALL" || parts[1].parse() != Ok(PROTOCOL) {
                    continue;
                }
                let (Ok(port), Ok(players), Ok(max), Ok(wave)) = (parts[2].parse::<u16>(), parts[3].parse(), parts[4].parse(), parts[5].parse()) else { continue };
                let addr = format!("{}:{}", from.ip(), port);
                let entry = LanServer { addr: addr.clone(), name: parts[8].to_string(), players, max, wave, permadeath: parts[6] == "1", password: parts[7] == "1", seen: Instant::now() };
                match self.servers.iter_mut().find(|e| e.addr == addr) {
                    Some(e) => *e = entry,
                    None => self.servers.push(entry),
                }
            }
        }
        self.servers.retain(|e| e.seen.elapsed().as_secs_f32() < 4.0);
    }
}

/// Keeps a dedicated server's process alive, with a command console on stdin.
pub fn run_dedicated(cfg: ServerConfig, seed: u64, load: Option<std::path::PathBuf>, dir: std::path::PathBuf) {
    use super::save;
    let slot = save::Slot::for_mode(true, cfg.permadeath);
    let path = dir.join(slot.file());
    let audio = crate::audio::Audio::silent();
    let mut world = match load {
        Some(p) => {
            let bytes = std::fs::read(&p).unwrap_or_else(|e| {
                eprintln!("cannot read {}: {e}", p.display());
                std::process::exit(1)
            });
            let mut w = save::decode(&bytes, audio).unwrap_or_else(|e| {
                eprintln!("cannot load {}: {e}", p.display());
                std::process::exit(1)
            });
            println!("loaded {}: wave {}, {} saved players", p.display(), w.wave, w.players.len());
            for pl in &mut w.players {
                pl.connected = false;
                pl.remote = true;
            }
            w
        }
        None => {
            let mut w = World::new(seed, audio);
            w.players.clear();
            w.hold_timer = true;
            w.messages.clear();
            w
        }
    };
    world.local = usize::MAX;
    let mut server = Server::start(cfg.clone(), &mut world).unwrap_or_else(|e| {
        eprintln!("cannot listen on port {}: {e}", cfg.port);
        std::process::exit(1)
    });
    let ip = local_ip().unwrap_or_else(|| "this machine's address".into());
    println!("ROTFALL dedicated server \"{}\" on {}:{}  ({} players max{}{})", server.cfg.name, ip, server.port, server.cfg.max_players,
        if cfg.permadeath { ", permadeath" } else { "" }, if cfg.password.is_empty() { "" } else { ", password set" });
    println!("Commands: status | start | kick NAME | save | quit");
    let (tx, rx) = channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        while stdin.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
            if tx.send(line.trim().to_string()).is_err() {
                break;
            }
            line.clear();
        }
    });
    let dt = 1.0 / 60.0;
    let mut next = Instant::now();
    let mut inputs: Vec<PlayerInput> = Vec::new();
    let save_now = |w: &World| match save::write_atomic(&path, &save::encode(w)) {
        Ok(()) => println!("saved to {}", path.display()),
        Err(e) => eprintln!("save failed: {e}"),
    };
    loop {
        while let Ok(line) = rx.try_recv() {
            let (cmd, arg) = line.split_once(' ').unwrap_or((line.as_str(), ""));
            match cmd {
                "status" => {
                    println!("wave {} | {:?} | zombies {} | scrap {} | kills {} | {:.0} KB/s out", world.wave, world.phase, world.zombies.len(), world.scrap, world.stats.kills, server.rate / 1024.0);
                    for e in server.roster(&world) {
                        println!("  {:>2} {:<16} {} {} ms", e.index, e.name, if e.connected { "online " } else { "offline" }, e.ping_ms);
                    }
                }
                "start" => world.call_next_wave(),
                "kick" => {
                    let want = arg.trim().to_ascii_uppercase();
                    match world.players.iter().position(|p| p.name == want) {
                        Some(i) if server.kick(i, &mut world) => println!("kicked {want}"),
                        _ => println!("no connected player named {want}"),
                    }
                }
                "save" => save_now(&world),
                "quit" | "exit" => {
                    if !world.game_over {
                        save_now(&world);
                    }
                    return;
                }
                "" => {}
                other => println!("unknown command: {other}"),
            }
        }
        server.poll(&mut world);
        server.take_inputs(&mut inputs, &world);
        world.update_all(dt, &inputs);
        server.after_update(&mut world, dt);
        if std::mem::take(&mut world.autosave_due) {
            save_now(&world);
        }
        // Simulation stops when the server is empty, so that timer alone could wait forever.
        let empty = !world.players.iter().any(|p| p.connected);
        if world.game_over && (world.game_over_timer > 15.0 || empty) {
            if cfg.permadeath {
                crate::storage::remove(&path);
            }
            println!("game over at wave {}; starting a new match", world.wave);
            let mut w = World::new(world.seed.wrapping_add(1), crate::audio::Audio::silent());
            w.players.clear();
            w.hold_timer = true;
            w.messages.clear();
            w.local = usize::MAX;
            w.permadeath = cfg.permadeath;
            world = w;
            world.fx.rec = Some(Vec::new());
            // Clients must rebuild their map, so everyone reconnects to the new match.
            server.restart_clients();
        }
        next += Duration::from_secs_f32(dt);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}

impl Server {
    fn restart_clients(&mut self) {
        for ci in 0..self.conns.len() {
            self.reject(ci, "MATCH OVER - THE SERVER HAS RESTARTED");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Audio;
    use crate::game::bot::Bot;

    fn ev_samples() -> Vec<Ev> {
        let (a, b) = (vec3(1.0, 2.0, 3.0), vec3(-4.0, 0.5, 6.0));
        vec![
            Ev::Blood { pos: a, dir: b, amount: 7, speed: 3.0 },
            Ev::Splat { pos: a, size: 1.0 },
            Ev::Muzzle { pos: a, dir: b, size: 0.5, color: a },
            Ev::Tracer { from: a, to: b, color: [8.0, 6.0, 2.0], width: 0.02 },
            Ev::Spark { pos: a, vel: b, color: [1.0, 2.0, 3.0], life: 0.2 },
            Ev::Smoke { pos: a, vel: b, size: 1.0, life: 2.0, shade: 0.1, opacity: 0.5 },
            Ev::Fire { pos: a, vel: b, size: 0.3, life: 0.4 },
            Ev::Frost { pos: a, vel: b, size: 0.3, life: 0.4 },
            Ev::Lightning { from: a, to: b, color: [1.0, 2.0, 3.0], width: 0.04 },
            Ev::Explosion { pos: a, radius: 6.0 },
            Ev::Debris { pos: a, vel: b, mesh: 3, scale: a, tint: [1.0, 0.5, 0.2], life: 3.0, bleeds: true },
            Ev::Casing { pos: a, right: b },
            Ev::Gibs { pos: a, count: 9, speed: 4.0, tint: [0.3, 0.2, 0.1] },
            Ev::Decal { x: 1.0, z: 2.0, size: 3.0, color: [0.1, 0.2, 0.3, 0.4], kind: 2 },
            Ev::Light { pos: a, radius: 5.0, color: b, life: 0.1 },
            Ev::Glow { pos: a, vel: b, size: 0.1, grow: 0.2, life: 0.3, c0: [1.0, 2.0, 3.0, 0.5], c1: [0.1, 0.2, 0.3, 0.0] },
            Ev::Snd { id: 3, pos: a, gain: 0.5 },
            Ev::SndUi { id: 4, gain: 0.7 },
            Ev::Msg { text: "WAVE 3".into(), color: [1.0, 0.5, 0.0], ttl: 2.0, big: true },
            Ev::Shake { amount: 0.5 },
            Ev::Blast { pos: a, radius: 4.0, force: 12.0 },
            Ev::ZombieDied { id: 77, dir: b, point: a, force: 9.0, gore: 0.5, flags: 5 },
            Ev::Severed { id: 12, mask: crate::ragdoll::MASK_L_ARM },
            Ev::PlayerDied { player: 2, away: a, cause: 2 },
            Ev::PlayerDown { player: 1, from: b, force: 12.0 },
            Ev::CorpseRose { player: 3 },
            Ev::Recoil { player: 1, kick: 0.3 },
        ]
    }

    #[test]
    fn every_event_round_trips() {
        for e in ev_samples() {
            let mut w = Writer::new();
            write_ev(&mut w, &e);
            let mut r = Reader::new(&w.buf);
            let back = read_ev(&mut r).unwrap_or_else(|| panic!("decode failed: {e:?}"));
            assert_eq!(r.left(), 0, "{e:?} left bytes behind");
            // Values are quantized, so compare after a second encode.
            let mut w2 = Writer::new();
            write_ev(&mut w2, &back);
            assert_eq!(w.buf, w2.buf, "{e:?}");
        }
    }

    #[test]
    fn commands_round_trip() {
        for c in [Command::Build(3, (10, 20)), Command::Sell((1, 2)), Command::Upgrade((3, 4)), Command::Repair((5, 6)), Command::BuyWeapon(7), Command::BuyAmmo(8), Command::BuyGrenades, Command::NextWave] {
            let mut w = Writer::new();
            c.write(&mut w);
            assert_eq!(Command::read(&mut Reader::new(&w.buf)), Some(c));
        }
    }

    /// A host world plus a client world, connected over a real loopback socket.
    struct Rig {
        host: World,
        server: Server,
        clients: Vec<(World, Client)>,
        inputs: Vec<PlayerInput>,
    }

    impl Rig {
        fn new(cfg: ServerConfig) -> Rig {
            let mut host = World::new(77, Audio::silent());
            host.hold_timer = true;
            host.players[0].name = "HOST".into();
            let server = Server::start(ServerConfig { port: 0, ..cfg }, &mut host).unwrap();
            Rig { host, server, clients: Vec::new(), inputs: Vec::new() }
        }

        fn step(&mut self, host_input: PlayerInput, client_input: PlayerInput) {
            let dt = 1.0 / 60.0;
            self.server.poll(&mut self.host);
            self.server.take_inputs(&mut self.inputs, &self.host);
            self.inputs[0] = host_input;
            self.host.update_all(dt, &self.inputs);
            self.server.after_update(&mut self.host, dt);
            for (w, c) in &mut self.clients {
                if c.state == ClientState::Playing {
                    c.poll(w);
                    if c.state == ClientState::Playing {
                        c.send_input(&client_input, w, dt);
                        update_client(w, dt, &client_input);
                    }
                }
            }
        }

        fn run(&mut self, frames: u32) {
            for _ in 0..frames {
                self.step(PlayerInput::default(), PlayerInput::default());
                std::thread::sleep(Duration::from_micros(300));
            }
        }

        fn join(&mut self, name: &str, password: &str) -> Result<usize, String> {
            let mut c = Client::connect(&format!("127.0.0.1:{}", self.server.port), name, password);
            let start = Instant::now();
            let welcome = loop {
                self.server.poll(&mut self.host);
                self.server.after_update(&mut self.host, 0.01);
                if let Some(w) = c.poll_welcome() {
                    break w;
                }
                if let ClientState::Closed(why) = &c.state {
                    return Err(why.clone());
                }
                assert!(start.elapsed().as_secs() < 10, "join timed out in {:?}", c.state);
                std::thread::sleep(Duration::from_millis(1));
            };
            let mut w = World::new(welcome.seed, Audio::silent());
            w.is_client = true;
            w.local = welcome.index;
            while w.players.len() <= welcome.index {
                let i = w.players.len();
                w.players.push(Player::new("", i));
            }
            w.permadeath = welcome.permadeath;
            self.clients.push((w, c));
            Ok(welcome.index)
        }
    }

    #[test]
    fn a_client_joins_and_sees_the_same_world() {
        let mut rig = Rig::new(ServerConfig::default());
        let idx = rig.join("alice", "").unwrap();
        assert_eq!(idx, 1);
        assert_eq!(rig.host.players[1].name, "ALICE");
        rig.host.scrap = 3456;
        rig.host.try_build(SKind::WoodWall, super::super::tile_of(Vec2::new(11.0, 11.0))).unwrap();
        rig.run(40);
        let (cw, c) = &rig.clients[0];
        assert_eq!(c.state, ClientState::Playing);
        assert_eq!(cw.seed, rig.host.seed);
        assert_eq!(cw.scrap, rig.host.scrap);
        assert_eq!(cw.structures.len(), rig.host.structures.len());
        assert_eq!(cw.tiles, rig.host.tiles, "client collision must match the server's");
        assert_eq!(cw.barrels.len(), rig.host.barrels.len());
        assert_eq!(c.roster.len(), 2);
        assert_eq!(c.server_name, "ROTFALL SERVER");
        assert_eq!(cw.players[0].name, "HOST", "names come through the roster");
    }

    #[test]
    fn zombies_replicate_and_their_deaths_become_client_ragdolls() {
        let mut rig = Rig::new(ServerConfig::default());
        rig.join("bob", "").unwrap();
        for i in 0..12 {
            rig.host.spawn_zombie(ZKind::Walker, Vec2::new(25.0 + i as f32, 25.0));
        }
        rig.host.phase = Phase::Assault;
        rig.host.spawn_budget = 500.0;
        rig.host.spawn_dirs = vec![Vec2::X];
        rig.host.spawn_timer = 1000.0;
        rig.run(90);
        let want = rig.host.zombies.len();
        let (cw, _) = &rig.clients[0];
        assert!(want >= 12);
        assert_eq!(cw.zombies.len(), want, "client should track each zombie");
        let (hz, cz) = (&rig.host.zombies[0], cw.zombies.iter().find(|z| z.id == rig.host.zombies[0].id).unwrap());
        assert_eq!(cz.style, hz.style);
        assert!(cz.pos.distance(hz.pos) < 1.0, "positions drifted: {} vs {}", cz.pos, hz.pos);
        // Kill them all on the server.
        for z in &mut rig.host.zombies {
            z.hp = 0.0;
            z.spawn_t = 0.0;
        }
        rig.run(60);
        let (cw, _) = &rig.clients[0];
        assert!(cw.zombies.len() <= rig.host.zombies.len() + 1, "ghost zombies remain: {} vs {}", cw.zombies.len(), rig.host.zombies.len());
        assert!(cw.ragdolls.bodies.len() >= 8, "client made {} corpses", cw.ragdolls.bodies.len());
    }

    #[test]
    fn client_input_moves_shoots_and_commands_on_the_server() {
        let mut rig = Rig::new(ServerConfig::default());
        rig.join("carol", "").unwrap();
        rig.run(20);
        // Movement is client-side and reaches the server quickly.
        let start = rig.host.players[1].pos;
        let walk = PlayerInput { move_dir: Vec2::X, aim: vec3(50.0, 1.2, 5.0), ..Default::default() };
        for _ in 0..60 {
            rig.step(PlayerInput::default(), walk);
            std::thread::sleep(Duration::from_micros(300));
        }
        let (cw, _) = &rig.clients[0];
        assert!(cw.players[1].pos.x > start.x + 2.0, "the client should have moved itself");
        assert!(rig.host.players[1].pos.distance(cw.players[1].pos) < 1.5, "server lags too far: {} vs {}", rig.host.players[1].pos, cw.players[1].pos);
        // Firing happens on the server, using the replicated aim.
        let mag = rig.host.players[1].mag[SIDEARM];
        let fire = PlayerInput { aim: vec3(50.0, 1.2, 5.0), fire: true, ..Default::default() };
        for _ in 0..30 {
            rig.step(PlayerInput::default(), fire);
            std::thread::sleep(Duration::from_micros(300));
        }
        assert!(rig.host.players[1].mag[SIDEARM] < mag, "server should have fired for the client");
        assert!(rig.host.players[1].facing.x > 0.8);
        assert!(!rig.clients[0].0.fx.p.is_empty(), "the shots should appear as effects on the client");
        // Commands: spend shared scrap, and hear about refusals privately.
        rig.host.scrap = 1000;
        let tile = super::super::tile_of(Vec2::new(15.0, 15.0));
        rig.clients[0].1.send_command(Command::Build(SKind::SentryMg as u8, tile));
        rig.clients[0].1.send_command(Command::BuyWeapon(1));
        rig.run(30);
        assert!(rig.host.structure_at(tile).is_some(), "the build order should have run");
        assert!(rig.host.players[1].owned[1]);
        let (cw, _) = &rig.clients[0];
        assert!(cw.structure_at(tile).is_some());
        assert!(cw.players[1].owned[1], "the client should learn what it owns");
        assert_eq!(cw.scrap, rig.host.scrap);
        rig.host.scrap = 0;
        rig.clients[0].1.send_command(Command::BuyGrenades);
        rig.run(30);
        assert_eq!(rig.clients[0].1.note.as_deref(), Some("NOT ENOUGH SCRAP"));
    }

    #[test]
    fn the_horde_hunts_every_player_and_clients_die_and_respawn() {
        let mut rig = Rig::new(ServerConfig::default());
        rig.join("dave", "").unwrap();
        rig.run(10);
        rig.host.hurt_player(1, 500.0, Vec3::ZERO, crate::game::player::DeathCause::Regular);
        assert!(!rig.host.players[1].alive);
        rig.run(20);
        assert!(!rig.clients[0].0.players[1].alive, "the client should know it died");
        assert!(!rig.clients[0].0.ragdolls.bodies.is_empty(), "with a body on the ground");
        rig.host.players[1].respawn = 0.01;
        rig.run(30);
        assert!(rig.host.players[1].alive);
        let (cw, _) = &rig.clients[0];
        assert!(cw.players[1].alive, "and that it came back");
        assert!(cw.players[1].pos.distance(rig.host.players[1].pos) < 1.0, "at the server's chosen spot");
    }

    #[test]
    fn knockdowns_reach_the_client_and_it_cannot_walk_away() {
        use crate::game::player::DeathCause;
        let mut rig = Rig::new(ServerConfig::default());
        rig.join("kay", "").unwrap();
        rig.run(10);
        let at = rig.host.players[1].pos + Vec3::X;
        rig.host.knock_down(1, at, 22.0);
        rig.run(10);
        let (cw, _) = &rig.clients[0];
        assert!(cw.players[1].down_t > 0.0, "the client should know it is down");
        assert!(cw.ragdolls.bodies.iter().any(|b| b.owner == 1), "with its own ragdoll");
        // Walking does nothing, and the server ignores position updates meanwhile.
        let before = rig.host.players[1].pos;
        let walk = PlayerInput { move_dir: Vec2::X, aim: vec3(50.0, 1.2, 5.0), ..Default::default() };
        for _ in 0..20 {
            rig.step(PlayerInput::default(), walk);
            std::thread::sleep(Duration::from_micros(300));
        }
        assert!(rig.host.players[1].down_t > 0.0);
        assert!(rig.host.players[1].pos.distance(before) < 5.0);
        // Then up again, everywhere, with both machines agreeing where.
        rig.host.players[1].down_t = 0.01;
        rig.run(60);
        let (cw, _) = &rig.clients[0];
        assert_eq!(rig.host.players[1].down_t, 0.0);
        assert_eq!(cw.players[1].down_t, 0.0, "the client should be back up");
        assert!(cw.ragdolls.bodies.iter().all(|b| b.owner != 1), "its ragdoll gone");
        assert!(cw.players[1].pos.distance(rig.host.players[1].pos) < 1.5, "{} vs {}", cw.players[1].pos, rig.host.players[1].pos);
        // A death while down goes through the same body.
        rig.host.knock_down(1, at, 22.0);
        rig.run(5);
        rig.host.hurt_player(1, 1000.0, at, DeathCause::Explosion);
        rig.run(10);
        let (cw, _) = &rig.clients[0];
        assert!(!cw.players[1].alive);
        assert_eq!(cw.players[1].death_cause, DeathCause::Explosion);
        assert_eq!(cw.ragdolls.bodies.iter().filter(|b| b.owner == 1).count(), 1, "one body, not two");
        assert_eq!(cw.wave_track_for_test(), crate::audio::Track::Smitherines, "the client hears its own death");
    }

    #[test]
    fn a_body_that_rises_does_so_for_everyone() {
        use crate::game::player::DeathCause;
        let mut rig = Rig::new(ServerConfig::default());
        rig.join("zed", "").unwrap();
        rig.run(10);
        rig.host.hurt_player(1, 1000.0, Vec3::ZERO, DeathCause::Zombie);
        rig.run(10);
        assert!(rig.clients[0].0.ragdolls.bodies.iter().any(|b| b.owner == 1));
        rig.host.players[1].respawn = 0.01;
        rig.run(20);
        assert!(rig.host.players[1].alive);
        assert!(rig.host.zombies.iter().any(|z| z.style >> 24 == 8), "up on the server");
        let (cw, _) = &rig.clients[0];
        assert!(cw.zombies.iter().any(|z| z.style >> 24 == 8), "and on the client");
        assert!(cw.ragdolls.bodies.iter().all(|b| b.owner != 1), "the corpse is gone");
    }

    #[test]
    fn password_capacity_names_and_kicks_are_enforced() {
        let mut rig = Rig::new(ServerConfig { password: "hunter2".into(), max_players: 2, ..Default::default() });
        assert_eq!(rig.join("eve", "wrong"), Err("WRONG PASSWORD".into()));
        assert_eq!(rig.join("HOST", "hunter2"), Err("THAT NAME IS TAKEN".into()));
        assert_eq!(rig.join("frank", "hunter2"), Ok(1));
        assert_eq!(rig.join("grace", "hunter2"), Err("SERVER FULL".into()));
        rig.run(10);
        assert!(rig.server.kick(1, &mut rig.host));
        rig.run(30);
        assert!(matches!(&rig.clients[0].1.state, ClientState::Closed(w) if w == "KICKED BY THE HOST"), "{:?}", rig.clients[0].1.state);
        assert!(!rig.host.players[1].connected);
        assert_eq!(rig.join("frank", "hunter2"), Err("KICKED FROM THIS SERVER".into()));
    }

    #[test]
    fn reconnecting_by_name_restores_the_inventory() {
        let mut rig = Rig::new(ServerConfig::default());
        assert_eq!(rig.join("heidi", ""), Ok(1));
        rig.run(10);
        rig.host.players[1].owned[5] = true;
        rig.host.players[1].grenades = 7;
        rig.clients.clear(); // Drops the socket, as a crash or network loss would.
        for _ in 0..30 {
            rig.server.poll(&mut rig.host);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!rig.host.players[1].connected, "the server should notice the drop");
        assert_eq!(rig.join("heidi", ""), Ok(1), "same slot");
        rig.run(20);
        let cw = &rig.clients[0].0;
        assert!(cw.players[1].owned[5]);
        assert_eq!(cw.players[1].grenades, 7);
    }

    #[test]
    fn permadeath_players_stay_dead_and_the_last_death_ends_the_game() {
        let mut rig = Rig::new(ServerConfig { permadeath: true, ..Default::default() });
        rig.join("ivan", "").unwrap();
        rig.run(10);
        rig.host.hurt_player(1, 500.0, Vec3::ZERO, crate::game::player::DeathCause::Regular);
        assert!(rig.host.players[1].out);
        assert!(!rig.host.game_over, "a teammate is still alive");
        for _ in 0..600 {
            rig.host.update_all(1.0 / 60.0, &[]);
        }
        assert!(!rig.host.players[1].alive, "no respawn in permadeath");
        rig.host.hurt_player(0, 500.0, Vec3::ZERO, crate::game::player::DeathCause::Regular);
        assert!(rig.host.game_over);
        rig.run(10);
        assert!(rig.clients[0].0.game_over, "the client should see the end");
        assert!(rig.clients[0].0.permadeath);
    }

    #[test]
    fn garbage_from_the_network_cannot_crash_the_server_or_client() {
        let mut rig = Rig::new(ServerConfig::default());
        let port = rig.server.port;
        let mut rng = crate::util::Rng::new(9);
        // Random bytes, oversized frames, and valid frames with hostile values.
        for round in 0..60 {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let len = rng.int(400);
            let junk: Vec<u8> = (0..len).map(|_| rng.next_u32() as u8).collect();
            let mut msg = (junk.len() as u32).to_le_bytes().to_vec();
            if round % 5 == 0 {
                msg = u32::MAX.to_le_bytes().to_vec();
            }
            msg.extend_from_slice(&junk);
            let _ = s.write_all(&msg);
            rig.run(2);
        }
        // A real join followed by commands with absurd arguments.
        rig.join("mallory", "").unwrap();
        rig.run(5);
        for c in [Command::Build(250, (60000, 60000)), Command::Build(16, (5, 5)), Command::BuyWeapon(255), Command::BuyAmmo(200), Command::Sell((65535, 0)), Command::Upgrade((65535, 65535)), Command::Repair((0, 65535))] {
            rig.clients[0].1.send_command(c);
        }
        rig.run(20);
        assert_eq!(rig.clients[0].1.state, ClientState::Playing, "bad requests are refused, not fatal");
        // The client decoders too, on random data.
        let mut w = World::new(5, Audio::silent());
        for _ in 0..800 {
            let len = rng.int(600);
            let junk: Vec<u8> = (0..len).map(|_| rng.next_u32() as u8).collect();
            let _ = apply_snapshot_and_events(&mut w, &mut Reader::new(&junk));
            let _ = read_ev(&mut Reader::new(&junk));
        }
    }

    #[test]
    fn a_full_battle_stays_in_sync_within_budget() {
        let mut rig = Rig::new(ServerConfig::default());
        rig.join("nina", "").unwrap();
        rig.host.scrap = 20000;
        rig.host.wave = 9;
        rig.host.hold_timer = false;
        rig.host.phase = Phase::Build { timer: 0.5 };
        rig.host.invincible = true;
        let mut bot = Bot::new();
        let mut peak = 0;
        let started = Instant::now();
        for _ in 0..60 * 50 {
            let h = bot.think(&mut rig.host, 1.0 / 60.0);
            let c = PlayerInput { aim: vec3(0.0, 1.2, 30.0), fire: true, ..Default::default() };
            rig.step(h, c);
            std::thread::sleep(Duration::from_micros(100));
            peak = peak.max(rig.host.zombies.len());
            assert!(rig.clients[0].0.players[1].pos.is_finite());
        }
        assert_eq!(rig.clients[0].1.state, ClientState::Playing);
        assert!(peak > 100, "wanted a real horde, peaked at {peak}");
        rig.run(30);
        let (cw, _) = &rig.clients[0];
        let diff = (cw.zombies.len() as i32 - rig.host.zombies.len() as i32).abs();
        assert!(diff <= 12, "zombie counts diverged: client {} host {}", cw.zombies.len(), rig.host.zombies.len());
        assert_eq!(cw.structures.len(), rig.host.structures.len());
        // Bytes per simulated second, to keep the design honest about bandwidth.
        let secs = 50.5;
        let kb = rig.server.conns[0].link.sent as f32 / 1024.0 / secs;
        println!("battle: peak {peak} zombies, {kb:.0} KB/s to the client, ran {:.1}s", started.elapsed().as_secs_f32());
        assert!(kb < 1500.0, "too much bandwidth: {kb} KB/s");
    }

    #[test]
    fn lan_browser_hears_the_beacon() {
        let mut browser = LanBrowser::new();
        if browser.sock.is_none() {
            return; // The port is in use on this machine, e.g. by a running copy of the game.
        }
        let mut rig = Rig::new(ServerConfig { name: "ROOM 7".into(), ..Default::default() });
        for _ in 0..30 {
            rig.server.after_update(&mut rig.host, 0.1);
            browser.poll();
            std::thread::sleep(Duration::from_millis(3));
        }
        let s = browser.servers.iter().find(|s| s.name == "ROOM 7").expect("beacon not heard");
        assert!(s.addr.ends_with(&format!(":{}", rig.server.port)));
    }
}
