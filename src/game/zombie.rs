//! Zombie behaviour and procedural animation.
use super::data::*;
use super::map::HALF;
use super::{tile_index, tile_min, tile_of, Phase, World, TILE};
use crate::audio::Sfx;
use crate::ragdoll::*;
use glam::{vec3, Vec2, Vec3};

pub const STYLE_PLAYER: u32 = 0xFFFF_FFFF;

pub struct Zombie {
    pub id: u32,
    pub kind: ZKind,
    pub pos: Vec3,
    pub vel: Vec2,
    pub knock: Vec2,
    pub facing: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub phase: f32,
    pub attack_t: f32,
    pub attack_cd: f32,
    pub attack_target: i32, // -1 player, >=0 structure index, -2 none
    pub touching: i32,
    pub burn: f32,
    pub frost: f32,
    pub stun: f32,
    pub rage: f32,
    pub flash: f32,
    pub special_t: f32,
    pub charge_t: f32,
    pub scale: f32,
    pub radius: f32,
    pub speed: f32,
    pub damage: f32,
    pub struct_damage: f32,
    pub style: u32,
    pub missing: u16,
    pub crawling: bool,
    pub spawn_t: f32,
    pub last_dir: Vec3,
    pub last_force: f32,
    pub last_point: Vec3,
    pub last_gore: f32,
    pub last_crit: bool,
    pub last_blast: bool,
    pub last_cold: bool,
    pub stagger: f32,
    /// How long this zombie has wanted to move without getting anywhere.
    pub stuck_t: f32,
    /// Client side only: where the server last said this zombie is.
    pub net_target: Vec2,
}

impl Zombie {
    #[inline]
    pub fn pos2(&self) -> Vec2 {
        Vec2::new(self.pos.x, self.pos.z)
    }

