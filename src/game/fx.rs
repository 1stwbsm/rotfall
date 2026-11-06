//! CPU particles, debris, ground decals and transient lights: the "chaos" layer.
use crate::render::{mesh as m, Decal, Instance, Particle, PointLight, MAX_PARTICLES};
use crate::util::Rng;
use glam::{vec3, Mat4, Quat, Vec3};

#[derive(Clone, Copy)]
pub struct P {
    pub pos: Vec3,
    pub vel: Vec3,
    pub size: f32,
    pub grow: f32,
    pub life: f32,
    pub max_life: f32,
    pub c0: [f32; 4],
    pub c1: [f32; 4],
    pub gravity: f32,
    pub drag: f32,
    pub shape: f32,
    /// Stretch along velocity (seconds of travel shown). 0 means a round sprite.
    pub stretch: f32,
    /// Paints a blood decal where it lands.
    pub blood: bool,
}

#[derive(Clone, Copy)]
struct Debris {
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    spin: Vec3,
    scale: Vec3,
    tint: [f32; 3],
    mesh: u16,
    life: f32,
    glow: f32,
    bleeds: bool,
    rest: bool,
}

#[derive(Clone, Copy)]
pub struct TempLight {
    pub pos: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub life: f32,
    pub max: f32,
}

const MAX_DEBRIS: usize = 900;

/// A replicated happening. The server records these as they occur; clients replay them,
/// with their own random numbers, since no two spark showers need to match.
#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    Blood { pos: Vec3, dir: Vec3, amount: u8, speed: f32 },
    Splat { pos: Vec3, size: f32 },
    Muzzle { pos: Vec3, dir: Vec3, size: f32, color: Vec3 },
    Tracer { from: Vec3, to: Vec3, color: [f32; 3], width: f32 },
    Spark { pos: Vec3, vel: Vec3, color: [f32; 3], life: f32 },
    Smoke { pos: Vec3, vel: Vec3, size: f32, life: f32, shade: f32, opacity: f32 },
    Fire { pos: Vec3, vel: Vec3, size: f32, life: f32 },
    Frost { pos: Vec3, vel: Vec3, size: f32, life: f32 },
    Lightning { from: Vec3, to: Vec3, color: [f32; 3], width: f32 },
    Explosion { pos: Vec3, radius: f32 },
    Debris { pos: Vec3, vel: Vec3, mesh: u16, scale: Vec3, tint: [f32; 3], life: f32, bleeds: bool },
    Casing { pos: Vec3, right: Vec3 },
    Gibs { pos: Vec3, count: u8, speed: f32, tint: [f32; 3] },
    Decal { x: f32, z: f32, size: f32, color: [f32; 4], kind: u8 },
    Light { pos: Vec3, radius: f32, color: Vec3, life: f32 },
    Glow { pos: Vec3, vel: Vec3, size: f32, grow: f32, life: f32, c0: [f32; 4], c1: [f32; 4] },
    Snd { id: u8, pos: Vec3, gain: f32 },
    SndUi { id: u8, gain: f32 },
    Msg { text: String, color: [f32; 3], ttl: f32, big: bool },
    Shake { amount: f32 },
    Blast { pos: Vec3, radius: f32, force: f32 },
    ZombieDied { id: u32, dir: Vec3, point: Vec3, force: f32, gore: f32, flags: u8 },
    Severed { id: u32, mask: u16 },
    PlayerDied { player: u8, away: Vec3, cause: u8 },
    PlayerDown { player: u8, from: Vec3, force: f32 },
    CorpseRose { player: u8 },
    Recoil { player: u8, kick: f32 },
}

impl Ev {
    /// Cosmetic chaff that may be dropped when a snapshot is full.
    pub fn is_chaff(&self) -> bool {
        matches!(self, Ev::Spark { .. } | Ev::Smoke { .. } | Ev::Fire { .. } | Ev::Frost { .. } | Ev::Casing { .. } | Ev::Debris { .. } | Ev::Glow { .. } | Ev::Light { .. })
    }
}

