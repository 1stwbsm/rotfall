//! The player: movement, shooting, reloading, dodge rolling.
use super::data::*;
use super::fx::Ev;
use super::{tile_index, tile_min, tile_of, World, TILE};
use crate::audio::Sfx;
use crate::ragdoll::*;
use crate::render::mesh as m;
use glam::{vec3, Vec2, Vec3};

#[derive(Clone, Copy, Default)]
pub struct PlayerInput {
    pub move_dir: Vec2,
    pub aim: Vec3,
    pub fire: bool,
    pub reload: bool,
    pub sprint: bool,
    pub dodge: bool,
    pub grenade: bool,
    pub toggle_light: bool,
    pub select: Option<usize>,
    pub cycle: i32,
}

/// What killed a player: it picks the death track and scene, and whether the body rises.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DeathCause {
    #[default]
    Regular,
    Zombie,
    Explosion,
}

impl DeathCause {
    pub fn from_u8(v: u8) -> DeathCause {
        match v {
            1 => DeathCause::Zombie,
            2 => DeathCause::Explosion,
            _ => DeathCause::Regular,
        }
    }
}

/// How long standing back up takes, once a knockdown ends.
const GETUP_TIME: f32 = 0.5;
/// Time taken off a knockdown by each press of the roll key.
const MASH_TIME: f32 = 0.2;
/// However hard you mash, the blast gets this long to put you on the ground.
const MIN_DOWN: f32 = 0.6;
/// Damage multiplier against someone lying helpless on the ground.
const DOWN_DAMAGE: f32 = 2.0;
/// How steeply a gun can tip up or down, in radians.
const MAX_PITCH: f32 = 1.15;

pub struct Player {
    pub pos: Vec3,
    pub vel: Vec2,
    pub aim: Vec3,
    pub facing: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub stamina: f32,
    pub regen_delay: f32,
    pub alive: bool,
    pub respawn: f32,
    pub weapon: usize,
    pub owned: [bool; 18],
    pub mag: [u32; 18],
    pub reserve: [u32; 18],
    pub cooldown: f32,
    pub reload: f32,
    pub recoil: f32,
    pub spin: f32,
    pub walk: f32,
    pub roll_t: f32,
    pub roll_dir: Vec2,
    pub roll_cd: f32,
    pub grenades: u32,
    pub grenade_cd: f32,
    pub flashlight: bool,
    pub hurt: f32,
    pub spread: f32,
    pub on_depot: bool,
    pub empty_click: f32,
    pub name: String,
    /// Moved by a network client rather than by this machine.
    pub remote: bool,
    /// False while a network player is away; their slot and inventory wait for them.
    pub connected: bool,
    /// Permadeath: this player has died and is spectating.
    pub out: bool,
    /// Bumped on every respawn so that a client knows to adopt the server's position.
    pub respawn_seq: u8,
    /// Client side only: where the server last said this player is.
    pub net_target: Vec2,
    /// Seconds left ragdolled and helpless after being blown off your feet.
    pub down_t: f32,
    /// Seconds left of the stand-up animation, and the pose it starts from.
    /// How long this knockdown has lasted so far.
    pub down_for: f32,
    pub getup_t: f32,
    pub getup_from: Pose,
    pub death_cause: DeathCause,
    /// Seconds since this player died.
    pub death_t: f32,
    pool_t: f32,
}

pub const RADIUS: f32 = 0.38;

impl Player {
    pub fn spawn_point(i: usize) -> Vec3 {
        let a = i as f32 * 1.1;
        vec3(a.sin() * 5.0, 0.0, a.cos() * 5.0 + 1.0)
    }

    pub fn new(name: &str, index: usize) -> Self {
        let mut owned = [false; 18];
        owned[SIDEARM] = true;
        let mut mag = [0; 18];
        let mut reserve = [0; 18];
        for i in 0..18 {
            mag[i] = WEAPONS[i].mag;
            reserve[i] = WEAPONS[i].reserve;
        }
        Player {
            pos: Self::spawn_point(index),
            vel: Vec2::ZERO,
            aim: vec3(0.0, 1.2, 12.0),
            facing: Vec2::Y,
            hp: 100.0,
            max_hp: 100.0,
            stamina: 1.0,
            regen_delay: 0.0,
            alive: true,
            respawn: 0.0,
            weapon: SIDEARM,
            owned,
            mag,
            reserve,
            cooldown: 0.0,
            reload: 0.0,
            recoil: 0.0,
            spin: 0.0,
            walk: 0.0,
            roll_t: 0.0,
            roll_dir: Vec2::Y,
            roll_cd: 0.0,
            grenades: 3,
            grenade_cd: 0.0,
            flashlight: true,
            hurt: 0.0,
            spread: 0.0,
            on_depot: false,
            empty_click: 0.0,
            name: name.to_string(),
            remote: false,
            connected: true,
            out: false,
            respawn_seq: 0,
            net_target: Vec2::ZERO,
            down_t: 0.0,
            down_for: 0.0,
            getup_t: 0.0,
            getup_from: [Vec3::ZERO; JOINTS],
            death_cause: DeathCause::Regular,
            death_t: 0.0,
            pool_t: 0.0,
        }
    }