    /// Procedural pose. Walk cycles are driven by actual motion so feet do not slide much.
    pub fn pose(&self, t: f32) -> Pose {
        let s = self.scale;
        let f = vec3(self.facing.x, 0.0, self.facing.y).normalize_or(Vec3::Z);
        let r = vec3(f.z, 0.0, -f.x);
        let base = self.pos;
        let w = |lx: f32, ly: f32, lz: f32| base + (r * lx + Vec3::Y * ly + f * lz) * s;
        let ph = self.phase;
        let speed = self.vel.length();
        let k = (speed / 1.2).min(1.0);
        let attack = if self.attack_t > 0.0 { (self.attack_t * 3.5).min(1.0) } else { 0.0 };
        let mut p = [Vec3::ZERO; JOINTS];
        let wob = (t * 1.7 + self.id as f32).sin();
        let wob2 = (t * 1.1 + self.id as f32 * 1.7).cos();

        if self.crawling || self.kind == ZKind::Crawler {
            // Dragging along the ground, face down, pulling with the arms.
            let reach = ph.sin();
            p[PELVIS] = w(0.0, 0.2, -0.65);
            p[CHEST] = w(0.0, 0.3, -0.25);
            p[NECK] = w(0.0, 0.38, 0.0);
            p[HEAD] = w(0.0, 0.46 + attack * 0.2, 0.14);
            p[L_SHO] = w(-0.2, 0.36, -0.04);
            p[R_SHO] = w(0.2, 0.36, -0.04);
            p[L_HAND] = w(-0.3, 0.06, 0.3 + reach * 0.28);
            p[R_HAND] = w(0.3, 0.06, 0.3 - reach * 0.28);
            p[L_ELB] = two_bone(p[L_SHO], p[L_HAND], 0.29 * s, 0.27 * s, Vec3::Y - r);
            p[R_ELB] = two_bone(p[R_SHO], p[R_HAND], 0.29 * s, 0.27 * s, Vec3::Y + r);
            p[L_HIP] = w(-0.11, 0.18, -0.7);
            p[R_HIP] = w(0.11, 0.18, -0.7);
            p[L_KNEE] = w(-0.14, 0.1, -1.08 + reach * 0.05);
            p[R_KNEE] = w(0.14, 0.1, -1.08 - reach * 0.05);
            p[L_FOOT] = w(-0.16, 0.06, -1.5);
            p[R_FOOT] = w(0.16, 0.06, -1.5);
            return p;
        }

        let (stride, lift, lean, drunk, arms_up) = match self.kind {
            ZKind::Runner => (0.5 * k, 0.24 * k, 0.32, 0.03, 0.3),
            ZKind::Brute => (0.28 * k, 0.1 * k, 0.28, 0.02, 0.0),
            ZKind::Bloater => (0.2 * k, 0.08 * k, -0.08, 0.1, 0.0),
            ZKind::Spitter => (0.26 * k, 0.1 * k, 0.2, 0.06, 0.1),
            ZKind::Screamer => (0.34 * k, 0.16 * k, 0.06, 0.07, 0.9),
            ZKind::Titan => (0.3 * k, 0.12 * k, 0.2, 0.0, 0.0),
            _ => (0.26 * k, 0.1 * k, 0.16 + 0.05 * wob2, 0.05 + 0.05 * wob2.abs(), 0.72),
        };
        let bob = (ph * 2.0).sin() * 0.03 * k;
        let sway = wob * drunk + (ph).sin() * 0.03 * k;
        let stag = self.stagger * 0.35;
        p[PELVIS] = w(sway * 0.5, 0.93 + bob, -stag * 0.2);
        p[CHEST] = w(sway * 0.8, 1.22 + bob, lean * 0.55 - stag * 0.6);
        p[NECK] = w(sway * 1.1, 1.42 + bob, lean * 1.0 - stag);
        p[HEAD] = w(sway * 1.4 + wob2 * 0.04, 1.57 + bob, lean * 1.25 - stag * 1.1 + 0.02);
        let sho_y = 1.38 + bob;
        p[L_SHO] = w(-0.21 + sway * 0.9, sho_y, lean * 0.9 - stag);
        p[R_SHO] = w(0.21 + sway * 0.9, sho_y, lean * 0.9 - stag);
        p[L_HIP] = w(-0.11 + sway * 0.4, 0.9 + bob, -stag * 0.2);
        p[R_HIP] = w(0.11 + sway * 0.4, 0.9 + bob, -stag * 0.2);

        for (hip, knee, foot, off, side) in [(L_HIP, L_KNEE, L_FOOT, 0.0, -1.0f32), (R_HIP, R_KNEE, R_FOOT, std::f32::consts::PI, 1.0)] {
            let tt = ph + off;
            let target = w(side * 0.14, 0.07 + tt.cos().max(0.0) * lift, tt.sin() * stride);
            p[foot] = target;
            p[knee] = two_bone(p[hip], target, 0.43 * s, 0.43 * s, f + Vec3::Y * 0.1);
        }

        // Arms: outstretched shamble, raising over the head to strike.
        let swing = ph.sin();
        for (sho, elb, hand, side, sw) in [(L_SHO, L_ELB, L_HAND, -1.0f32, swing), (R_SHO, R_ELB, R_HAND, 1.0, -swing)] {
            let shamble_y = 1.05 + arms_up * 0.25 + sw * 0.03 + (t * 2.2 + side + self.id as f32).sin() * 0.03;
            let shamble_z = 0.1 + arms_up * 0.45 + sw * 0.12 * k + lean * 0.5;
            let mut target = w(side * 0.25 + sway, shamble_y, shamble_z);
            if self.kind == ZKind::Runner {
                target = w(side * 0.28 + sway, 1.0 + sw.abs() * 0.1, 0.1 + sw * 0.34 + lean);
            }
            if self.kind == ZKind::Screamer && self.special_t > 3.5 {
                target = w(side * 0.5, 1.55, 0.15);
            }
            if attack > 0.0 {
                // Claw windup and downward strike.
                let stroke = (self.attack_t * 3.0 + if side < 0.0 { 0.0 } else { 0.6 }).sin();
                let strike = w(side * 0.2, 1.3 + stroke * 0.42, 0.56 + stroke.abs() * 0.15 + lean);
                target = target.lerp(strike, attack);
            }
            p[hand] = target;
            p[elb] = two_bone(p[sho], target, 0.29 * s, 0.27 * s, -Vec3::Y + r * side * 0.8);
        }

        // Rising out of the dirt as they spawn.
        if self.spawn_t > 0.0 {
            let drop = self.spawn_t.min(1.0) * 1.1 * s;
            for j in &mut p {
                j.y -= drop;
            }
        }
        p
    }
}