pub struct Fx {
    pub p: Vec<P>,
    debris: Vec<Debris>,
    pub lights: Vec<TempLight>,
    pub decals: Vec<Decal>,
    pub rng: Rng,
    pub clear_decals: bool,
    /// Events since the network layer last collected them. None unless hosting.
    pub rec: Option<Vec<Ev>>,
    /// Nesting depth of composite effects and muted sections, which do not record.
    quiet: u32,
}

const BLOOD: [f32; 4] = [0.20, 0.008, 0.008, 1.0];
const DECAL_BLOOD: [f32; 4] = [0.075, 0.004, 0.004, 0.9];

impl Fx {
    pub fn new(seed: u64) -> Self {
        Fx {
            p: Vec::with_capacity(MAX_PARTICLES),
            debris: Vec::with_capacity(MAX_DEBRIS),
            lights: Vec::with_capacity(128),
            decals: Vec::with_capacity(256),
            rng: Rng::new(seed),
            clear_decals: true,
            rec: None,
            quiet: 0,
        }
    }

    #[inline]
    pub fn record(&mut self, e: impl FnOnce() -> Ev) {
        if self.quiet == 0 {
            if let Some(r) = &mut self.rec {
                if r.len() < 20_000 {
                    r.push(e());
                }
            }
        }
    }

    /// While muted, nothing is recorded: used for effects clients derive for themselves.
    pub fn muted(&mut self, on: bool) {
        if on {
            self.quiet += 1;
        } else {
            self.quiet = self.quiet.saturating_sub(1);
        }
    }