    pub fn pos2(&self) -> Vec2 {
        Vec2::new(self.pos.x, self.pos.z)
    }

    pub fn gun_origin(&self) -> Vec3 {
        let f = vec3(self.facing.x, 0.0, self.facing.y);
        let r = vec3(self.facing.y, 0.0, -self.facing.x);
        self.pos + vec3(0.0, 1.22, 0.0) + f * 0.42 + r * 0.14
    }

    /// Which way the gun points: towards the aim, tipped up or down as far as arms can manage.
    pub fn aim_dir(&self) -> Vec3 {
        let to = self.aim - self.gun_origin();
        let across = Vec2::new(to.x, to.z).length().max(0.01);
        let (sin, cos) = to.y.atan2(across).clamp(-MAX_PITCH, MAX_PITCH).sin_cos();
        vec3(self.facing.x * cos, sin, self.facing.y * cos)
    }

    pub fn muzzle(&self) -> Vec3 {
        self.gun_origin() + self.aim_dir() * WEAPONS[self.weapon].muzzle
    }

    /// The line from the muzzle to the exact aim point, which the gun's offset from the body's
    /// center would otherwise miss by a little. Anything too close to line up gets the gun's own line.
    pub fn shot_dir(&self) -> Vec3 {
        let gun = self.aim_dir();
        let to = self.aim - self.muzzle();
        match to.try_normalize() {
            Some(d) if to.length() > 0.6 && d.dot(gun) > 0.8 => d,
            _ => gun,
        }
    }

    pub fn add_ammo_fraction(&mut self, wi: usize, frac: f32) {
        let w = &WEAPONS[wi];
        if w.reserve == u32::MAX {
            return;
        }
        let add = ((w.reserve as f32 * frac) as u32).max(w.mag);
        self.reserve[wi] = (self.reserve[wi].saturating_add(add)).min(w.reserve);
    }

    /// A standing/running/aiming pose for the player's 16-joint skeleton.
    pub fn pose(&self) -> Pose {
        let f = vec3(self.facing.x, 0.0, self.facing.y);
        let r = vec3(self.facing.y, 0.0, -self.facing.x);
        let speed = self.vel.length();
        let swing = (speed / 7.0).min(1.0);
        let base = self.pos;
        let w = |lx: f32, ly: f32, lz: f32| base + r * lx + Vec3::Y * ly + f * lz;
        let mut p = rest_pose(1.0);
        let rolling = self.roll_t > 0.0;
        let bob = (self.walk * 2.0).sin().abs() * 0.05 * swing;
        let crouch = if rolling { 0.45 } else { 0.0 };
        p[PELVIS] = w(0.0, 0.94 + bob - crouch, -0.02);
        p[CHEST] = w(0.0, 1.25 + bob - crouch, 0.05);
        p[NECK] = w(0.0, 1.47 + bob - crouch, 0.09);
        p[HEAD] = w(0.0, 1.63 + bob - crouch, 0.11);
        p[L_SHO] = w(-0.2, 1.41 + bob - crouch, 0.06);
        p[R_SHO] = w(0.2, 1.41 + bob - crouch, 0.06);
        p[L_HIP] = w(-0.11, 0.91 + bob - crouch, -0.02);
        p[R_HIP] = w(0.11, 0.91 + bob - crouch, -0.02);
        // Legs step in the direction of travel, not the direction of aim.
        let vd = self.vel.normalize_or_zero();
        let dir3 = vec3(vd.x, 0.0, vd.y);
        let stride = 0.42 * swing;
        for (hip, knee, foot, ph, side) in [(L_HIP, L_KNEE, L_FOOT, 0.0, -1.0f32), (R_HIP, R_KNEE, R_FOOT, std::f32::consts::PI, 1.0)] {
            let t = self.walk + ph;
            let lift = t.cos().max(0.0) * 0.2 * swing;
            let target = base + r * (side * 0.13) + dir3 * (t.sin() * stride) + Vec3::Y * (0.07 + lift);
            let target = if rolling { p[hip] + f * 0.25 + Vec3::Y * -0.3 } else { target };
            p[foot] = target;
            p[knee] = two_bone(p[hip], target, 0.43, 0.43, f + dir3 * 0.5);
        }
        // Hands hold the gun.
        let def = &WEAPONS[self.weapon];
        let kick = self.recoil * 0.1;
        let reload_dip = if self.reload > 0.0 { (self.reload * 6.0).sin().abs() * 0.08 + 0.1 } else { 0.0 };
        let grip = self.gun_origin() - f * kick - Vec3::Y * (reload_dip + crouch);
        p[R_HAND] = grip;
        p[R_ELB] = two_bone(p[R_SHO], grip, 0.29, 0.27, r * 0.8 - Vec3::Y);
        let off = if def.two_handed { grip + self.aim_dir() * (def.muzzle * 0.55).min(0.42) - r * 0.02 + Vec3::Y * 0.02 } else { grip - r * 0.08 - f * 0.06 - Vec3::Y * 0.02 };
        p[L_HAND] = off;
        p[L_ELB] = two_bone(p[L_SHO], off, 0.29, 0.27, -r * 0.8 - Vec3::Y);
        // Climbing up from wherever the ragdoll came to rest.
        if self.getup_t > 0.0 {
            let k = (self.getup_t / GETUP_TIME).clamp(0.0, 1.0);
            let k = k * k * (3.0 - 2.0 * k);
            for j in 0..JOINTS {
                p[j] = p[j].lerp(self.getup_from[j], k);
            }
        }
        p
    }
}