/// Clothes and skin picked by a per-zombie style number.
pub fn style_colors(style: u32) -> ([f32; 3], [f32; 3], [f32; 3]) {
    const SKIN: [[f32; 3]; 5] = [[0.40, 0.46, 0.30], [0.48, 0.50, 0.36], [0.34, 0.38, 0.30], [0.50, 0.42, 0.36], [0.30, 0.34, 0.24]];
    const SHIRT: [[f32; 3]; 8] = [
        [0.25, 0.18, 0.15], [0.20, 0.26, 0.34], [0.34, 0.12, 0.10], [0.16, 0.24, 0.16],
        [0.42, 0.38, 0.30], [0.14, 0.14, 0.17], [0.33, 0.25, 0.10], [0.28, 0.14, 0.26],
    ];
    const PANTS: [[f32; 3]; 4] = [[0.12, 0.14, 0.20], [0.18, 0.15, 0.10], [0.10, 0.10, 0.11], [0.20, 0.20, 0.16]];
    (SKIN[(style & 0xFF) as usize % 5], SHIRT[((style >> 8) & 0xFF) as usize % 8], PANTS[((style >> 16) & 0xFF) as usize % 4])
}

impl World {
    pub fn spawn_zombie(&mut self, kind: ZKind, at: Vec2) {
        let d = zdef(kind);
        let n = self.wave.max(1) as f32 - 1.0;
        let hp_scale = 1.0 + 0.10 * n + 0.004 * n * n;
        let dmg_scale = 1.0 + 0.06 * n;
        let scale_var = if kind == ZKind::Titan { 1.0 } else { self.rng.range(0.93, 1.08) };
        let style = self.rng.next_u32() & 0x00FF_FFFF | ((kind as u32) << 24);
        let at = Vec2::new(at.x.clamp(-HALF + 1.0, HALF - 1.0), at.y.clamp(-HALF + 1.0, HALF - 1.0));
        let id = self.next_zombie_id;
        self.next_zombie_id += 1;
        self.zombies.push(Zombie {
            id,
            kind,
            pos: vec3(at.x, 0.0, at.y),
            vel: Vec2::ZERO,
            knock: Vec2::ZERO,
            facing: (-at).normalize_or(Vec2::Y),
            hp: d.hp * hp_scale,
            max_hp: d.hp * hp_scale,
            phase: self.rng.range(0.0, 6.28),
            attack_t: 0.0,
            attack_cd: self.rng.range(0.0, 0.5),
            attack_target: -2,
            touching: -1,
            burn: 0.0,
            frost: 0.0,
            stun: 0.0,
            rage: 0.0,
            flash: 0.0,
            special_t: self.rng.range(0.0, 3.0),
            charge_t: 0.0,
            scale: d.scale * scale_var,
            radius: d.radius * scale_var,
            speed: d.speed * self.rng.range(0.88, 1.15),
            damage: d.damage * dmg_scale,
            struct_damage: d.struct_damage * dmg_scale,
            style,
            missing: 0,
            crawling: false,
            spawn_t: 0.9,
            last_dir: Vec3::Z,
            last_force: 0.0,
            last_point: Vec3::ZERO,
            last_gore: 0.0,
            last_crit: false,
            last_blast: false,
            last_cold: false,
            stagger: 0.0,
            stuck_t: 0.0,
            net_target: at,
        });
    }

