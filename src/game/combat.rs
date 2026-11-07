//! Damage, gunfire, projectiles, explosions and dying.
use super::data::*;
use super::fx::Ev;
use super::player::{rotate_y, DeathCause};
use super::World;
use crate::audio::Sfx;
use crate::ragdoll::*;
use crate::render::mesh as m;
use glam::{vec3, Vec2, Vec3};

#[derive(Clone, Copy)]
pub struct Explosion {
    pub pos: Vec3,
    pub radius: f32,
    pub damage: f32,
    pub from_enemy: bool,
    pub force: f32,
    /// Leaves players alone: the blast that clears space when someone redeploys.
    pub spares_players: bool,
    /// Barrels throw anyone anywhere in the blast, not just those close to its center.
    pub barrel: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ProjKind {
    Rocket,
    Grenade,
    Shell,
    Saw,
    Spit,
}

impl ProjKind {
    pub fn from_u8(v: u8) -> Option<ProjKind> {
        use ProjKind::*;
        [Rocket, Grenade, Shell, Saw, Spit].get(v as usize).copied()
    }
}

const BARREL_RADIUS: f32 = 0.45;
const BARREL_TOP: f32 = 1.3;

/// Where a ray passes through an upright cylinder standing on the ground: entry and exit distances.
pub fn ray_cylinder(from: Vec3, dir: Vec3, at: Vec3, radius: f32, top: f32) -> Option<(f32, f32)> {
    let d = Vec2::new(dir.x, dir.z);
    let o = Vec2::new(from.x - at.x, from.z - at.z);
    let a = d.length_squared();
    if a < 1e-6 {
        return None;
    }
    let b = o.dot(d);
    let disc = b * b - a * (o.length_squared() - radius * radius);
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let (mut t0, mut t1) = ((-b - root) / a, (-b + root) / a);
    if dir.y.abs() < 1e-6 {
        if from.y < 0.0 || from.y > top {
            return None;
        }
    } else {
        let (ya, yb) = (-from.y / dir.y, (top - from.y) / dir.y);
        t0 = t0.max(ya.min(yb));
        t1 = t1.min(ya.max(yb));
    }
    (t0 <= t1).then_some((t0, t1))
}

/// Where a unit ray strikes a zombie, and whether it found the head. Anything just behind the
/// muzzle still counts, since a gun can be pushed right through something at point blank.
pub fn ray_zombie(z: &super::zombie::Zombie, time: f32, from: Vec3, dir: Vec3, heads: bool) -> Option<(f32, bool)> {
    let s = z.scale;
    if heads && ray_cylinder(from, dir, z.pos, z.radius + 0.45 * s, z.body_top() + 0.4 * s).is_some() {
        let m = z.pose(time)[HEAD] - from;
        let along = m.dot(dir);
        let r = 0.3 * s;
        let miss2 = m.length_squared() - along * along;
        if miss2 < r * r {
            let t = along - (r * r - miss2).sqrt();
            if t > -0.3 {
                return Some((t, true));
            }
        }
    }
    let (t, far) = ray_cylinder(from, dir, z.pos, z.radius * 1.07, z.body_top())?;
    (far > -0.3).then_some((t, false))
}

pub struct Projectile {
    pub kind: ProjKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub damage: f32,
    pub radius: f32,
    pub ttl: f32,
    pub bounces: u32,
    pub spin: f32,
    pub hit: Vec<u32>,
    pub fx_t: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum HazKind {
    Fire,
    Acid,
}

pub struct Hazard {
    pub kind: HazKind,
    pub pos: Vec3,
    pub radius: f32,
    pub ttl: f32,
    pub tick: f32,
    pub dps: f32,
}

impl World {
    /// Applies damage and records how the blow landed, used when the zombie dies this frame.
    #[allow(clippy::too_many_arguments)]
    pub fn damage_zombie(&mut self, i: usize, dmg: f32, point: Vec3, dir: Vec3, knock: f32, gore: f32, crit: bool) {
        let z = &mut self.zombies[i];
        if z.hp <= 0.0 {
            // Already dying this frame; accumulate overkill so corpses react.
            z.last_force = z.last_force.max(knock);
            return;
        }
        let dmg = if crit { dmg * 2.0 } else { dmg };
        z.hp -= dmg;
        z.flash = 1.0;
        let mass = z.scale * z.scale;
        z.knock += Vec2::new(dir.x, dir.z) * knock / mass;
        z.stagger = (z.stagger + knock * 0.12 / mass).min(1.0);
        z.last_dir = dir;
        z.last_force = knock;
        z.last_point = point;
        z.last_gore = gore;
        z.last_crit = crit;
        let (ratio, kind, hp, pos) = (dmg / z.max_hp, z.kind, z.hp, z.pos);
        // Blood volume tracks how hard the hit was.
        let amount = (2.0 + dmg * 0.09).min(14.0) as u32;
        self.fx.blood(point, dir, amount, 2.5 + (dmg * 0.02).min(5.0));
        if self.rng.chance(0.4) {
            self.fx.blood_splat(pos + dir * self.rng.range(0.3, 1.5), self.rng.range(0.14, 0.35));
        }
        if self.rng.chance(0.4) {
            let id = match self.rng.int(3) {
                0 => Sfx::Fleshy1,
                1 => Sfx::Fleshy2,
                _ => Sfx::Fleshy3,
            };
            self.snd(id, point, 0.5);
        }
        // Non-lethal dismemberment: heavy hits tear off arms, or take out the legs.
        if hp > 0.0 && gore > 0.3 && kind != ZKind::Titan && kind != ZKind::Crawler && ratio > 0.12 {
            let roll = self.rng.f();
            let p = (ratio * gore).min(0.55);
            if roll < p {
                self.sever_alive(i);
            }
        }
    }