impl World {
    /// Local players run movement here; a remote player's client does it and sends the result.
    pub fn player_move(&mut self, i: usize, dt: f32, input: &PlayerInput) {
        let wish = input.move_dir.clamp_length_max(1.0);
        let moving = wish.length_squared() > 0.01;
        let p = &mut self.players[i];
        let sprinting = input.sprint && moving && p.stamina > 0.02 && p.roll_t <= 0.0;
        if sprinting {
            p.stamina = (p.stamina - dt * 0.28).max(0.0);
        } else {
            p.stamina = (p.stamina + dt * 0.2).min(1.0);
        }
        let firing_penalty = if input.fire && WEAPONS[p.weapon].spinup > 0.0 { 0.55 } else { 1.0 };
        let speed = if sprinting { 9.0 } else { 6.2 * firing_penalty };
        p.roll_cd -= dt;
        if input.dodge && p.roll_cd <= 0.0 && p.stamina > 0.15 && moving {
            p.roll_t = 0.32;
            p.roll_cd = 0.75;
            p.roll_dir = wish.normalize();
            p.stamina -= 0.15;
            let at = p.pos;
            self.snd(Sfx::Dash, at, 0.7);
        }
        let p = &mut self.players[i];
        let target_vel = if p.roll_t > 0.0 {
            p.roll_t -= dt;
            p.roll_dir * 16.0
        } else {
            wish * speed
        };
        let accel = if p.roll_t > 0.0 { 30.0 } else { 14.0 };
        p.vel += (target_vel - p.vel) * (accel * dt).min(1.0);
        let vel = p.vel;
        let moved = p.pos2() + vel * dt;
        p.walk += vel.length() * dt * 1.55;
        let resolved = self.collide_player(moved);
        let p = &mut self.players[i];
        p.pos.x = resolved.x;
        p.pos.z = resolved.y;
    }