    pub fn update_zombies(&mut self, dt: f32) {
        let count = self.zombies.len();
        let assault_ending = self.spawn_budget <= 0.0 && matches!(self.phase, Phase::Assault);

        // Separation forces are gathered first, with the spatial hash built this frame.
        let mut push: Vec<Vec2> = vec![Vec2::ZERO; count];
        for i in 0..count {
            let pi = self.zombies[i].pos2();
            let ri = self.zombies[i].radius;
            let cx = ((pi.x + HALF) / super::ZHASH_CELL) as i32;
            let cz = ((pi.y + HALF) / super::ZHASH_CELL) as i32;
            let mut checks = 0;
            'outer: for nz in (cz - 1).max(0)..=(cz + 1).min(super::ZHASH_DIM as i32 - 1) {
                for nx in (cx - 1).max(0)..=(cx + 1).min(super::ZHASH_DIM as i32 - 1) {
                    let mut it = self.zhash_head[nz as usize * super::ZHASH_DIM + nx as usize];
                    while it >= 0 {
                        let j = it as usize;
                        it = self.zhash_next[j];
                        if j == i || j >= count {
                            continue;
                        }
                        checks += 1;
                        if checks > 24 {
                            break 'outer;
                        }
                        let o = &self.zombies[j];
                        let d = pi - o.pos2();
                        let min = ri + o.radius;
                        let l2 = d.length_squared();
                        if l2 < min * min && l2 > 1e-6 {
                            let l = l2.sqrt();
                            let weight = o.radius / (ri + o.radius);
                            push[i] += d / l * (min - l) * weight;
                        }
                    }
                }
            }
        }

        let mut spit_requests: Vec<(Vec3, Vec3)> = Vec::new();
        let mut melee: Vec<(usize, i32)> = Vec::new();
        let mut screams: Vec<Vec2> = Vec::new();
        let mut slam: Vec<Vec3> = Vec::new();
        let mut roars: Vec<(Sfx, Vec3, f32)> = Vec::new();
        let mut burrow: Vec<usize> = Vec::new();