    fn sever_alive(&mut self, i: usize) {
        let missing = self.zombies[i].missing;
        let choice = self.rng.int(4);
        let (mask, is_leg) = match choice {
            0 => (MASK_L_ARM, false),
            1 => (MASK_R_ARM, false),
            2 => (MASK_L_LEG, true),
            _ => (MASK_R_LEG, true),
        };
        if missing & mask != 0 {
            return;
        }
        self.fx.muted(true);
        self.sever_visual(i, mask);
        self.fx.muted(false);
        let id = self.zombies[i].id;
        self.fx.record(|| Ev::Severed { id, mask });
        let z = &mut self.zombies[i];
        z.missing |= mask;
        if is_leg {
            z.crawling = true;
        }
    }

    /// The flying limb itself. Runs on servers and clients alike.
    pub fn sever_visual(&mut self, i: usize, mask: u16) {
        let t = self.time;
        let z = &self.zombies[i];
        let pose = z.pose(t);
        let (style, scale) = (z.style, z.scale);
        let vel = vec3(z.vel.x, 2.0, z.vel.y) + z.last_dir * 4.0;
        let is_leg = mask == MASK_L_LEG || mask == MASK_R_LEG;
        let (point, dir) = (z.last_point, z.last_dir);
        let life = self.rng.range(10.0, 18.0);
        let body = self.ragdolls.spawn(&pose, vel, scale, mask, style, life);
        self.ragdolls.impulse(body, point, dir * 8.0 + Vec3::Y * 3.0, 2.0);
        self.fx.blood(pose[if is_leg { PELVIS } else { CHEST }], Vec3::Y + vel * 0.1, 18, 5.0);
    }

    /// Where a ray meets the ground, if it is heading that way.
    fn ground_t(from: Vec3, dir: Vec3) -> Option<f32> {
        (dir.y < -1e-4 && from.y > 0.0).then(|| -from.y / dir.y)
    }

    /// What the cursor is really over: the first zombie along the camera's ray, with its head as a
    /// target of its own, then a barrel, then the ground. Guns fire at exactly this point.
    pub fn aim_point(&self, eye: Vec3, ray: Vec3) -> Vec3 {
        let mut best = f32::MAX;
        let mut point = Self::ground_t(eye, ray).map_or(eye + ray * 40.0, |t| eye + ray * t);
        for z in &self.zombies {
            if z.spawn_t > 0.5 {
                continue;
            }
            let Some((t, head)) = ray_zombie(z, self.time, eye, ray, true) else { continue };
            if t >= best {
                continue;
            }
            best = t;
            point = if head {
                z.pose(self.time)[HEAD]
            } else {
                // The body's center along this line of sight, so that shots don't just graze it.
                let (o, d) = (Vec2::new(z.pos.x - eye.x, z.pos.z - eye.z), Vec2::new(ray.x, ray.z));
                let mid = eye + ray * (o.dot(d) / d.length_squared().max(1e-5));
                let low = z.crawling || z.kind == ZKind::Crawler;
                let top = z.scale * if low { 0.5 } else { 1.5 };
                vec3(mid.x, mid.y.clamp(0.3 * z.scale, top), mid.z)
            };
        }
        for b in &self.barrels {
            if let Some((t, _)) = ray_cylinder(eye, ray, b.pos, BARREL_RADIUS, BARREL_TOP) {
                if t < best {
                    best = t;
                    point = vec3(b.pos.x, BARREL_TOP * 0.6, b.pos.z);
                }
            }
        }
        point
    }