    pub fn update_player(&mut self, i: usize, dt: f32, input: &PlayerInput) {
        let p = &mut self.players[i];
        p.recoil = (p.recoil - dt * 4.0).max(0.0);
        p.hurt = (p.hurt - dt * 2.0).max(0.0);
        p.spread = (p.spread - dt * 3.0).max(0.0);
        p.cooldown -= dt;
        p.grenade_cd -= dt;
        p.empty_click -= dt;
        p.getup_t = (p.getup_t - dt).max(0.0);

        // Thrown off your feet: no moving, no shooting, and everything hurts double.
        if p.alive && p.down_t > 0.0 {
            p.down_t -= dt;
            p.down_for += dt;
            if input.dodge {
                // Never so far that it ends before the floor, which only time can pass.
                p.down_t = (p.down_t - MASH_TIME).max((MIN_DOWN - p.down_for).max(0.0));
                self.struggle(i);
            }
            let p = &mut self.players[i];
            if p.down_t <= 0.0 {
                self.stand_up(i);
            }
            return;
        }
        if p.alive && p.getup_t > 0.0 {
            return;
        }

        if !p.alive {
            if !p.out {
                p.respawn -= dt;
                if p.respawn <= 0.0 && !self.game_over {
                    self.respawn_player(i);
                }
            }
            return;
        }

        // ---- aim
        let aim2 = Vec2::new(input.aim.x - p.pos.x, input.aim.z - p.pos.z);
        if aim2.length_squared() > 0.04 {
            p.facing = aim2.normalize();
        }
        p.aim = input.aim;

        if !p.remote {
            self.player_move(i, dt, input);
        }
        let p = &mut self.players[i];

        // ---- regen
        p.regen_delay -= dt;
        if p.regen_delay <= 0.0 && p.hp < p.max_hp {
            p.hp = (p.hp + dt * 4.0).min(p.max_hp);
        }

        if input.toggle_light {
            p.flashlight = !p.flashlight;
            self.snd_ui(Sfx::Click, 0.5);
        }

        // ---- weapon switching
        let mut want = input.select;
        if input.cycle != 0 {
            let n = WEAPONS.len() as i32;
            let mut w = self.players[i].weapon as i32;
            for _ in 0..n {
                w = (w + input.cycle).rem_euclid(n);
                if self.players[i].owned[w as usize] {
                    break;
                }
            }
            want = Some(w as usize);
        }
        if let Some(w) = want {
            if w < WEAPONS.len() && self.players[i].owned[w] && w != self.players[i].weapon {
                self.equip(i, w);
            }
        }

        // ---- reload
        let wi = self.players[i].weapon;
        let def = &WEAPONS[wi];
        let p = &mut self.players[i];
        if p.reload > 0.0 {
            p.reload -= dt;
            if p.reload <= 0.0 {
                let need = def.mag.saturating_sub(p.mag[wi]);
                let take = if def.reserve == u32::MAX { need } else { need.min(p.reserve[wi]) };
                p.mag[wi] += take;
                if def.reserve != u32::MAX {
                    p.reserve[wi] -= take;
                }
                p.reload = 0.0;
                let at = p.pos;
                self.snd(Sfx::ReloadEnd, at, 0.6);
            }
        } else if input.reload && p.mag[wi] < def.mag && (p.reserve[wi] > 0) {
            self.begin_reload(i);
        }

        // ---- spin-up (rotary guns need to wind up before shooting)
        let p = &mut self.players[i];
        if def.spinup > 0.0 {
            let up = input.fire && p.reload <= 0.0 && p.mag[wi] > 0;
            p.spin = (p.spin + if up { dt / def.spinup } else { -dt * 0.8 }).clamp(0.0, 1.0);
        } else {
            p.spin = 0.0;
        }

        // ---- firing
        if input.fire && self.players[i].reload <= 0.0 && self.players[i].cooldown <= 0.0 && !self.game_over {
            if self.players[i].mag[wi] == 0 {
                if self.players[i].reserve[wi] > 0 {
                    self.begin_reload(i);
                } else {
                    if self.players[i].empty_click <= 0.0 {
                        self.players[i].empty_click = 0.5;
                        let at = self.players[i].pos;
                        self.snd(Sfx::Empty, at, 0.8);
                        if i == self.local {
                            self.announce("OUT OF AMMO - SWITCHING TO SIDEARM", [1.0, 0.5, 0.2], 1.5, false);
                        }
                    }
                    self.equip(i, SIDEARM);
                }
            } else if def.spinup <= 0.0 || self.players[i].spin > 0.98 {
                self.player_fire(i, wi);
            }
        }

        if input.grenade && self.players[i].grenades > 0 && self.players[i].grenade_cd <= 0.0 {
            self.throw_grenade(i);
        }

        // ---- ammo depots refill
        let pos = self.players[i].pos;
        let near_depot = self.structures.iter().any(|s| s.kind == SKind::AmmoDepot && s.pos.distance_squared(pos) < 16.0);
        let p = &mut self.players[i];
        p.on_depot = near_depot;
        if near_depot {
            for w in 0..18 {
                if p.owned[w] && WEAPONS[w].reserve != u32::MAX {
                    let add = (WEAPONS[w].reserve as f32 * 0.05 * dt).ceil() as u32;
                    p.reserve[w] = (p.reserve[w] + add).min(WEAPONS[w].reserve);
                }
            }
        }
    }

    pub fn equip(&mut self, i: usize, w: usize) {
        let p = &mut self.players[i];
        p.weapon = w;
        p.reload = 0.0;
        p.cooldown = 0.18;
        p.spin = 0.0;
        let at = p.pos;
        self.snd(Sfx::ReloadStart, at, 0.35);
    }

    fn begin_reload(&mut self, i: usize) {
        let p = &mut self.players[i];
        p.reload = WEAPONS[p.weapon].reload;
        let at = p.pos;
        self.snd(Sfx::ReloadStart, at, 0.6);
    }