        for i in 0..count {
            let z = &mut self.zombies[i];
            z.flash = (z.flash - dt * 6.0).max(0.0);
            z.frost = (z.frost - dt * 0.6).max(0.0);
            z.stun = (z.stun - dt).max(0.0);
            z.rage = (z.rage - dt).max(0.0);
            z.attack_cd -= dt;
            z.special_t += dt;
            z.stagger = (z.stagger - dt * 3.0).max(0.0);
            z.charge_t = (z.charge_t - dt).max(0.0);
            if z.spawn_t > 0.0 {
                z.spawn_t -= dt;
                continue;
            }
            if z.burn > 0.0 {
                z.burn -= dt;
                z.hp -= 16.0 * dt * (1.0 + z.max_hp / 800.0);
                z.last_gore = z.last_gore.max(0.1);
            }

            let p2 = z.pos2();
            // Every zombie hunts whichever player is closest.
            let mut nearest: Option<(Vec2, f32)> = None;
            for pl in &self.players {
                if pl.alive && pl.connected {
                    let d = pl.pos2().distance_squared(p2);
                    if nearest.is_none_or(|n| d < n.1) {
                        nearest = Some((pl.pos2(), d));
                    }
                }
            }
            let palive = nearest.is_some();
            let ppos = nearest.map_or(Vec2::ZERO, |n| n.0);
            let to_player = ppos - p2;
            let dplayer = to_player.length();
            let mut want = self.nav.direction(p2);
            let mut chase_player = false;
            // Blocked cells have no flow: head for the reactor to get back onto the field.
            if want.length_squared() < 0.01 {
                want = (-p2).normalize_or(Vec2::X);
            }
            // Near the player, head straight for them rather than following coarse flow.
            if palive && dplayer < 7.0 && dplayer > 0.05 {
                want = to_player / dplayer;
                chase_player = true;
            }
            let def_reach = zdef(z.kind).reach * z.scale.max(1.0).min(1.5);
            let mut speed_mult = 1.0;
            if z.frost > 0.0 {
                speed_mult *= (1.0 - z.frost * 0.5).clamp(0.0, 1.0);
            }
            if z.rage > 0.0 {
                speed_mult *= 1.6;
            }
            if z.charge_t > 0.0 {
                speed_mult *= 2.4;
            }
            if z.stun > 0.0 {
                speed_mult *= 0.1;
            }
            if assault_ending && count < 12 {
                speed_mult *= 1.5; // Stragglers hurry up so waves always resolve.
            }

            match z.kind {
                ZKind::Spitter => {
                    if palive && dplayer < 15.0 {
                        if dplayer > 7.5 && z.attack_cd <= 0.0 && z.frost < 1.0 {
                            z.attack_cd = zdef(ZKind::Spitter).attack_time * self.rng.range(0.8, 1.3);
                            z.attack_t = 0.01;
                            spit_requests.push((z.pos + Vec3::Y * 1.5 * z.scale, vec3(ppos.x, 0.2, ppos.y)));
                        }
                        if dplayer < 11.0 && dplayer > 0.1 {
                            want = -to_player / dplayer * 0.5; // Back away to keep lobbing.
                        } else if dplayer < 13.0 {
                            want = Vec2::ZERO;
                        }
                    }
                }
                ZKind::Screamer => {
                    if z.special_t > 6.0 {
                        z.special_t = 0.0;
                        screams.push(p2);
                        roars.push((Sfx::Scream, z.pos, 1.0));
                    }
                }
                ZKind::Brute => {
                    if palive && dplayer < 14.0 && dplayer > 4.0 && z.special_t > 5.0 {
                        z.special_t = 0.0;
                        z.charge_t = 1.1;
                        roars.push((Sfx::BruteRoar, z.pos, 1.0));
                    }
                }
                _ => {}
            }

            // Melee decisions.
            if z.attack_t > 0.0 {
                z.attack_t += dt;
                speed_mult *= 0.25;
                let at = zdef(z.kind).attack_time * 0.5;
                if z.attack_t > at {
                    z.attack_t = 0.0;
                    if z.kind == ZKind::Titan {
                        slam.push(z.pos);
                    } else if z.kind != ZKind::Spitter {
                        melee.push((i, z.attack_target));
                    }
                }
            } else if z.attack_cd <= 0.0 && z.stun <= 0.0 && z.frost < 1.6 {
                let mut target = -2;
                if palive && dplayer < def_reach + 0.25 {
                    target = -1;
                } else if z.touching >= 0 {
                    target = z.touching;
                } else if z.kind == ZKind::Titan && palive && dplayer < 5.0 {
                    target = -1;
                }
                if target != -2 && z.kind != ZKind::Spitter {
                    z.attack_t = 0.01;
                    z.attack_cd = zdef(z.kind).attack_time;
                    z.attack_target = target;
                    if self.rng.chance(0.3) {
                        roars.push((Sfx::Attack, z.pos, 0.5));
                    }
                } else if target >= 0 {
                    // Spitters chew walls too when boxed in.
                    z.attack_t = 0.01;
                    z.attack_cd = 1.6;
                    z.attack_target = target;
                }
            }

            // Motion.
            let crawl_mult = if z.crawling { 0.6 } else { 1.0 };
            let desired = want * z.speed * speed_mult * crawl_mult;
            z.vel += (desired - z.vel) * (6.0 * dt).min(1.0);
            let drag = (1.0 - 5.0 * dt).max(0.0);
            z.knock *= drag;
            let step = (z.vel + z.knock) * dt + push[i] * (10.0 * dt).min(0.6);
            let mut np = p2 + step;
            np = np.clamp(Vec2::splat(-HALF + 0.5), Vec2::splat(HALF - 0.5));
            np = self.map.resolve_circle(np, z.radius);

            // Structures physically stop them; note what we are leaning on.
            z.touching = -1;
            let t = tile_of(np);
            let probe = if want.length_squared() > 0.1 { want } else { z.facing };
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let c = (t.0 + dx, t.1 + dz);
                    let Some(ti) = tile_index(c) else { continue };
                    if !self.blocked_tiles[ti] {
                        continue;
                    }
                    let min = tile_min(c);
                    if let Some(n) = super::map::push_out_circle(np, z.radius, min, min + Vec2::splat(TILE)) {
                        let center = min + Vec2::splat(TILE * 0.5);
                        if (center - np).normalize_or_zero().dot(probe) > 0.2 {
                            z.touching = self.tiles[ti];
                        }
                        np = n;
                    }
                }
            }
            // Bodies keep out of the players' personal space.
            for pl in &self.players {
                if pl.alive && pl.connected {
                    let q = pl.pos2();
                    let d = np - q;
                    let min = z.radius + super::player::RADIUS;
                    let l = d.length();
                    if l < min && l > 1e-4 {
                        np = q + d / l * min;
                    }
                }
            }

            let moved = np - p2;
            // Nothing may wedge forever: one stuck zombie would hold up the whole wave.
            let trying = want.length_squared() > 0.1 && z.attack_t <= 0.0 && z.touching < 0 && z.stun <= 0.0 && z.frost < 0.5;
            if trying && moved.length() < z.speed * dt * 0.2 {
                z.stuck_t += dt;
            } else if moved.length() > z.speed * dt * 0.5 {
                z.stuck_t = (z.stuck_t - dt * 2.0).max(0.0);
            }
            if z.stuck_t > 2.0 {
                burrow.push(i);
            }
            z.phase += moved.length() * if z.kind == ZKind::Runner { 1.7 } else { 2.6 } / z.scale;
            z.pos.x = np.x;
            z.pos.z = np.y;
            let face = if chase_player && dplayer > 0.05 { to_player } else if z.vel.length_squared() > 0.02 { z.vel } else { z.facing };
            let target_face = face.normalize_or(z.facing);
            z.facing = (z.facing + (target_face - z.facing) * (7.0 * dt).min(1.0)).normalize_or(target_face);
        }

        for i in burrow {
            self.burrow_zombie(i);
        }
        for (s, p, g) in roars {
            self.snd(s, p, g);
        }
        for (from, to) in spit_requests {
            self.spawn_spit(from, to);
            self.snd(Sfx::Spit, from, 0.8);
        }
        for p in screams {
            for z in &mut self.zombies {
                if z.pos2().distance_squared(p) < 196.0 && z.kind != ZKind::Screamer {
                    z.rage = 5.0;
                }
            }
            let at = vec3(p.x, 1.5, p.y);
            for k in 0..28 {
                let a = k as f32 / 28.0 * 6.283;
                let d = vec3(a.cos(), 0.1, a.sin());
                self.fx.frost_puff(at, d * 12.0, 0.18, 0.5);
            }
            self.fx.light(at, 14.0, vec3(3.0, 6.0, 14.0), 0.4);
            self.shake(0.12);
        }
        for p in slam {
            self.titan_slam(p);
        }
        for (i, target) in melee {
            if i >= self.zombies.len() {
                continue;
            }
            let z = &self.zombies[i];
            let (dmg, sdmg, pos, kind) = (z.damage, z.struct_damage, z.pos, z.kind);
            let reach = zdef(kind).reach * z.scale.max(1.0).min(1.5) + 0.5;
            if target == -1 {
                let victim = (0..self.players.len())
                    .filter(|&p| self.players[p].alive && self.players[p].connected)
                    .min_by(|&a, &b| self.players[a].pos.distance_squared(pos).total_cmp(&self.players[b].pos.distance_squared(pos)));
                if let Some(v) = victim {
                    if self.players[v].pos.distance(pos) < reach + 0.1 {
                        self.hurt_player(v, dmg, pos);
                        if kind == ZKind::Brute {
                            let d = (self.players[v].pos2() - Vec2::new(pos.x, pos.z)).normalize_or_zero();
                            self.players[v].vel += d * 12.0;
                        }
                    }
                }
            } else if target >= 0 && (target as usize) < self.structures.len() {
                self.hurt_structure(target as usize, sdmg, pos);
            }
        }
    }

    /// Digs a wedged zombie out: it sinks and climbs back up further along its route.
    fn burrow_zombie(&mut self, i: usize) {
        let (from, radius) = (self.zombies[i].pos2(), self.zombies[i].radius);
        let mut p = from;
        let mut best = None;
        for _ in 0..40 {
            let d = self.nav.direction(p);
            if d.length_squared() < 0.1 {
                // Inside a blocked cell or at a seed: nudge toward the reactor to find flow again.
                p += (-p).normalize_or(Vec2::X);
            } else {
                p += d;
            }
            if p.distance(from) > 4.0 && self.nav.reachable(p) && !self.map.is_blocked(p, radius + 0.1) {
                let tile = tile_of(p);
                let free = tile_index(tile).is_some_and(|t| !self.blocked_tiles[t]);
                if free {
                    best = Some(p);
                    break;
                }
            }
        }
        let z = &mut self.zombies[i];
        z.stuck_t = 0.0;
        if let Some(p) = best {
            z.pos.x = p.x;
            z.pos.z = p.y;
            z.vel = Vec2::ZERO;
            z.knock = Vec2::ZERO;
            z.spawn_t = 0.9;
        }
    }

    /// Fire, ice and rage effects on living zombies. Lights are budgeted per frame.
    pub fn zombie_status_fx(&mut self, dt: f32) {
        let mut light_budget = 10;
        let mut spread: Option<Vec3> = None;
        for i in 0..self.zombies.len() {
            let (burn, pos, scale) = {
                let z = &self.zombies[i];
                (z.burn, z.pos, z.scale)
            };
            if burn > 0.0 {
                if self.rng.chance(dt * 26.0) {
                    let off = self.rng.dir3() * 0.25 * scale;
                    let at = pos + Vec3::Y * self.rng.range(0.3, 1.5) * scale + off;
                    let v = Vec3::Y * self.rng.range(0.8, 2.2);
                    let life = self.rng.range(0.25, 0.5);
                    self.fx.fire_puff(at, v, 0.11 * scale, life);
                }
                if light_budget > 0 {
                    light_budget -= 1;
                    self.fx.light(pos + Vec3::Y, 5.0, vec3(4.5, 2.0, 0.5), 0.03);
                }
                if self.rng.chance(dt * 0.35) {
                    spread = Some(pos);
                }
            }
        }
        // Burning bodies set their neighbours alight, one contact at a time.
        if let Some(p) = spread {
            for z in &mut self.zombies {
                if z.burn <= 0.0 && z.pos.distance_squared(p) < 2.2 {
                    z.burn = 3.0;
                    break;
                }
            }
        }
    }

    fn titan_slam(&mut self, pos: Vec3) {
        self.shake(0.6);
        self.snd(Sfx::ExplosionBig, pos, 1.0);
        self.fx.decal(pos.x, pos.z, 5.0, [0.02, 0.02, 0.02, 0.8], 1.0);
        for k in 0..40 {
            let a = k as f32 / 40.0 * 6.283;
            let d = vec3(a.cos(), 0.18, a.sin());
            self.fx.smoke(pos + d * 1.5 + Vec3::Y * 0.3, d * 9.0, 0.6, 1.0, 0.12, 0.5);
            self.fx.debris(pos + d, d * self.rng.range(4.0, 10.0) + Vec3::Y * 5.0, crate::render::mesh::GIB, Vec3::splat(0.14), [0.15, 0.13, 0.11], 3.0, false);
        }
        self.blast(pos, 8.0, 12.0);
        for p in 0..self.players.len() {
            if self.players[p].alive && self.players[p].connected && self.players[p].pos.distance(pos) < 6.5 {
                let dmg = 28.0 + self.wave as f32;
                self.hurt_player(p, dmg, pos);
            }
        }
        let list: Vec<usize> = self.structures.iter().enumerate().filter(|(_, s)| s.pos.distance(pos) < 6.5).map(|(i, _)| i).collect();
        for i in list {
            self.hurt_structure(i, 300.0, pos);
        }
    }
}