    /// Hitscan shot: damages everything along the ray up to `pierce` extra targets.
    #[allow(clippy::too_many_arguments)]
    pub fn hitscan(&mut self, from: Vec3, dir: Vec3, range: f32, damage: f32, pierce: u32, knock: f32, gore: f32, tracer: [f32; 3], width: f32) -> Vec3 {
        let wall = self.map.raycast(from, dir, range);
        let blocker = match (wall, Self::ground_t(from, dir)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let limit = blocker.unwrap_or(range).min(range);
        // Level fire has no aim height to judge a head by, so sentries and touch controls score
        // critical hits by how squarely they strike. Anything aimed in 3D has to find the head.
        let level = dir.y.abs() < 0.01;
        let time = self.time;
        let flat = Vec2::new(dir.x, dir.z).normalize_or_zero();
        let mut hits = std::mem::take(&mut self.hit_scratch);
        hits.clear();
        for (i, z) in self.zombies.iter().enumerate() {
            if z.spawn_t > 0.5 {
                continue;
            }
            let Some((t, head)) = ray_zombie(z, time, from, dir, !level) else { continue };
            if t > limit {
                continue;
            }
            let crit = if level {
                let o = Vec2::new(z.pos.x - from.x, z.pos.z - from.z);
                (o.x * flat.y - o.y * flat.x).abs() < z.radius * 0.28
            } else {
                head
            };
            hits.push((t, i as u32, crit));
        }
        hits.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

        let mut end_t = limit;
        let mut dmg = damage;
        let mut left = pierce + 1;
        let mut hit_something = false;
        for k in 0..hits.len() {
            let (t, i, crit) = hits[k];
            let i = i as usize;
            let point = from + dir * t.max(0.0);
            if crit {
                self.stats.headshots += 1;
            }
            self.damage_zombie(i, dmg, point, dir, knock, gore, crit);
            hit_something = true;
            dmg *= 0.85;
            left -= 1;
            if left == 0 {
                end_t = t;
                break;
            }
        }
        self.hit_scratch = hits;

        // Barrels in the line of fire.
        let end = end_t;
        let mut hit_barrel = None;
        for (bi, b) in self.barrels.iter().enumerate() {
            if let Some((t, far)) = ray_cylinder(from, dir, b.pos, BARREL_RADIUS, BARREL_TOP) {
                if far >= 0.0 && t <= end {
                    hit_barrel = Some((bi, t.max(0.0)));
                    break;
                }
            }
        }
        if let Some((bi, t)) = hit_barrel {
            self.barrels[bi].hp -= damage.max(12.0);
            self.barrels[bi].burn = 2.5;
            let at = from + dir * t;
            self.fx.spark(at, Vec3::Y * 6.0, [10.0, 6.0, 1.0], 0.3);
        }

        let to = from + dir * end_t;
        self.fx.tracer(from, to, tracer, width);
        if !hit_something && blocker.is_some_and(|b| b <= range) {
            for _ in 0..3 {
                let v = (-dir + self.rng.dir3() * 0.7) * self.rng.range(2.0, 7.0);
                self.fx.spark(to, v, [8.0, 5.0, 1.5], 0.25);
            }
            self.fx.smoke(to, Vec3::Y * 0.5, 0.18, 0.5, 0.25, 0.25);
            if self.rng.chance(0.15) {
                self.snd(Sfx::Ricochet, to, 0.4);
            }
        }
        to
    }

    pub fn flame_stream(&mut self, from: Vec3, dir: Vec3, range: f32, damage: f32, ignite: f32) {
        for _ in 0..2 {
            let d = rotate_y(dir, self.rng.range(-0.2, 0.2));
            let sp = range * self.rng.range(1.6, 2.5);
            let life = self.rng.range(0.28, 0.46);
            self.fx.fire_puff(from, d * sp + Vec3::Y * self.rng.range(-0.3, 0.8), 0.17, life);
        }
        if self.rng.chance(0.25) {
            self.fx.light(from + dir * range * 0.45, range * 1.5, vec3(9.0, 4.0, 1.0), 0.09);
        }
        let center = Vec2::new(from.x, from.z) + Vec2::new(dir.x, dir.z) * range * 0.5;
        let list = self.query_zombies(center, range * 0.7);
        for &i in &list {
            let i = i as usize;
            let d = self.zombies[i].pos + Vec3::Y * from.y - from;
            let t = d.dot(dir);
            if t < 0.0 || t > range {
                continue;
            }
            let perp = (d - dir * t).length();
            if perp < 0.45 + t * 0.2 + self.zombies[i].radius {
                let p = self.zombies[i].pos + Vec3::Y;
                self.damage_zombie(i, damage, p, dir, 0.15, 0.0, false);
                let z = &mut self.zombies[i];
                z.burn = z.burn.max(ignite * 6.0);
                z.frost = 0.0;
            }
        }
        self.give_back_query(list);
        // Fire lights barrels.
        for b in &mut self.barrels {
            let d = b.pos - from;
            let t = d.x * dir.x + d.z * dir.z;
            if t > 0.0 && t < range && d.length_squared() - t * t < 1.5 {
                b.burn = 2.5;
            }
        }
    }

    pub fn frost_stream(&mut self, from: Vec3, dir: Vec3, range: f32, damage: f32) {
        for _ in 0..2 {
            let d = rotate_y(dir, self.rng.range(-0.18, 0.18));
            let life = self.rng.range(0.3, 0.5);
            self.fx.frost_puff(from, d * range * self.rng.range(1.5, 2.4), 0.15, life);
        }
        let center = Vec2::new(from.x, from.z) + Vec2::new(dir.x, dir.z) * range * 0.5;
        let list = self.query_zombies(center, range * 0.7);
        for &i in &list {
            let i = i as usize;
            let d = self.zombies[i].pos + Vec3::Y * from.y - from;
            let t = d.dot(dir);
            if t < 0.0 || t > range {
                continue;
            }
            let perp = (d - dir * t).length();
            if perp < 0.4 + t * 0.18 + self.zombies[i].radius {
                let p = self.zombies[i].pos + Vec3::Y;
                self.damage_zombie(i, damage, p, dir, 0.05, 0.1, false);
                let z = &mut self.zombies[i];
                let resist = if z.kind == ZKind::Titan { 0.2 } else { 1.0 };
                z.frost = (z.frost + 0.09 * resist).min(2.5);
                z.burn = 0.0;
                z.last_cold = true;
            }
        }
        self.give_back_query(list);
    }

    /// Chain lightning from `from` along `dir`, forking through nearby zombies.
    pub fn tesla_arc(&mut self, from: Vec3, dir: Vec3, range: f32, damage: f32, chains: u32) {
        let color = [3.0, 7.0, 16.0];
        // Find the first target near the aim line.
        let mut best: Option<(usize, f32)> = None;
        for (i, z) in self.zombies.iter().enumerate() {
            if z.spawn_t > 0.5 {
                continue;
            }
            let d = z.pos + Vec3::Y * from.y - from;
            let t = d.dot(dir);
            if t < 0.0 || t > range {
                continue;
            }
            let perp = (d - dir * t).length();
            let score = t + perp * 3.0;
            if perp < 3.0 && best.is_none_or(|b| score < b.1) {
                best = Some((i, score));
            }
        }
        let Some((first, _)) = best else {
            let end = from + rotate_y(dir, self.rng.range(-0.15, 0.15)) * range * 0.6;
            self.fx.lightning(from, end, color, 0.04);
            return;
        };
        self.chain_from(from, first, damage, chains, color);
    }

    pub fn chain_from(&mut self, from: Vec3, first: usize, damage: f32, chains: u32, color: [f32; 3]) {
        let mut visited: Vec<usize> = Vec::with_capacity(chains as usize + 1);
        let mut cur = first;
        let mut prev_pos = from;
        let mut dmg = damage;
        for _ in 0..=chains {
            visited.push(cur);
            let at = self.zombies[cur].pos + Vec3::Y * 1.1 * self.zombies[cur].scale;
            self.fx.lightning(prev_pos, at, color, 0.045);
            self.damage_zombie(cur, dmg, at, (at - prev_pos).normalize_or(Vec3::Z), 0.8, 0.15, false);
            let z = &mut self.zombies[cur];
            z.stun = z.stun.max(0.35);
            prev_pos = at;
            dmg *= 0.88;
            let p = self.zombies[cur].pos;
            let mut next: Option<(usize, f32)> = None;
            for (i, o) in self.zombies.iter().enumerate() {
                if visited.contains(&i) || o.hp <= 0.0 || o.spawn_t > 0.5 {
                    continue;
                }
                let d = o.pos.distance_squared(p);
                if d < 42.0 && next.is_none_or(|n| d < n.1) {
                    next = Some((i, d));
                }
            }
            match next {
                Some((i, _)) => cur = i,
                None => break,
            }
        }
    }

    pub fn beam(&mut self, from: Vec3, dir: Vec3, range: f32, damage: f32) {
        let wall = self.map.raycast(from, dir, range).unwrap_or(range);
        let wall = Self::ground_t(from, dir).map_or(wall, |g| wall.min(g));
        let mut hits: Vec<(f32, usize)> = Vec::new();
        for (i, z) in self.zombies.iter().enumerate() {
            if z.spawn_t > 0.5 {
                continue;
            }
            if let Some((t, _)) = ray_cylinder(from, dir, z.pos, z.radius + 0.14, z.body_top()) {
                if t <= wall {
                    hits.push((t.max(0.0), i));
                }
            }
        }
        hits.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let n = hits.len().min(5);
        let end = if hits.len() > 5 { hits[4].0 } else { wall };
        for &(t, i) in hits.iter().take(n) {
            let p = from + dir * t;
            self.damage_zombie(i, damage, p, dir, 0.3, 0.3, false);
            self.zombies[i].burn = self.zombies[i].burn.max(0.8);
            if self.rng.chance(0.3) {
                self.fx.fire_puff(p, Vec3::Y * 1.5, 0.15, 0.3);
            }
        }
        let to = from + dir * end;
        self.fx.tracer(from, to, [28.0, 8.0, 3.0], 0.08);
        self.fx.tracer(from, to, [16.0, 12.0, 10.0], 0.03);
        self.fx.light(to, 6.0, vec3(9.0, 3.0, 1.0), 0.05);
        if self.rng.chance(0.5) {
            self.fx.light(from + dir * end * 0.5, 9.0, vec3(5.0, 1.5, 0.6), 0.05);
        }
        // Scorch where the beam touches the floor, and sparks at the tip.
        self.fx.decal(to.x, to.z, 0.3, [0.02, 0.015, 0.012, 0.25], 1.0);
        let v = -dir * 4.0 + self.rng.dir3() * 4.0;
        self.fx.spark(to, v, [14.0, 6.0, 2.0], 0.25);
        for b in &mut self.barrels {
            let d = b.pos - from;
            let t = d.x * dir.x + d.z * dir.z;
            if t > 0.0 && t < end && d.length_squared() - t * t < 0.3 {
                b.hp -= damage;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_explosive(&mut self, from: Vec3, dir: Vec3, speed: f32, radius: f32, damage: f32, arc: bool, is_hand: bool) {
        let (kind, vel, ttl) = if arc {
            let v = dir * speed + Vec3::Y * if is_hand { speed * 0.55 } else { 4.0 };
            (ProjKind::Grenade, v, if is_hand { 1.9 } else { 2.4 })
        } else {
            (ProjKind::Rocket, dir * speed, 5.0)
        };
        self.projectiles.push(Projectile { kind, pos: from, vel, damage, radius, ttl, bounces: 0, spin: 0.0, hit: Vec::new(), fx_t: 0.0 });
    }

    pub fn spawn_shell(&mut self, from: Vec3, to: Vec3, radius: f32, damage: f32) {
        let flight = (from.distance(to) / 24.0).clamp(0.9, 2.6);
        let g = 16.0;
        let mut v = (to - from) / flight;
        v.y = (to.y - from.y) / flight + 0.5 * g * flight;
        self.projectiles.push(Projectile { kind: ProjKind::Shell, pos: from, vel: v, damage, radius, ttl: flight + 0.2, bounces: 0, spin: 0.0, hit: Vec::new(), fx_t: 0.0 });
    }

    pub fn spawn_saw(&mut self, from: Vec3, dir: Vec3, damage: f32, bounces: u32) {
        self.projectiles.push(Projectile { kind: ProjKind::Saw, pos: from, vel: dir * 26.0, damage, radius: 0.5, ttl: 6.0, bounces, spin: 0.0, hit: Vec::with_capacity(16), fx_t: 0.0 });
    }

    pub fn spawn_spit(&mut self, from: Vec3, to: Vec3) {
        let flight = (from.distance(to) / 13.0).clamp(0.5, 1.5);
        let g = 14.0;
        let mut v = (to - from) / flight;
        v.y = (to.y - from.y) / flight + 0.5 * g * flight;
        self.projectiles.push(Projectile { kind: ProjKind::Spit, pos: from, vel: v, damage: 12.0, radius: 1.7, ttl: flight + 1.0, bounces: 0, spin: 0.0, hit: Vec::new(), fx_t: 0.0 });
    }

    pub fn update_projectiles(&mut self, dt: f32) {
        let mut list = std::mem::take(&mut self.projectiles);
        list.retain_mut(|p| self.step_projectile(p, dt));
        // Anything spawned during the update (for example clusters) joins the list.
        list.append(&mut self.projectiles);
        self.projectiles = list;
    }

    fn step_projectile(&mut self, p: &mut Projectile, dt: f32) -> bool {
        p.ttl -= dt;
        p.spin += dt * 20.0;
        p.fx_t -= dt;
        match p.kind {
            ProjKind::Rocket => {
                let next = p.pos + p.vel * dt;
                if p.fx_t <= 0.0 {
                    p.fx_t = 0.018;
                    self.fx.fire_puff(p.pos, -p.vel * 0.05, 0.14, 0.15);
                    let s = self.rng.range(0.1, 0.2);
                    self.fx.smoke(p.pos, Vec3::Y * 0.3, s, 1.1, 0.2, 0.35);
                }
                self.fx.light(p.pos, 7.0, vec3(9.0, 4.0, 1.0), 0.03);
                let dir = p.vel.normalize_or(Vec3::Z);
                let wall = self.map.raycast(p.pos, dir, p.vel.length() * dt);
                let hit_z = self.zombies.iter().any(|z| {
                    z.spawn_t <= 0.5 && Vec2::new(z.pos.x - next.x, z.pos.z - next.z).length_squared() < (z.radius + 0.3).powi(2)
                });
                if wall.is_some() || hit_z || p.ttl <= 0.0 || next.y <= 0.05 || self.barrel_contact(next, 0.5) {
                    self.explosions.push(Explosion { pos: next, radius: p.radius, damage: p.damage, from_enemy: false, force: 22.0, spares_players: false, barrel: false });
                    return false;
                }
                p.pos = next;
                true
            }
            ProjKind::Grenade => {
                p.vel.y -= 16.0 * dt;
                let flat = Vec3::new(p.vel.x, 0.0, p.vel.z);
                if let Some(dirn) = flat.try_normalize() {
                    let wall = self.map.raycast(vec3(p.pos.x, p.pos.y.max(0.3), p.pos.z), dirn, flat.length() * dt + 0.2);
                    if wall.is_some() {
                        // Bounce off walls by reflecting on the dominant axis.
                        if dirn.x.abs() > dirn.z.abs() {
                            p.vel.x *= -0.5;
                        } else {
                            p.vel.z *= -0.5;
                        }
                    }
                }
                let mut np = p.pos + p.vel * dt;
                if np.y < 0.1 {
                    np.y = 0.1;
                    p.vel.y *= -0.45;
                    p.vel.x *= 0.65;
                    p.vel.z *= 0.65;
                }
                p.pos = np;
                if p.fx_t <= 0.0 {
                    p.fx_t = 0.05;
                    self.fx.spark(p.pos, Vec3::Y, [8.0, 3.0, 1.0], 0.15);
                }
                let contact = self.zombies.iter().any(|z| {
                    z.spawn_t <= 0.5 && z.pos.distance_squared(p.pos) < (z.radius + 0.25).powi(2) + 1.3
                });
                if p.ttl <= 0.0 || (contact && p.ttl < 1.7) {
                    self.explosions.push(Explosion { pos: p.pos, radius: p.radius, damage: p.damage, from_enemy: false, force: 20.0, spares_players: false, barrel: false });
                    return false;
                }
                true
            }
            ProjKind::Shell => {
                p.vel.y -= 16.0 * dt;
                p.pos += p.vel * dt;
                if p.fx_t <= 0.0 {
                    p.fx_t = 0.03;
                    self.fx.smoke(p.pos, Vec3::ZERO, 0.1, 0.5, 0.25, 0.25);
                }
                if p.pos.y <= 0.15 || p.ttl <= 0.0 {
                    p.pos.y = 0.15;
                    self.explosions.push(Explosion { pos: p.pos, radius: p.radius, damage: p.damage, from_enemy: false, force: 20.0, spares_players: false, barrel: false });
                    return false;
                }
                true
            }
            ProjKind::Saw => {
                let mut next = p.pos + p.vel * dt;
                let flat = p.vel.normalize_or(Vec3::Z);
                if self.map.raycast(p.pos, flat, p.vel.length() * dt + 0.25).is_some() {
                    if p.bounces == 0 {
                        for _ in 0..8 {
                            let v = self.rng.dir3() * 8.0;
                            self.fx.spark(p.pos, v, [10.0, 8.0, 3.0], 0.35);
                        }
                        return false;
                    }
                    p.bounces -= 1;
                    // Pick the axis whose motion actually collides.
                    let xhit = self.map.raycast(p.pos, vec3(flat.x.signum(), 0.0, 0.0), 0.6).is_some();
                    let zhit = self.map.raycast(p.pos, vec3(0.0, 0.0, flat.z.signum()), 0.6).is_some();
                    if xhit {
                        p.vel.x = -p.vel.x;
                    }
                    if zhit || !xhit {
                        p.vel.z = -p.vel.z;
                    }
                    p.hit.clear();
                    next = p.pos + p.vel * dt;
                    for _ in 0..5 {
                        let v = self.rng.dir3() * 7.0;
                        self.fx.spark(p.pos, v, [10.0, 8.0, 3.0], 0.25);
                    }
                    self.snd(Sfx::Saw, p.pos, 0.4);
                }
                p.pos = next;
                let dmg = p.damage;
                let dir = p.vel.normalize_or(Vec3::Z);
                let lim = p.pos;
                let count = self.zombies.len();
                for i in 0..count {
                    let (id, zp, r, spawn) = {
                        let z = &self.zombies[i];
                        (z.id, z.pos, z.radius, z.spawn_t)
                    };
                    if spawn > 0.5 || p.hit.contains(&id) {
                        continue;
                    }
                    let d = Vec2::new(zp.x - lim.x, zp.z - lim.z);
                    if d.length_squared() < (r + 0.4).powi(2) {
                        p.hit.push(id);
                        self.damage_zombie(i, dmg, zp + Vec3::Y * 1.1, dir, 0.8, 1.0, false);
                        // Saws keep flying but lose a little of their momentum.
                        p.vel *= 0.985;
                    }
                }
                p.ttl > 0.0 && lim.x.abs() < 80.0 && lim.z.abs() < 80.0
            }
            ProjKind::Spit => {
                p.vel.y -= 14.0 * dt;
                p.pos += p.vel * dt;
                if p.fx_t <= 0.0 {
                    p.fx_t = 0.03;
                    self.fx.glow(p.pos, Vec3::ZERO, 0.14, 0.0, 0.35, [0.5, 2.5, 0.1, 0.5], [0.05, 0.3, 0.0, 0.0]);
                }
                if p.pos.y <= 0.1 {
                    let at = vec3(p.pos.x, 0.05, p.pos.z);
                    self.hazards.push(Hazard { kind: HazKind::Acid, pos: at, radius: p.radius, ttl: 5.0, tick: 0.0, dps: p.damage });
                    self.fx.decal(at.x, at.z, p.radius * 0.8, [0.035, 0.11, 0.008, 0.6], 2.0);
                    self.fx.light(at, 3.5, vec3(0.5, 2.5, 0.1), 0.3);
                    self.snd(Sfx::Fleshy2, at, 0.5);
                    return false;
                }
                true
            }
        }
    }

    fn barrel_contact(&self, p: Vec3, r: f32) -> bool {
        self.barrels.iter().any(|b| b.pos.distance_squared(vec3(p.x, b.pos.y, p.z)) < (0.4 + r) * (0.4 + r))
    }

    pub fn update_hazards(&mut self, dt: f32) {
        let mut list = std::mem::take(&mut self.hazards);
        list.retain_mut(|h| {
            h.ttl -= dt;
            h.tick -= dt;
            let tick = h.tick <= 0.0;
            if tick {
                h.tick = 0.25;
            }
            match h.kind {
                HazKind::Fire => {
                    if self.rng.chance(dt * 40.0) {
                        let off = self.rng.dir2() * self.rng.range(0.0, h.radius);
                        let at = h.pos + vec3(off.x, 0.1, off.y);
                        let v = Vec3::Y * self.rng.range(1.0, 2.8);
                        let life = self.rng.range(0.3, 0.7);
                        self.fx.fire_puff(at, v, 0.2, life);
                        if self.rng.chance(0.2) {
                            self.fx.smoke(at + Vec3::Y, Vec3::Y * 1.5, 0.3, 1.4, 0.05, 0.3);
                        }
                    }
                    self.fx.light(h.pos + Vec3::Y, h.radius * 3.0, vec3(5.0, 2.2, 0.5), 0.03);
                    if tick {
                        for z in &mut self.zombies {
                            if z.pos.distance_squared(h.pos) < h.radius * h.radius {
                                z.burn = z.burn.max(2.5);
                                z.hp -= h.dps * 0.25;
                                z.flash = 0.6;
                            }
                        }
                    }
                }
                HazKind::Acid => {
                    if self.rng.chance(dt * 18.0) {
                        let off = self.rng.dir2() * self.rng.range(0.0, h.radius);
                        let at = h.pos + vec3(off.x, 0.1, off.y);
                        self.fx.glow(at, Vec3::Y * 0.9, 0.12, 0.1, 0.7, [0.3, 1.6, 0.1, 0.1], [0.0, 0.2, 0.0, 0.0]);
                    }
                    self.fx.light(h.pos + Vec3::Y * 0.3, h.radius * 2.0, vec3(0.3, 1.3, 0.1), 0.03);
                    if tick {
                        let dmg = h.dps * 0.25;
                        for pi in 0..self.players.len() {
                            let pl = &self.players[pi];
                            if pl.alive && pl.connected && pl.pos.distance_squared(h.pos) < h.radius * h.radius {
                                let at = h.pos;
                                self.hurt_player(pi, dmg, at, DeathCause::Regular);
                            }
                        }
                        let t = super::tile_of(Vec2::new(h.pos.x, h.pos.z));
                        if let Some(idx) = super::tile_index(t) {
                            let si = self.tiles[idx];
                            if si >= 0 {
                                let at = h.pos;
                                self.hurt_structure(si as usize, dmg * 1.5, at);
                            }
                        }
                    }
                }
            }
            h.ttl > 0.0
        });
        list.append(&mut self.hazards);
        self.hazards = list;
    }

    pub fn update_barrels(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.barrels.len() {
            if self.barrels[i].burn > 0.0 {
                self.barrels[i].burn -= dt;
                let p = self.barrels[i].pos + Vec3::Y * 1.05;
                if self.rng.chance(dt * 30.0) {
                    let life = self.rng.range(0.25, 0.5);
                    self.fx.fire_puff(p, Vec3::Y * 2.5, 0.14, life);
                }
                self.fx.light(p, 5.0, vec3(4.0, 1.8, 0.4), 0.03);
                if self.barrels[i].burn <= 0.0 {
                    self.barrels[i].hp = 0.0;
                }
            }
            if self.barrels[i].hp <= 0.0 {
                let b = self.barrels.swap_remove(i);
                let at = b.pos + Vec3::Y * 0.5;
                self.explosions.push(Explosion { pos: at, radius: 7.0, damage: 520.0, from_enemy: false, force: 26.0, spares_players: false, barrel: true });
                self.hazards.push(Hazard { kind: HazKind::Fire, pos: b.pos, radius: 2.4, ttl: 8.0, tick: 0.0, dps: 26.0 });
                for _ in 0..7 {
                    let v = self.rng.dir3() * 7.0 + Vec3::Y * 6.0;
                    self.fx.debris(at, v, m::PLANK, vec3(0.5, 0.05, 0.3), [0.6, 0.15, 0.1], 4.0, false);
                }
                continue;
            }
            i += 1;
        }
    }

    pub fn process_explosions(&mut self) {
        let mut guard = 0;
        while let Some(e) = self.explosions.pop() {
            guard += 1;
            if guard > 40 {
                self.explosions.clear();
                break;
            }
            self.detonate(e);
        }
    }

    fn detonate(&mut self, e: Explosion) {
        self.fx.explosion(e.pos, e.radius);
        self.snd(if e.radius > 5.0 { Sfx::ExplosionBig } else { Sfx::ExplosionSmall }, e.pos, 1.0);
        self.shake((e.radius * 0.09).min(0.65));
        self.blast(e.pos, e.radius * 1.6, e.force * 0.9);

        // Zombies.
        let r2 = e.radius * e.radius;
        for i in 0..self.zombies.len() {
            let (zp, spawn) = {
                let z = &self.zombies[i];
                (z.pos, z.spawn_t)
            };
            if spawn > 0.5 {
                continue;
            }
            let d = zp + Vec3::Y * 0.8 - e.pos;
            let l2 = d.length_squared();
            if l2 < r2 {
                let l = l2.sqrt();
                let falloff = (1.0 - l / e.radius).clamp(0.15, 1.0);
                let dir = (d / l.max(0.1) + Vec3::Y * 0.6).normalize();
                let dmg = e.damage * falloff;
                self.damage_zombie(i, dmg, zp + Vec3::Y, dir, e.force * falloff, 1.0, false);
                let z = &mut self.zombies[i];
                z.last_blast = true;
                z.last_force = z.last_force.max(e.force * falloff);
            }
        }
        // Barrels cook off in chains.
        for b in &mut self.barrels {
            if b.pos.distance_squared(e.pos) < r2 {
                b.hp -= e.damage * 0.4;
                b.burn = b.burn.max(0.25);
            }
        }
        // Enemy-caused blasts hurt your things. Your own only reach you lightly.
        if !e.spares_players {
            for pi in 0..self.players.len() {
                let pl = &self.players[pi];
                let pdist = pl.pos.distance(e.pos);
                if pl.alive && pl.connected && pdist < e.radius * 0.8 {
                    let k = 1.0 - pdist / (e.radius * 0.8);
                    let dmg = if e.from_enemy { e.damage * k * 0.55 } else { e.damage * k * 0.035 };
                    self.hurt_player(pi, dmg, e.pos, DeathCause::Explosion);
                    // Anyone close enough who lives is thrown off their feet, helpless until they get up.
                    if self.players[pi].alive && (k > 0.2 || e.barrel) {
                        self.knock_down(pi, e.pos, e.force * (0.4 + k * 0.6));
                    }
                } else if e.barrel && pl.alive && pl.connected && pdist < e.radius {
                    // Past the reach of the damage, but the shockwave still takes your feet.
                    self.knock_down(pi, e.pos, e.force * 0.4);
                }
            }
        }
        if e.from_enemy {
            let list: Vec<usize> = self.structures.iter().enumerate().filter(|(_, s)| s.pos.distance_squared(e.pos) < r2).map(|(i, _)| i).collect();
            for i in list {
                self.hurt_structure(i, e.damage * 0.8, e.pos);
            }
        }
    }

    /// Removes dead zombies and turns them into ragdolls, gibs and rewards.
    pub fn reap_zombies(&mut self) {
        let mut i = 0;
        while i < self.zombies.len() {
            if self.zombies[i].hp > 0.0 {
                i += 1;
                continue;
            }
            let z = self.zombies.swap_remove(i);
            self.on_zombie_death(z);
        }
    }

    fn on_zombie_death(&mut self, z: super::zombie::Zombie) {
        let d = zdef(z.kind);
        self.stats.kills += 1;
        self.combo += 1;
        self.combo_timer = 3.0;
        self.stats.best_combo = self.stats.best_combo.max(self.combo);
        if self.combo > 0 && self.combo % 25 == 0 {
            let c = self.combo;
            self.announce(&format!("{c} KILL STREAK!"), [1.0, 0.6, 0.15], 1.6, false);
        }
        // Scrap and loot. Streaks pay extra.
        let mult = 1.0 + (self.combo as f32 / 120.0).min(0.5);
        let value = (d.scrap as f32 * mult) as u32;
        self.drop_pickup(0, z.pos, value);
        if self.rng.chance(0.045) {
            self.drop_pickup(1, z.pos, 1);
        } else if self.rng.chance(0.018) {
            self.drop_pickup(2, z.pos, 30);
        }

        if z.kind == ZKind::Bloater {
            self.explosions.push(Explosion { pos: z.pos + Vec3::Y * 0.8, radius: 4.8, damage: 150.0, from_enemy: true, force: 16.0, spares_players: false, barrel: false });
            self.hazards.push(Hazard { kind: HazKind::Acid, pos: z.pos, radius: 2.5, ttl: 6.0, tick: 0.0, dps: 10.0 });
        }
        // Clients derive the ragdoll and gore from this one event.
        let flags = u8::from(z.last_crit) | u8::from(z.last_blast) << 1 | u8::from(z.last_cold) << 2 | u8::from(z.burn > 0.0) << 3;
        self.fx.record(|| Ev::ZombieDied { id: z.id, dir: z.last_dir, point: z.last_point, force: z.last_force, gore: z.last_gore, flags });
        self.fx.muted(true);
        self.zombie_death_visual(&z);
        self.fx.muted(false);

        if z.kind == ZKind::Titan {
            self.shake(0.9);
            for k in 0..3 {
                let off = self.rng.dir2() * k as f32;
                self.explosions.push(Explosion { pos: z.pos + vec3(off.x, 1.0, off.y), radius: 6.0, damage: 300.0, from_enemy: false, force: 20.0, spares_players: false, barrel: false });
            }
            self.announce("TITAN DOWN", [1.0, 0.85, 0.2], 3.0, true);
            self.snd(Sfx::Boss, z.pos, 1.0);
            for _ in 0..12 {
                self.drop_pickup(0, z.pos, 30);
            }
            self.drop_pickup(2, z.pos, 60);
            self.drop_pickup(1, z.pos, 1);
        }
    }

    /// Body, gore and sound of a death. Purely cosmetic: it runs on clients too.
    pub fn zombie_death_visual(&mut self, z: &super::zombie::Zombie) {
        let t = self.time;
        let pose = z.pose(t);
        let chest = pose[CHEST];

        if z.kind == ZKind::Bloater {
            self.fx.decal(z.pos.x, z.pos.z, 2.1, [0.035, 0.11, 0.008, 0.6], 2.0);
            self.fx.gibs(chest, 16, 11.0, [0.4, 0.5, 0.2]);
            self.snd(Sfx::Pop, z.pos, 1.0);
            return;
        }
        // Deep-frozen targets shatter into ice instead of falling down.
        if z.frost > 1.4 && z.last_cold && z.kind != ZKind::Titan {
            for _ in 0..14 {
                let v = self.rng.dir3() * self.rng.range(3.0, 9.0) + Vec3::Y * 3.0;
                let s = self.rng.range(0.08, 0.2);
                self.fx.debris(chest, v, m::SHARD, Vec3::splat(s), [0.65, 0.88, 1.0], 2.5, false);
            }
            self.fx.frost_puff(chest, Vec3::Y, 0.7, 0.5);
            self.fx.light(chest, 5.0, vec3(1.5, 3.0, 5.0), 0.15);
            self.snd(Sfx::Casing, z.pos, 1.0);
            return;
        }

        let force = z.last_force;
        // Total liquefaction: a very close blast leaves only chunks.
        if z.last_blast && force > 15.0 && z.kind != ZKind::Titan && self.rng.chance(0.55) {
            self.fx.gibs(chest, 12, 10.0, style_tint(z.style));
            self.fx.blood(chest, Vec3::Y, 30, 8.0);
            self.fx.blood_splat(z.pos, self.rng.range(0.9, 1.5));
            self.snd(Sfx::Death2, z.pos, 0.7);
            return;
        }

        // Standard death: the animated pose hands over to the ragdoll solver.
        let base_vel = vec3(z.vel.x + z.knock.x, 0.0, z.vel.y + z.knock.y) * 0.8;
        let enabled = ALL_JOINTS & !z.missing;
        let life = if z.kind == ZKind::Titan { 25.0 } else { self.rng.range(14.0, 24.0) };
        let body = self.ragdolls.spawn(&pose, base_vel, z.scale, enabled, z.style, life);
        let dir = z.last_dir;
        let impulse = dir * (force * 1.4 + 3.0) + Vec3::Y * (1.5 + force * 0.25);
        let point = if z.last_point == Vec3::ZERO { chest } else { z.last_point };
        self.ragdolls.impulse(body, point, impulse, 1.6);
        if z.burn > 0.0 {
            self.ragdolls.bodies[body].char_amount = 1.0;
        }

        let gore = z.last_gore;
        let crit = z.last_crit;
        let b = &mut self.ragdolls.bodies[body];
        // Stumps on the corpse match any limbs lost in life.
        if z.missing & MASK_L_ARM != 0 {
            b.severed |= 1 << Limb::LArm as u8;
        }
        if z.missing & MASK_R_ARM != 0 {
            b.severed |= 1 << Limb::RArm as u8;
        }
        if z.missing & MASK_L_LEG != 0 {
            b.severed |= 1 << Limb::LLeg as u8;
        }
        if z.missing & MASK_R_LEG != 0 {
            b.severed |= 1 << Limb::RLeg as u8;
        }
        let head_pop = z.kind != ZKind::Titan && gore > 0.05 && (crit || self.rng.chance(gore * 0.45));
        if head_pop {
            let b = &mut self.ragdolls.bodies[body];
            b.sever(Limb::Head);
            b.vel[HEAD] += dir * self.rng.range(3.0, 7.0) + Vec3::Y * self.rng.range(3.0, 7.0);
            let at = b.pos[NECK];
            self.fx.blood(at, Vec3::Y + dir * 0.3, 22, 6.5);
        }
        if gore >= 0.7 {
            let limbs = [Limb::LArm, Limb::RArm, Limb::LLeg, Limb::RLeg];
            let count = 1 + self.rng.int(if z.last_blast { 4 } else { 2 });
            for _ in 0..count {
                let l = limbs[self.rng.int(4)];
                let bd = &mut self.ragdolls.bodies[body];
                if bd.sever(l) {
                    let joints: [usize; 3] = match l {
                        Limb::LArm => [L_SHO, L_ELB, L_HAND],
                        Limb::RArm => [R_SHO, R_ELB, R_HAND],
                        Limb::LLeg => [L_HIP, L_KNEE, L_FOOT],
                        _ => [R_HIP, R_KNEE, R_FOOT],
                    };
                    let fly = (self.rng.dir3() + Vec3::Y * 0.8 + dir).normalize() * self.rng.range(3.0, 9.0);
                    let bd = &mut self.ragdolls.bodies[body];
                    for j in joints {
                        bd.vel[j] += fly;
                    }
                }
            }
            self.fx.gibs(chest, 3, 7.0, style_tint(z.style));
        }
        self.fx.blood(chest, dir + Vec3::Y * 0.5, 16, 5.0);
        let splat = self.rng.range(0.45, 0.95) * z.scale;
        self.fx.blood_splat(z.pos + dir * 0.6, splat);

        if z.kind == ZKind::Titan {
            self.fx.gibs(chest, 30, 14.0, style_tint(z.style));
            self.fx.blood_splat(z.pos, 4.0);
        } else if self.rng.chance(0.35) {
            let id = if self.rng.chance(0.5) { Sfx::Death1 } else { Sfx::Death2 };
            self.snd(id, z.pos, 0.55);
        }
    }
}

pub fn style_tint(style: u32) -> [f32; 3] {
    super::zombie::style_colors(style).0
}