    fn collide_player(&self, mut p: Vec2) -> Vec2 {
        let lim = super::map::HALF - 1.0;
        p = p.clamp(Vec2::splat(-lim), Vec2::splat(lim));
        p = self.map.resolve_circle(p, RADIUS);
        let t = tile_of(p);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let c = (t.0 + dx, t.1 + dz);
                let Some(ti) = tile_index(c) else { continue };
                if !self.blocked_tiles[ti] {
                    continue;
                }
                let si = self.tiles[ti];
                if si >= 0 && self.structures[si as usize].kind == SKind::Gate {
                    continue; // Gates open for you.
                }
                let min = tile_min(c);
                if let Some(n) = super::map::push_out_circle(p, RADIUS, min, min + Vec2::splat(TILE)) {
                    p = n;
                }
            }
        }
        p
    }

    fn player_fire(&mut self, i: usize, wi: usize) {
        let def = &WEAPONS[wi];
        let p = &mut self.players[i];
        p.mag[wi] -= 1;
        p.cooldown = 1.0 / def.rate;
        p.recoil = (p.recoil + def.recoil).min(0.8);
        let aim = p.shot_dir();
        let from = p.muzzle();
        // Flames, arcs and thrown things spread out at their own height, so they only take the heading.
        let f2 = Vec2::new(aim.x, aim.z).try_normalize().unwrap_or(p.facing);
        let moving = (p.vel.length() / 6.0).min(1.0);
        let extra_spread = p.spread * 0.06 + moving * 0.012;
        p.spread = (p.spread + def.recoil * 1.2).min(1.0);
        let cooldown = p.cooldown;
        // Only the shooter's own camera kicks.
        if i == self.local {
            self.trauma = (self.trauma + def.kick).min(1.0);
        } else {
            self.fx.record(|| Ev::Recoil { player: i as u8, kick: def.kick });
        }

        let dir = vec3(f2.x, 0.0, f2.y);
        let right = vec3(f2.y, 0.0, -f2.x);
        let loud = matches!(def.fire, Fire::Flame | Fire::Cryo | Fire::Beam);
        if !loud || cooldown <= 0.0 || self.time.rem_euclid(0.18) < 0.02 {
            self.snd(def.sfx, from, 0.75);
        }
        match def.fire {
            Fire::Hitscan { pellets, spread, pierce } => {
                let fc = if wi == 5 { 0.75 } else { 1.0 };
                self.fx.muzzle_flash(from, aim, 0.3 + pellets as f32 * 0.03 + def.kick * 0.5, vec3(1.0, 0.72, 0.36) * fc);
                self.fx.casing(from - aim * def.muzzle * 0.8, right);
                let width = if wi == 17 { 0.14 } else if pierce > 4 { 0.05 } else { 0.022 };
                for _ in 0..pellets {
                    let a = self.rng.range(-1.0, 1.0) * (spread + extra_spread);
                    let d = rotate_y(aim, a);
                    let col = if wi == 17 { [4.0, 9.0, 20.0] } else { def.tracer };
                    self.hitscan(from, d, 60.0, def.damage, pierce, def.knock, def.gore, col, width);
                }
                if wi == 17 {
                    // The rail leaves an ionized trail.
                    for k in 0..16 {
                        let mut q = from + aim * k as f32 * 2.0;
                        q.y = q.y.max(0.1);
                        self.fx.frost_puff(q, Vec3::Y * 0.4, 0.14, 0.6);
                    }
                    self.fx.light(from + dir * 10.0, 22.0, vec3(5.0, 12.0, 25.0), 0.2);
                }
            }
            Fire::Explosive { speed, radius, arc } => {
                self.fx.muzzle_flash(from, aim, 0.5, vec3(1.0, 0.7, 0.3));
                // Rockets fly straight at the aim point; launcher grenades lob along their own arc.
                self.spawn_explosive(from, if arc { dir } else { aim }, speed, radius, def.damage, arc, false);
            }
            Fire::Flame => self.flame_stream(from, dir, 7.5, def.damage, 0.5),
            Fire::Cryo => self.frost_stream(from, dir, 8.0, def.damage),
            Fire::Tesla { chains } => {
                self.fx.muzzle_flash(from, dir, 0.3, vec3(0.3, 0.6, 1.0));
                self.tesla_arc(from, dir, 15.0, def.damage, chains);
            }
            Fire::Beam => self.beam(from, aim, 34.0, def.damage),
            Fire::Saw { bounces } => {
                self.fx.muzzle_flash(from, dir, 0.25, vec3(1.0, 0.9, 0.6));
                self.spawn_saw(from, dir, def.damage, bounces);
            }
        }
    }

    fn throw_grenade(&mut self, i: usize) {
        let p = &mut self.players[i];
        p.grenades -= 1;
        p.grenade_cd = 0.7;
        let f = p.facing;
        let dir = vec3(f.x, 0.0, f.y);
        let from = p.gun_origin() + Vec3::Y * 0.2;
        let dist = (Vec2::new(p.aim.x, p.aim.z) - p.pos2()).length().clamp(4.0, 24.0);
        let speed = dist.sqrt() * 3.6;
        self.spawn_explosive(from, dir, speed, 6.5, 420.0, true, true);
        self.snd(Sfx::ReloadStart, from, 0.6);
    }

    pub fn hurt_player(&mut self, i: usize, amount: f32, from: Vec3, cause: DeathCause) {
        let invincible = self.invincible;
        let p = &mut self.players[i];
        if !p.alive || p.roll_t > 0.0 || invincible {
            return;
        }
        let amount = if p.down_t > 0.0 { amount * DOWN_DAMAGE } else { amount };
        p.hp -= amount;
        p.hurt = (p.hurt + amount / 22.0).min(1.0);
        p.regen_delay = 5.0;
        let ground = p.pos;
        let pos = ground + Vec3::Y * 1.2;
        let dead = p.hp <= 0.0;
        let away = (pos - from).normalize_or_zero();
        self.fx.blood(pos, away, 8, 4.0);
        self.fx.blood_splat(ground, 0.3);
        if i == self.local {
            self.shake((amount / 35.0).min(0.5));
        }
        self.snd(Sfx::Hurt, pos, 0.9);
        if dead {
            self.kill_player(i, away, cause);
        }
    }

    fn owned_body(&self, i: usize) -> Option<usize> {
        self.ragdolls.bodies.iter().position(|b| b.owner == i as i16)
    }

    /// Blows a living player off their feet. Their body becomes a ragdoll until they get up.
    pub fn knock_down(&mut self, i: usize, from: Vec3, force: f32) {
        let invincible = self.invincible;
        let p = &mut self.players[i];
        if !p.alive || invincible {
            return;
        }
        let time = 1.5 + (force / 26.0).min(1.0) * 1.2;
        if p.down_t > 0.0 {
            // Already on the ground: the new blast only keeps them there.
            p.down_t = p.down_t.max(time);
            p.down_for = 0.0;
            return;
        }
        p.down_t = time;
        p.down_for = 0.0;
        p.roll_t = 0.0;
        p.reload = 0.0;
        p.spin = 0.0;
        self.fx.muted(true);
        self.knockdown_visual(i, from, force);
        self.fx.muted(false);
        self.fx.record(|| Ev::PlayerDown { player: i as u8, from, force });
        if i == self.local {
            self.trauma = (self.trauma + 0.4).min(1.0);
        }
        let at = self.players[i].pos;
        self.snd(Sfx::Hurt, at, 1.0);
    }

    /// A press of the mash key, as seen and heard: the body jerks as it fights to rise.
    pub fn struggle(&mut self, i: usize) {
        if let Some(b) = self.owned_body(i) {
            let at = self.ragdolls.bodies[b].pos[PELVIS];
            // Mixed from the pose, so that each press kicks a different way without touching the rng.
            let wobble = (at.x * 37.0 + at.z * 53.0 + self.players[i].down_t * 91.0).sin();
            self.ragdolls.impulse(b, at, vec3(wobble * 1.5, 3.2, (wobble * 7.0).cos() * 1.5), 2.0);
        }
        if i == self.local {
            self.audio.send(crate::audio::Cmd::Play { id: Sfx::MenuToggle, gain: 0.55, pan: 0.0, pitch: 0.6 });
        }
    }

    /// The ragdoll for a knockdown. Runs on servers and clients alike.
    pub fn knockdown_visual(&mut self, i: usize, from: Vec3, force: f32) {
        let p = &self.players[i];
        let pose = p.pose();
        let at = p.pos + Vec3::Y;
        let vel = vec3(p.vel.x, 1.0, p.vel.y);
        let body = self.ragdolls.spawn(&pose, vel, 1.0, ALL_JOINTS, super::zombie::STYLE_PLAYER, 60.0);
        self.ragdolls.bodies[body].owner = i as i16;
        let dir = ((at - from).normalize_or(Vec3::Z) + Vec3::Y * 0.6).normalize();
        self.ragdolls.impulse(body, at, dir * force * 0.75, 3.0);
    }

    fn stand_up(&mut self, i: usize) {
        self.finish_knockdown(i);
        let p2 = self.players[i].pos2();
        let free = self.collide_player(p2);
        let p = &mut self.players[i];
        p.pos.x = free.x;
        p.pos.z = free.y;
        p.vel = Vec2::ZERO;
        p.respawn_seq = p.respawn_seq.wrapping_add(1);
    }

    /// Swaps the ragdoll back for the character, who then climbs to their feet.
    pub fn finish_knockdown(&mut self, i: usize) {
        let body = self.owned_body(i);
        let p = &mut self.players[i];
        p.down_t = 0.0;
        if let Some(b) = body {
            let pose = self.ragdolls.bodies[b].pos;
            p.getup_from = pose;
            p.getup_t = GETUP_TIME;
            p.pos.x = pose[PELVIS].x;
            p.pos.z = pose[PELVIS].z;
            self.ragdolls.bodies.swap_remove(b);
        }
    }

    /// Keeps a player on their ragdoll, and makes death leave its mark. Runs everywhere.
    pub fn track_player_bodies(&mut self, dt: f32) {
        self.fx.muted(true);
        for i in 0..self.players.len() {
            let (alive, down) = (self.players[i].alive, self.players[i].down_t > 0.0);
            if alive && !down {
                continue;
            }
            let body = self.owned_body(i);
            if let Some(b) = body {
                let at = self.ragdolls.bodies[b].pos[PELVIS];
                let p = &mut self.players[i];
                p.pos.x = at.x;
                p.pos.z = at.z;
            }
            if !alive {
                let at = self.players[i].pos;
                let p = &mut self.players[i];
                p.death_t += dt;
                p.pool_t -= dt;
                let (t, due) = (p.death_t, p.pool_t <= 0.0 && p.death_t < 3.0);
                if due {
                    p.pool_t = 0.3;
                    // The pool spreads for a few seconds, from wherever the body came to rest.
                    self.fx.blood_splat(at, 0.45 + t * 0.4);
                }
            }
        }
        self.fx.muted(false);
    }

    /// The body and blood of a fallen player. Runs on servers and clients alike.
    pub fn player_death_visual(&mut self, i: usize, away: Vec3, cause: DeathCause) {
        let p = &mut self.players[i];
        p.death_cause = cause;
        p.death_t = 0.0;
        p.pool_t = 0.0;
        p.down_t = 0.0;
        p.getup_t = 0.0;
        let pose = p.pose();
        let ground = p.pos;
        let vel = vec3(p.vel.x, 2.0, p.vel.y);
        // Someone already on the ground dies where they lie.
        let body = match self.owned_body(i) {
            Some(b) => b,
            None => {
                let b = self.ragdolls.spawn(&pose, vel, 1.0, ALL_JOINTS, super::zombie::STYLE_PLAYER, 60.0);
                self.ragdolls.bodies[b].owner = i as i16;
                b
            }
        };
        self.ragdolls.impulse(body, ground + Vec3::Y, away * 5.0, 2.0);
        self.fx.blood(ground + Vec3::Y, Vec3::Y, 30, 6.0);
        self.fx.blood_splat(ground, 1.2);
        match cause {
            DeathCause::Explosion => {
                // Smitherines, as advertised.
                self.ragdolls.impulse(body, ground, (away + Vec3::Y).normalize_or(Vec3::Y) * 16.0, 3.0);
                for limb in [Limb::LArm, Limb::RLeg, Limb::Head, Limb::RArm] {
                    if self.fx.rng.chance(0.75) {
                        let fly = (self.fx.rng.dir3() + Vec3::Y).normalize() * self.fx.rng.range(5.0, 12.0);
                        let b = &mut self.ragdolls.bodies[body];
                        b.sever(limb);
                        let joints: &[usize] = match limb {
                            Limb::LArm => &[L_SHO, L_ELB, L_HAND],
                            Limb::RArm => &[R_SHO, R_ELB, R_HAND],
                            Limb::LLeg => &[L_HIP, L_KNEE, L_FOOT],
                            Limb::RLeg => &[R_HIP, R_KNEE, R_FOOT],
                            Limb::Head => &[HEAD],
                        };
                        for &j in joints {
                            b.vel[j] += fly;
                        }
                    }
                }
                self.ragdolls.bodies[body].char_amount = 1.0;
                self.fx.gibs(ground + Vec3::Y, 18, 12.0, [0.3, 0.32, 0.22]);
                self.fx.decal(ground.x, ground.z, 2.6, [0.015, 0.012, 0.01, 0.9], 1.0);
            }
            DeathCause::Zombie => {
                // They are still eating.
                self.fx.blood(ground + Vec3::Y * 0.6, Vec3::Y, 40, 3.5);
                self.fx.gibs(ground + Vec3::Y * 0.8, 6, 4.0, [0.3, 0.32, 0.22]);
            }
            DeathCause::Regular => {}
        }
    }

    fn kill_player(&mut self, i: usize, away: Vec3, cause: DeathCause) {
        self.fx.muted(true);
        self.player_death_visual(i, away, cause);
        self.fx.muted(false);
        self.fx.record(|| Ev::PlayerDied { player: i as u8, away, cause: cause as u8 });
        let permadeath = self.permadeath;
        let name = self.players[i].name.clone();
        let p = &mut self.players[i];
        p.alive = false;
        p.hp = 0.0;
        p.respawn = 8.0;
        p.out = permadeath;
        let at = p.pos;
        self.stats.deaths += 1;
        self.snd(Sfx::PlayerDeath, at, 1.0);
        if permadeath {
            let everyone_out = self.players.iter().all(|p| p.out || !p.connected);
            if everyone_out {
                self.game_over = true;
                self.announce("NOBODY LEFT STANDING", [1.0, 0.2, 0.1], 8.0, true);
            } else {
                self.announce(&format!("{name} IS GONE FOR GOOD"), [1.0, 0.25, 0.2], 4.0, true);
            }
        } else if i != self.local {
            self.announce(&format!("{name} IS DOWN - REDEPLOYING"), [1.0, 0.25, 0.2], 3.0, false);
        }
        self.nav_dirty = true;
    }

    fn respawn_player(&mut self, i: usize) {
        self.settle_corpse(i);
        let spot = Player::spawn_point(i);
        let p = &mut self.players[i];
        p.alive = true;
        p.hp = p.max_hp;
        p.pos = spot;
        p.vel = Vec2::ZERO;
        p.stamina = 1.0;
        p.regen_delay = 0.0;
        p.roll_t = 0.0;
        p.respawn_seq = p.respawn_seq.wrapping_add(1);
        self.nav_dirty = true;
        if i == self.local {
            self.announce("BACK IN THE FIGHT", [0.5, 1.0, 0.6], 2.0, false);
        }
        // Clear space on landing so a respawn is never an instant death.
        self.explosions.push(super::combat::Explosion { pos: spot, radius: 7.0, damage: 200.0, from_enemy: false, force: 16.0, spares_players: true, barrel: false });
    }

    /// Lets go of the old body as its player redeploys. If zombies did the killing, it joins them.
    fn settle_corpse(&mut self, i: usize) {
        let Some(b) = self.owned_body(i) else { return };
        if self.players[i].death_cause != DeathCause::Zombie {
            self.ragdolls.bodies[b].owner = -1;
            return;
        }
        let at = self.ragdolls.bodies[b].pos[PELVIS];
        self.ragdolls.bodies.swap_remove(b);
        self.fx.record(|| Ev::CorpseRose { player: i as u8 });
        self.spawn_zombie(super::data::ZKind::Walker, Vec2::new(at.x, at.z));
        if let Some(z) = self.zombies.last_mut() {
            z.style = (z.style & 0x00FF_FFFF) | super::zombie::STYLE_TURNED;
            z.hp *= 1.6;
            z.max_hp = z.hp;
        }
        let name = self.players[i].name.clone();
        let text = if i == self.local { "YOUR OLD BODY RISES".to_string() } else { format!("{name}'S BODY RISES") };
        self.announce(&text, [0.6, 1.0, 0.4], 3.0, false);
        self.snd(Sfx::Groan3, vec3(at.x, 0.0, at.z), 1.0);
    }

    pub fn buy_weapon(&mut self, p: usize, i: usize) -> Result<(), &'static str> {
        let def = WEAPONS.get(i).ok_or("NO SUCH WEAPON")?;
        if self.players[p].owned[i] {
            self.equip(p, i);
            return Ok(());
        }
        if def.unlock_wave > self.wave.max(1) {
            return Err("LOCKED");
        }
        if self.scrap < def.price {
            return Err("NOT ENOUGH SCRAP");
        }
        self.scrap -= def.price;
        self.players[p].owned[i] = true;
        self.players[p].mag[i] = def.mag;
        self.players[p].reserve[i] = def.reserve / 2;
        self.equip(p, i);
        let at = self.players[p].pos;
        self.snd(Sfx::Buy, at, 0.8);
        Ok(())
    }

    pub fn buy_ammo(&mut self, p: usize, i: usize) -> Result<(), &'static str> {
        let def = WEAPONS.get(i).ok_or("NO SUCH WEAPON")?;
        if !self.players[p].owned[i] || def.reserve == u32::MAX {
            return Err("NOT APPLICABLE");
        }
        if self.players[p].reserve[i] >= def.reserve {
            return Err("ALREADY FULL");
        }
        if self.scrap < def.ammo_price {
            return Err("NOT ENOUGH SCRAP");
        }
        self.scrap -= def.ammo_price;
        self.players[p].add_ammo_fraction(i, 0.4);
        let at = self.players[p].pos;
        self.snd(Sfx::Buy, at, 0.8);
        Ok(())
    }

    pub fn buy_grenades(&mut self, p: usize) -> Result<(), &'static str> {
        if self.players[p].grenades >= 9 {
            return Err("ALREADY FULL");
        }
        if self.scrap < 120 {
            return Err("NOT ENOUGH SCRAP");
        }
        self.scrap -= 120;
        self.players[p].grenades += 2;
        let at = self.players[p].pos;
        self.snd(Sfx::Buy, at, 0.8);
        Ok(())
    }
}

pub fn rotate_y(v: Vec3, a: f32) -> Vec3 {
    let (s, c) = a.sin_cos();
    vec3(v.x * c + v.z * s, v.y, -v.x * s + v.z * c)
}

/// Mesh id of a held weapon.
pub fn gun_mesh(w: usize) -> usize {
    WEAPONS.get(w).map_or(m::GUN0, |d| d.mesh)
}