    /// Replays an event that arrived from the server.
    pub fn replay(&mut self, e: &Ev) {
        match e.clone() {
            Ev::Blood { pos, dir, amount, speed } => self.blood(pos, dir, amount as u32, speed),
            Ev::Splat { pos, size } => self.blood_splat(pos, size),
            Ev::Muzzle { pos, dir, size, color } => self.muzzle_flash(pos, dir, size, color),
            Ev::Tracer { from, to, color, width } => self.tracer(from, to, color, width),
            Ev::Spark { pos, vel, color, life } => self.spark(pos, vel, color, life),
            Ev::Smoke { pos, vel, size, life, shade, opacity } => self.smoke(pos, vel, size, life, shade, opacity),
            Ev::Fire { pos, vel, size, life } => self.fire_puff(pos, vel, size, life),
            Ev::Frost { pos, vel, size, life } => self.frost_puff(pos, vel, size, life),
            Ev::Lightning { from, to, color, width } => self.lightning(from, to, color, width),
            Ev::Explosion { pos, radius } => self.explosion(pos, radius),
            Ev::Debris { pos, vel, mesh, scale, tint, life, bleeds } => self.debris(pos, vel, mesh as usize, scale, tint, life, bleeds),
            Ev::Casing { pos, right } => self.casing(pos, right),
            Ev::Gibs { pos, count, speed, tint } => self.gibs(pos, count as u32, speed, tint),
            Ev::Decal { x, z, size, color, kind } => self.decal(x, z, size, color, kind as f32),
            // Continuous emitters refresh a light every server frame. Snapshots batch those
            // together, so stretch the life to bridge the gap to the next snapshot.
            Ev::Light { pos, radius, color, life } => self.light(pos, radius, color, life.max(0.05)),
            Ev::Glow { pos, vel, size, grow, life, c0, c1 } => self.glow(pos, vel, size, grow, life, c0, c1),
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn glow(&mut self, pos: Vec3, vel: Vec3, size: f32, grow: f32, life: f32, c0: [f32; 4], c1: [f32; 4]) {
        self.record(|| Ev::Glow { pos, vel, size, grow, life, c0, c1 });
        self.add(P { pos, vel, size, grow, life, max_life: life, c0, c1, gravity: 0.0, drag: 0.5, shape: 0.0, stretch: 0.0, blood: false });
    }

    #[inline]
    pub fn add(&mut self, p: P) {
        if self.p.len() < MAX_PARTICLES {
            self.p.push(p);
        }
    }

    pub fn light(&mut self, pos: Vec3, radius: f32, color: Vec3, life: f32) {
        self.record(|| Ev::Light { pos, radius, color, life });
        if self.lights.len() < 180 {
            self.lights.push(TempLight { pos, radius, color, life, max: life });
        }
    }

    pub fn decal(&mut self, x: f32, z: f32, size: f32, color: [f32; 4], kind: f32) {
        self.record(|| Ev::Decal { x, z, size, color, kind: kind as u8 });
        if self.decals.len() >= 900 {
            return;
        }
        let seed = self.rng.f();
        let rot = self.rng.range(0.0, 6.28);
        self.decals.push(Decal { center: [x, z], size, rot, color, params: [seed, kind, super::map::HALF, 0.0] });
    }

    pub fn blood_splat(&mut self, pos: Vec3, size: f32) {
        self.record(|| Ev::Splat { pos, size });
        self.quiet += 1;
        self.decal(pos.x, pos.z, size, DECAL_BLOOD, 0.0);
        self.quiet -= 1;
    }

    /// Directional gout of blood droplets; they paint the ground where they land.
    pub fn blood(&mut self, pos: Vec3, dir: Vec3, amount: u32, speed: f32) {
        self.record(|| Ev::Blood { pos, dir, amount: amount.min(255) as u8, speed });
        for i in 0..amount {
            let v = (dir + self.rng.dir3() * 0.65).normalize_or_zero() * self.rng.range(0.3, 1.0) * speed + Vec3::Y * 1.5;
            let size = self.rng.range(0.025, 0.075);
            let life = self.rng.range(0.25, 0.8);
            self.add(P {
                pos,
                vel: v,
                size,
                grow: 0.0,
                life,
                max_life: life,
                c0: BLOOD,
                c1: BLOOD,
                gravity: -13.0,
                drag: 0.5,
                shape: 2.0,
                stretch: 0.012,
                blood: i % 3 == 0,
            });
        }
    }

    pub fn muzzle_flash(&mut self, pos: Vec3, dir: Vec3, size: f32, color: Vec3) {
        self.record(|| Ev::Muzzle { pos, dir, size, color });
        self.quiet += 1;
        let c = [color.x * 7.0, color.y * 7.0, color.z * 7.0, 0.0];
        self.add(P { pos: pos + dir * size * 0.5, vel: dir, size: size * 0.6, grow: 0.0, life: 0.05, max_life: 0.05, c0: c, c1: c, gravity: 0.0, drag: 0.0, shape: 0.0, stretch: size * 1.5, blood: false });
        self.add(P { pos, vel: Vec3::ZERO, size: size * 1.1, grow: 0.0, life: 0.04, max_life: 0.05, c0: c, c1: [c[0], c[1], c[2], 0.0], gravity: 0.0, drag: 0.0, shape: 0.0, stretch: 0.0, blood: false });
        self.light(pos, 7.5 * size.max(0.5).sqrt() + 4.0, color * 8.0, 0.06);
        for _ in 0..2 {
            let v = (dir * 8.0 + self.rng.dir3() * 4.0) * self.rng.range(0.4, 1.0);
            self.spark(pos, v, [5.0, 3.0, 1.0], 0.14);
        }
        self.quiet -= 1;
    }

    pub fn tracer(&mut self, from: Vec3, to: Vec3, color: [f32; 3], width: f32) {
        self.record(|| Ev::Tracer { from, to, color, width });
        let life = 0.055;
        let dir = to - from;
        // `stretch` means "world-space vector back along the streak" with velocity = dir.
        let c = [color[0], color[1], color[2], 0.0];
        self.add(P { pos: to, vel: dir, size: width, grow: 0.0, life, max_life: life, c0: c, c1: [c[0] * 0.2, c[1] * 0.2, c[2] * 0.2, 0.0], gravity: 0.0, drag: 0.0, shape: 0.0, stretch: -1.0, blood: false });
    }

    pub fn spark(&mut self, pos: Vec3, vel: Vec3, color: [f32; 3], life: f32) {
        self.record(|| Ev::Spark { pos, vel, color, life });
        let c = [color[0], color[1], color[2], 0.0];
        self.add(P { pos, vel, size: 0.03, grow: 0.0, life, max_life: life, c0: c, c1: [c[0] * 0.3, c[1] * 0.2, 0.0, 0.0], gravity: -11.0, drag: 0.8, shape: 0.0, stretch: 0.03, blood: false });
    }

    pub fn smoke(&mut self, pos: Vec3, vel: Vec3, size: f32, life: f32, shade: f32, opacity: f32) {
        self.record(|| Ev::Smoke { pos, vel, size, life, shade, opacity });
        let c0 = [shade * opacity, shade * opacity, shade * opacity, opacity];
        self.add(P { pos, vel, size, grow: size * 1.4, life, max_life: life, c0, c1: [0.0, 0.0, 0.0, 0.0], gravity: 0.5, drag: 1.2, shape: 1.0, stretch: 0.0, blood: false });
    }

    pub fn fire_puff(&mut self, pos: Vec3, vel: Vec3, size: f32, life: f32) {
        self.record(|| Ev::Fire { pos, vel, size, life });
        let c0 = [9.0, 3.6, 0.7, 0.0];
        let c1 = [1.0, 0.12, 0.02, 0.0];
        self.add(P { pos, vel, size, grow: size * 0.9, life, max_life: life, c0, c1, gravity: 2.5, drag: 1.5, shape: 1.0, stretch: 0.0, blood: false });
    }

    pub fn frost_puff(&mut self, pos: Vec3, vel: Vec3, size: f32, life: f32) {
        self.record(|| Ev::Frost { pos, vel, size, life });
        let c0 = [1.4, 3.2, 5.0, 0.03];
        let c1 = [0.25, 0.5, 0.8, 0.0];
        self.add(P { pos, vel, size, grow: size * 1.2, life, max_life: life, c0, c1, gravity: -1.0, drag: 2.5, shape: 1.0, stretch: 0.0, blood: false });
    }

    pub fn lightning(&mut self, from: Vec3, to: Vec3, color: [f32; 3], width: f32) {
        self.record(|| Ev::Lightning { from, to, color, width });
        self.quiet += 1;
        let segs = ((from.distance(to) / 1.1).ceil() as usize).clamp(2, 12);
        let mut prev = from;
        for i in 1..=segs {
            let t = i as f32 / segs as f32;
            let mut cur = from.lerp(to, t);
            if i < segs {
                cur += self.rng.dir3() * 0.35;
            }
            self.tracer(prev, cur, color, width);
            prev = cur;
        }
        self.light(to, 7.0, Vec3::from(color) * 0.6, 0.1);
        self.quiet -= 1;
    }

    pub fn explosion(&mut self, pos: Vec3, radius: f32) {
        self.record(|| Ev::Explosion { pos, radius });
        self.quiet += 1;
        let flash = radius * 0.9;
        let c = [24.0, 11.0, 3.2, 0.0];
        self.add(P { pos: pos + Vec3::Y * 0.5, vel: Vec3::ZERO, size: flash, grow: flash * 2.5, life: 0.13, max_life: 0.13, c0: c, c1: [8.0, 1.5, 0.1, 0.0], gravity: 0.0, drag: 0.0, shape: 0.0, stretch: 0.0, blood: false });
        self.light(pos + Vec3::Y * 1.5, radius * 4.2, vec3(50.0, 25.0, 8.0), 0.3);
        let n = (radius * 7.0) as u32;
        for _ in 0..n {
            let d = (self.rng.dir3() + Vec3::Y * 0.65).normalize();
            let sp = self.rng.range(1.0, 3.0) * radius;
            let life = self.rng.range(0.35, 0.8);
            let size = self.rng.range(0.22, 0.42) * radius.sqrt();
            self.fire_puff(pos + d * 0.4, d * sp, size, life);
        }
        for _ in 0..(radius * 4.0) as u32 {
            let d = (self.rng.dir3() + Vec3::Y * 0.5).normalize();
            let life = self.rng.range(1.2, 3.2);
            let size = self.rng.range(0.4, 0.8) * radius.sqrt();
            let shade = self.rng.range(0.03, 0.07);
            let sp = self.rng.range(0.4, 1.8);
            self.smoke(pos + d * 0.5, d * sp * radius, size, life, shade, 0.75);
        }
        for _ in 0..(radius * 14.0) as u32 {
            let d = (self.rng.dir3() + Vec3::Y * 0.4).normalize();
            let life = self.rng.range(0.4, 1.3);
            let sp = self.rng.range(6.0, 24.0);
            self.spark(pos + Vec3::Y * 0.3, d * sp, [12.0, 6.0, 1.5], life);
        }
        // Dirt chunks.
        for _ in 0..(radius * 1.7) as u32 {
            let d = (self.rng.dir3() + Vec3::Y * 0.9).normalize();
            let v = d * self.rng.range(4.0, 14.0);
            let s = self.rng.range(0.06, 0.18);
            self.debris(pos + Vec3::Y * 0.2, v, m::GIB, Vec3::splat(s), [0.1, 0.09, 0.08], 3.5, false);
        }
        self.decal(pos.x, pos.z, radius * 0.75, [0.015, 0.012, 0.01, 0.92], 1.0);
        self.quiet -= 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn debris(&mut self, pos: Vec3, vel: Vec3, mesh: usize, scale: Vec3, tint: [f32; 3], life: f32, bleeds: bool) {
        self.record(|| Ev::Debris { pos, vel, mesh: mesh as u16, scale, tint, life, bleeds });
        if self.debris.len() >= MAX_DEBRIS {
            let idx = self.rng.int(self.debris.len());
            self.debris.swap_remove(idx);
        }
        let rot = Quat::from_axis_angle(self.rng.dir3().normalize_or(Vec3::Y), self.rng.range(0.0, 6.28));
        let spin = self.rng.dir3() * self.rng.range(4.0, 16.0);
        self.debris.push(Debris { pos, vel, rot, spin, scale, tint, mesh: mesh as u16, life, glow: 0.0, bleeds, rest: false });
    }

    pub fn casing(&mut self, pos: Vec3, right: Vec3) {
        self.record(|| Ev::Casing { pos, right });
        self.quiet += 1;
        let v = right * self.rng.range(2.0, 4.0) + Vec3::Y * self.rng.range(1.5, 3.5);
        self.debris(pos, v, m::CASING, vec3(0.022, 0.05, 0.022), [1.0; 3], 1.8, false);
        self.quiet -= 1;
    }

    pub fn gibs(&mut self, pos: Vec3, count: u32, speed: f32, tint: [f32; 3]) {
        self.record(|| Ev::Gibs { pos, count: count.min(255) as u8, speed, tint });
        self.quiet += 1;
        for _ in 0..count {
            let d = (self.rng.dir3() + Vec3::Y * 0.8).normalize();
            let s = self.rng.range(0.07, 0.18);
            let v = d * speed * self.rng.range(0.5, 1.2);
            let t = if self.rng.chance(0.5) { [0.34, 0.03, 0.03] } else { tint };
            self.debris(pos, v, m::GIB, Vec3::splat(s), t, 7.0, true);
        }
        self.quiet -= 1;
    }

    pub fn update(&mut self, dt: f32) {
        self.quiet += 1;
        let mut landed: [Vec3; 24] = [Vec3::ZERO; 24];
        let mut nl = 0;
        self.p.retain_mut(|p| {
            p.life -= dt;
            if p.life <= 0.0 {
                return false;
            }
            p.vel.y += p.gravity * dt;
            p.vel *= (1.0 - p.drag * dt).max(0.0);
            // Negative stretch marks tracers, whose `vel` is the streak vector rather than motion.
            if p.stretch >= 0.0 {
                p.pos += p.vel * dt;
            }
            p.size += p.grow * dt;
            if p.pos.y < 0.02 && p.gravity < 0.0 {
                if p.blood {
                    if nl < landed.len() {
                        landed[nl] = p.pos;
                        nl += 1;
                    }
                    return false;
                }
                p.pos.y = 0.02;
                p.vel.y *= -0.3;
                p.vel.x *= 0.6;
                p.vel.z *= 0.6;
            }
            true
        });
        for l in landed.iter().take(nl) {
            let s = self.rng.range(0.1, 0.3);
            self.blood_splat(*l, s);
        }

        let mut bleed_points: [Vec3; 12] = [Vec3::ZERO; 12];
        let mut nb = 0;
        self.debris.retain_mut(|d| {
            d.life -= dt;
            if d.life <= 0.0 {
                return false;
            }
            if d.rest {
                return true;
            }
            d.vel.y -= 14.0 * dt;
            d.pos += d.vel * dt;
            let ang = d.spin.length();
            if ang > 1e-3 {
                d.rot = (Quat::from_axis_angle(d.spin / ang, ang * dt) * d.rot).normalize();
            }
            let floor = d.scale.y * 0.3 + 0.01;
            if d.pos.y < floor {
                d.pos.y = floor;
                if d.bleeds && d.vel.y < -2.0 && nb < bleed_points.len() {
                    bleed_points[nb] = d.pos;
                    nb += 1;
                }
                d.vel.y *= -0.35;
                d.vel.x *= 0.6;
                d.vel.z *= 0.6;
                d.spin *= 0.6;
                if d.vel.length_squared() < 0.4 {
                    d.rest = true;
                }
            }
            true
        });
        for b in bleed_points.iter().take(nb) {
            let s = self.rng.range(0.15, 0.4);
            self.blood_splat(*b, s);
        }

        self.lights.retain_mut(|l| {
            l.life -= dt;
            l.life > 0.0
        });
        self.quiet -= 1;
    }

    pub fn emit_particles(&self, out: &mut Vec<Particle>, center: Vec3, range: f32) {
        let r2 = range * range;
        for p in &self.p {
            if (p.pos - center).length_squared() > r2 {
                continue;
            }
            let k = 1.0 - p.life / p.max_life;
            let mut c = [0.0f32; 4];
            for (i, v) in c.iter_mut().enumerate() {
                *v = p.c0[i] + (p.c1[i] - p.c0[i]) * k;
            }
            let (stretch, pos) = if p.stretch < 0.0 {
                (p.vel, p.pos)
            } else if p.stretch > 0.0 {
                (p.vel * p.stretch, p.pos)
            } else {
                (Vec3::ZERO, p.pos)
            };
            out.push(Particle { pos: pos.to_array(), size: p.size, stretch: stretch.to_array(), shape: p.shape, color: c });
        }
    }

    pub fn emit_lights(&self, out: &mut Vec<PointLight>) {
        for l in &self.lights {
            let k = (l.life / l.max).clamp(0.0, 1.0);
            out.push(PointLight { pos: l.pos.to_array(), radius: l.radius, color: (l.color * k).to_array(), _pad: 0.0 });
        }
    }

    pub fn emit_debris(&self, out: &mut [Vec<Instance>], center: Vec3, range: f32) {
        let r2 = range * range;
        for d in &self.debris {
            if (d.pos - center).length_squared() > r2 {
                continue;
            }
            let fade = (d.life * 2.0).clamp(0.05, 1.0);
            let mm = Mat4::from_scale_rotation_translation(d.scale * fade, d.rot, d.pos);
            out[d.mesh as usize].push(Instance::new(&mm, d.tint, d.glow));
        }
    }
}
