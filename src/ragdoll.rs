//! A self-contained ragdoll engine built on position-based dynamics.
//!
//! Every humanoid in the game (living or dead) is described by the same 16-joint
//! skeleton. Living characters have their joints posed procedurally; on death the
//! current pose and its velocity are handed to this solver, so there is no visible
//! pop between animation and physics.
//!
//! The solver supports: rigid-ish torso cross bracing, joint-limit style min/max
//! distance constraints, dismemberment (constraints can be broken per limb group),
//! partial bodies (a severed arm is a ragdoll with only arm joints enabled),
//! static collision, particle-vs-particle pile-ups and sleeping.
use glam::{vec3, Vec3};

pub const HEAD: usize = 0;
pub const NECK: usize = 1;
pub const CHEST: usize = 2;
pub const PELVIS: usize = 3;
pub const L_SHO: usize = 4;
pub const L_ELB: usize = 5;
pub const L_HAND: usize = 6;
pub const R_SHO: usize = 7;
pub const R_ELB: usize = 8;
pub const R_HAND: usize = 9;
pub const L_HIP: usize = 10;
pub const L_KNEE: usize = 11;
pub const L_FOOT: usize = 12;
pub const R_HIP: usize = 13;
pub const R_KNEE: usize = 14;
pub const R_FOOT: usize = 15;
pub const JOINTS: usize = 16;
pub type Pose = [Vec3; JOINTS];

pub const ALL_JOINTS: u16 = 0xFFFF;
pub const MASK_L_ARM: u16 = (1 << L_SHO) | (1 << L_ELB) | (1 << L_HAND);
pub const MASK_R_ARM: u16 = (1 << R_SHO) | (1 << R_ELB) | (1 << R_HAND);
pub const MASK_L_LEG: u16 = (1 << L_HIP) | (1 << L_KNEE) | (1 << L_FOOT);
pub const MASK_R_LEG: u16 = (1 << R_HIP) | (1 << R_KNEE) | (1 << R_FOOT);

/// Which detachable group a joint belongs to (0 = core).
pub const fn group_of(j: usize) -> u8 {
    match j {
        HEAD => 1,
        L_SHO | L_ELB | L_HAND => 2,
        R_SHO | R_ELB | R_HAND => 3,
        L_HIP | L_KNEE | L_FOOT => 4,
        R_HIP | R_KNEE | R_FOOT => 5,
        _ => 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Limb {
    Head = 1,
    LArm = 2,
    RArm = 3,
    LLeg = 4,
    RLeg = 5,
}

/// Standing rest pose, 1.75 units tall, facing +Z, right is +X.
pub fn rest_pose(scale: f32) -> Pose {
    let mut p = [Vec3::ZERO; JOINTS];
    p[HEAD] = vec3(0.0, 1.66, 0.0);
    p[NECK] = vec3(0.0, 1.50, 0.0);
    p[CHEST] = vec3(0.0, 1.27, 0.0);
    p[PELVIS] = vec3(0.0, 0.97, 0.0);
    p[L_SHO] = vec3(-0.21, 1.44, 0.0);
    p[L_ELB] = vec3(-0.24, 1.16, 0.0);
    p[L_HAND] = vec3(-0.25, 0.90, 0.0);
    p[R_SHO] = vec3(0.21, 1.44, 0.0);
    p[R_ELB] = vec3(0.24, 1.16, 0.0);
    p[R_HAND] = vec3(0.25, 0.90, 0.0);
    p[L_HIP] = vec3(-0.11, 0.93, 0.0);
    p[L_KNEE] = vec3(-0.12, 0.50, 0.0);
    p[L_FOOT] = vec3(-0.12, 0.07, 0.0);
    p[R_HIP] = vec3(0.11, 0.93, 0.0);
    p[R_KNEE] = vec3(0.12, 0.50, 0.0);
    p[R_FOOT] = vec3(0.12, 0.07, 0.0);
    for v in &mut p {
        *v *= scale;
    }
    p
}

/// Rigid constraints (rest length from the rest pose).
const LINKS: &[(usize, usize)] = &[
    (HEAD, NECK),
    (NECK, CHEST),
    (CHEST, PELVIS),
    (NECK, PELVIS),
    // arms
    (L_SHO, NECK),
    (L_SHO, CHEST),
    (L_SHO, PELVIS),
    (L_SHO, L_ELB),
    (L_ELB, L_HAND),
    (R_SHO, NECK),
    (R_SHO, CHEST),
    (R_SHO, PELVIS),
    (R_SHO, R_ELB),
    (R_ELB, R_HAND),
    // legs
    (L_HIP, PELVIS),
    (L_HIP, CHEST),
    (L_HIP, L_KNEE),
    (L_KNEE, L_FOOT),
    (R_HIP, PELVIS),
    (R_HIP, CHEST),
    (R_HIP, R_KNEE),
    (R_KNEE, R_FOOT),
    // torso bracing keeps the rib cage from collapsing flat
    (L_SHO, R_SHO),
    (L_HIP, R_HIP),
    (L_SHO, R_HIP),
    (R_SHO, L_HIP),
];

/// Soft joint limits, expressed as allowed distance as a fraction of the rest distance.
const LIMITS: &[(usize, usize, f32, f32)] = &[
    (L_SHO, L_HAND, 0.45, 1.02),
    (R_SHO, R_HAND, 0.45, 1.02),
    (L_HIP, L_FOOT, 0.55, 1.02),
    (R_HIP, R_FOOT, 0.55, 1.02),
    (HEAD, CHEST, 0.80, 1.05),
    (HEAD, L_SHO, 0.70, 1.25),
    (HEAD, R_SHO, 0.70, 1.25),
    (L_KNEE, PELVIS, 0.60, 1.10),
    (R_KNEE, PELVIS, 0.60, 1.10),
    (L_FOOT, R_FOOT, 0.25, 8.0),
    (L_KNEE, R_KNEE, 0.35, 8.0),
];

const INV_MASS: [f32; JOINTS] = [
    1.6, 1.2, 0.55, 0.50, // head, neck, chest, pelvis
    1.1, 1.6, 2.4, 1.1, 1.6, 2.4, // arms
    0.9, 1.2, 1.8, 0.9, 1.2, 1.8, // legs
];

const JOINT_RADIUS: [f32; JOINTS] = [
    0.12, 0.07, 0.15, 0.14, 0.07, 0.05, 0.05, 0.07, 0.05, 0.05, 0.08, 0.06, 0.06, 0.08, 0.06, 0.06,
];

const SUBSTEPS: usize = 2;
const ITERATIONS: usize = 4;
const GRAVITY: f32 = -14.0;
const SLEEP_SPEED2: f32 = 0.09;
const SLEEP_TIME: f32 = 0.5;
const MAX_SPEED: f32 = 45.0;

/// Static geometry the ragdolls collide against. Implementations return the position
/// corrected to lie outside any solid, or `None` when `p` is already free.
pub trait Collider {
    fn resolve(&self, p: Vec3, radius: f32) -> Option<Vec3>;
}

/// A collider with nothing but the infinite ground plane.
#[cfg(test)]
pub struct NoCollider;
#[cfg(test)]
impl Collider for NoCollider {
    fn resolve(&self, _p: Vec3, _r: f32) -> Option<Vec3> {
        None
    }
}

#[derive(Clone)]
pub struct Ragdoll {
    pub pos: Pose,
    pub prev: Pose,
    pub vel: Pose,
    /// Bitmask of joints that exist in this body at all.
    pub enabled: u16,
    /// Bitmask of limb groups (bit = group id) detached from the core.
    pub severed: u8,
    pub scale: f32,
    pub sleeping: bool,
    still: f32,
    pub age: f32,
    /// Lifetime in seconds; once expired the body is removed.
    pub life: f32,
    /// Opaque index the game uses to look up colours and meshes.
    pub style: u32,
    /// Remaining bleeding time per stump (joint index, seconds).
    pub bleed: [(u8, f32); 3],
    /// Last known torso right-axis, kept for rendering once shoulders detach.
    pub right_hint: Vec3,
    /// Burn amount for rendering, 0..1.
    pub char_amount: f32,
    /// The player this body currently is, or -1.
    pub owner: i16,
}

impl Ragdoll {
    pub fn is_on(&self, j: usize) -> bool {
        self.enabled & (1 << j) != 0
    }
    fn link_active(&self, a: usize, b: usize) -> bool {
        if !self.is_on(a) || !self.is_on(b) {
            return false;
        }
        let (ga, gb) = (group_of(a), group_of(b));
        if ga == gb {
            return true;
        }
        // Links across groups break as soon as either side is severed.
        let s = self.severed as u32;
        s & (1 << ga as u32) == 0 && s & (1 << gb as u32) == 0
    }
    pub fn center(&self) -> Vec3 {
        let (mut c, mut n) = (Vec3::ZERO, 0.0);
        for j in 0..JOINTS {
            if self.is_on(j) {
                c += self.pos[j];
                n += 1.0;
            }
        }
        if n > 0.0 {
            c / n
        } else {
            Vec3::ZERO
        }
    }
    /// Detach a limb. The joints keep existing and fly off as a free chain.
    pub fn sever(&mut self, limb: Limb) -> bool {
        let bit = 1u8 << limb as u8;
        if self.severed & bit != 0 {
            return false;
        }
        self.severed |= bit;
        self.sleeping = false;
        self.still = 0.0;
        let stump = match limb {
            Limb::Head => NECK,
            Limb::LArm | Limb::RArm => CHEST,
            Limb::LLeg | Limb::RLeg => PELVIS,
        } as u8;
        for slot in &mut self.bleed {
            if slot.1 <= 0.0 {
                *slot = (stump, 2.8);
                break;
            }
        }
        true
    }
}

pub struct RagdollWorld {
    pub bodies: Vec<Ragdoll>,
    pub max_bodies: usize,
    rest_links: Vec<f32>,
    rest_limits: Vec<(f32, f32)>,
    hash_heads: Vec<i32>,
    hash_next: Vec<i32>,
    hash_items: Vec<(u32, u8)>,
    half_extent: f32,
    /// Accumulated count of physics contacts, handy for profiling output.
    pub stat_active: usize,
}

const HASH_CELL: f32 = 0.4;

impl RagdollWorld {
    pub fn new(half_extent: f32, max_bodies: usize) -> Self {
        let rest = rest_pose(1.0);
        let dim = (2.0 * half_extent / HASH_CELL).ceil() as usize + 1;
        RagdollWorld {
            bodies: Vec::with_capacity(max_bodies),
            max_bodies,
            rest_links: LINKS.iter().map(|&(a, b)| rest[a].distance(rest[b])).collect(),
            rest_limits: LIMITS
                .iter()
                .map(|&(a, b, lo, hi)| {
                    let d = rest[a].distance(rest[b]);
                    (d * lo, d * hi)
                })
                .collect(),
            hash_heads: vec![-1; dim * dim],
            hash_next: Vec::new(),
            hash_items: Vec::new(),
            half_extent,
            stat_active: 0,
        }
    }

    /// Spawn a body from a pose and initial velocity.
    pub fn spawn(&mut self, pose: &Pose, vel: Vec3, scale: f32, enabled: u16, style: u32, life: f32) -> usize {
        if self.bodies.len() >= self.max_bodies {
            // Drop the oldest sleeping body, or just the oldest.
            let idx = self.bodies.iter().position(|b| b.sleeping).unwrap_or(0);
            self.bodies.swap_remove(idx);
        }
        let mut pos = *pose;
        for p in &mut pos {
            debug_assert!(p.is_finite());
            p.y = p.y.max(0.02);
        }
        self.bodies.push(Ragdoll {
            pos,
            prev: pos,
            vel: [vel; JOINTS],
            enabled,
            severed: 0,
            scale,
            sleeping: false,
            still: 0.0,
            age: 0.0,
            life,
            style,
            bleed: [(0, 0.0); 3],
            right_hint: (pos[R_HIP] - pos[L_HIP]).try_normalize().unwrap_or(Vec3::X),
            char_amount: 0.0,
            owner: -1,
        });
        self.bodies.len() - 1
    }

    /// Point impulse with a radial falloff around `point`.
    pub fn impulse(&mut self, body: usize, point: Vec3, dv: Vec3, radius: f32) {
        let b = &mut self.bodies[body];
        b.sleeping = false;
        b.still = 0.0;
        for j in 0..JOINTS {
            let d = b.pos[j].distance(point);
            let w = (1.0 - d / radius).clamp(0.08, 1.0);
            b.vel[j] += dv * w * INV_MASS[j];
        }
    }

    /// Radial explosion: throws every body in range, including the sleeping ones.
    pub fn blast(&mut self, center: Vec3, radius: f32, force: f32) {
        let r2 = radius * radius;
        for b in &mut self.bodies {
            if b.center().distance_squared(center) > r2 * 1.5 {
                continue;
            }
            b.sleeping = false;
            b.still = 0.0;
            for j in 0..JOINTS {
                let d = b.pos[j] - center;
                let l = d.length().max(0.2);
                if l < radius {
                    let falloff = 1.0 - l / radius;
                    let dir = (d / l + Vec3::Y * 0.65).normalize();
                    b.vel[j] += dir * force * falloff * INV_MASS[j];
                }
            }
        }
    }

    pub fn step(&mut self, dt: f32, collider: &dyn Collider) {
        if dt <= 0.0 {
            return;
        }
        // Age / expire.
        let mut i = 0;
        while i < self.bodies.len() {
            self.bodies[i].age += dt;
            if self.bodies[i].age > self.bodies[i].life {
                self.bodies.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let h = dt / SUBSTEPS as f32;
        let mut active = 0;
        for b in &mut self.bodies {
            for s in &mut b.bleed {
                s.1 -= dt;
            }
            if b.sleeping {
                continue;
            }
            active += 1;
            for _ in 0..SUBSTEPS {
                Self::substep(b, h, collider, &self.rest_links, &self.rest_limits);
            }
            let rv = (b.pos[R_HIP] - b.pos[L_HIP]) + (b.pos[R_SHO] - b.pos[L_SHO]);
            if let Some(r) = rv.try_normalize() {
                if b.severed & 0b111100 == 0 {
                    b.right_hint = r;
                }
            }
            // Sleep detection.
            let mut max_v2: f32 = 0.0;
            for j in 0..JOINTS {
                if b.is_on(j) {
                    max_v2 = max_v2.max(b.vel[j].length_squared());
                }
            }
            if max_v2 < SLEEP_SPEED2 {
                b.still += dt;
                if b.still > SLEEP_TIME {
                    b.sleeping = true;
                    b.vel = [Vec3::ZERO; JOINTS];
                }
            } else {
                b.still = 0.0;
            }
        }
        self.stat_active = active;
        if active > 0 {
            self.resolve_contacts();
        }
    }

    fn substep(b: &mut Ragdoll, h: f32, collider: &dyn Collider, links: &[f32], limits: &[(f32, f32)]) {
        let damp = (1.0 - 0.35 * h).max(0.0);
        for j in 0..JOINTS {
            if !b.is_on(j) {
                continue;
            }
            let mut v = b.vel[j];
            v.y += GRAVITY * h;
            v *= damp;
            let sp = v.length();
            if sp > MAX_SPEED {
                v *= MAX_SPEED / sp;
            }
            b.vel[j] = v;
            b.prev[j] = b.pos[j];
            b.pos[j] += v * h;
        }
        let scale = b.scale;
        for it in 0..ITERATIONS {
            for (k, &(a, bb)) in LINKS.iter().enumerate() {
                if !b.link_active(a, bb) {
                    continue;
                }
                solve_distance(&mut b.pos, a, bb, links[k] * scale, links[k] * scale, 1.0);
            }
            for (k, &(a, bb, _, _)) in LIMITS.iter().enumerate() {
                if !b.link_active(a, bb) {
                    continue;
                }
                let (lo, hi) = limits[k];
                solve_distance(&mut b.pos, a, bb, lo * scale, hi * scale, 0.7);
            }
            for j in 0..JOINTS {
                if !b.is_on(j) {
                    continue;
                }
                let r = JOINT_RADIUS[j] * scale;
                if b.pos[j].y < r {
                    b.pos[j].y = r;
                }
                if it == ITERATIONS - 1 {
                    if let Some(p) = collider.resolve(b.pos[j], r) {
                        b.pos[j] = p;
                    }
                }
            }
        }
        let inv_h = 1.0 / h;
        for j in 0..JOINTS {
            if !b.is_on(j) {
                continue;
            }
            let mut v = (b.pos[j] - b.prev[j]) * inv_h;
            if b.pos[j].y <= JOINT_RADIUS[j] * scale + 0.004 {
                let f = (1.0 - 9.0 * h).max(0.0);
                v.x *= f;
                v.z *= f;
                if v.y < 0.0 {
                    v.y = 0.0;
                }
            }
            b.vel[j] = v;
        }
    }

    /// One pass of particle-vs-particle separation so bodies pile instead of overlapping.
    fn resolve_contacts(&mut self) {
        let dim = (2.0 * self.half_extent / HASH_CELL).ceil() as usize + 1;
        self.hash_heads.iter_mut().for_each(|h| *h = -1);
        self.hash_items.clear();
        self.hash_next.clear();
        let he = self.half_extent;
        let cell = |p: Vec3| -> (usize, usize) {
            let cx = ((p.x + he) / HASH_CELL).clamp(0.0, (dim - 1) as f32) as usize;
            let cz = ((p.z + he) / HASH_CELL).clamp(0.0, (dim - 1) as f32) as usize;
            (cx, cz)
        };
        // Only torso/pelvis/head joints collide: cheap and enough for convincing piles.
        const CONTACT: [usize; 6] = [HEAD, CHEST, PELVIS, L_KNEE, R_KNEE, L_HAND];
        for (bi, b) in self.bodies.iter().enumerate() {
            for &j in &CONTACT {
                if !b.is_on(j) {
                    continue;
                }
                let (cx, cz) = cell(b.pos[j]);
                let idx = cz * dim + cx;
                self.hash_next.push(self.hash_heads[idx]);
                self.hash_heads[idx] = self.hash_items.len() as i32;
                self.hash_items.push((bi as u32, j as u8));
            }
        }
        for item in 0..self.hash_items.len() {
            let (bi, j) = self.hash_items[item];
            if self.bodies[bi as usize].sleeping {
                continue;
            }
            let p = self.bodies[bi as usize].pos[j as usize];
            let (cx, cz) = cell(p);
            for nz in cz.saturating_sub(1)..=(cz + 1).min(dim - 1) {
                for nx in cx.saturating_sub(1)..=(cx + 1).min(dim - 1) {
                    let mut it = self.hash_heads[nz * dim + nx];
                    while it >= 0 {
                        let (bo, jo) = self.hash_items[it as usize];
                        it = self.hash_next[it as usize];
                        if bo == bi {
                            continue;
                        }
                        let q = self.bodies[bo as usize].pos[jo as usize];
                        let d = q - p;
                        let min_d = JOINT_RADIUS[j as usize] + JOINT_RADIUS[jo as usize];
                        let l2 = d.length_squared();
                        if l2 >= min_d * min_d || l2 < 1e-8 {
                            continue;
                        }
                        let l = l2.sqrt();
                        let push = d / l * (min_d - l);
                        let other_static = self.bodies[bo as usize].sleeping;
                        if other_static {
                            self.bodies[bi as usize].pos[j as usize] -= push;
                        } else {
                            self.bodies[bi as usize].pos[j as usize] -= push * 0.5;
                            self.bodies[bo as usize].pos[jo as usize] += push * 0.5;
                        }
                    }
                }
            }
        }
    }
}

#[inline]
fn solve_distance(pos: &mut Pose, a: usize, b: usize, lo: f32, hi: f32, stiffness: f32) {
    let d = pos[b] - pos[a];
    let l2 = d.length_squared();
    if l2 < 1e-9 {
        return;
    }
    let l = l2.sqrt();
    let target = if l < lo {
        lo
    } else if l > hi {
        hi
    } else {
        return;
    };
    let (wa, wb) = (INV_MASS[a], INV_MASS[b]);
    let c = (l - target) / (l * (wa + wb)) * stiffness;
    pos[a] += d * (c * wa);
    pos[b] -= d * (c * wb);
}

/// Solve a two-bone chain: returns the elbow/knee given shoulder/hip `a`, end effector `c`.
pub fn two_bone(a: Vec3, c: Vec3, l1: f32, l2: f32, pole: Vec3) -> Vec3 {
    let ac = c - a;
    let d = ac.length().clamp(1e-4, l1 + l2 - 1e-3);
    let dir = ac.try_normalize().unwrap_or(Vec3::NEG_Y);
    let x = (l1 * l1 - l2 * l2 + d * d) / (2.0 * d);
    let h = (l1 * l1 - x * x).max(0.0).sqrt();
    let side = (pole - dir * pole.dot(dir)).try_normalize().unwrap_or(Vec3::Z);
    a + dir * x + side * h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite(b: &Ragdoll) -> bool {
        b.pos.iter().chain(b.vel.iter()).all(|v| v.is_finite())
    }

    #[test]
    fn falls_settles_and_sleeps_above_ground() {
        let mut w = RagdollWorld::new(50.0, 8);
        let mut pose = rest_pose(1.0);
        for p in &mut pose {
            p.y += 1.0;
        }
        w.spawn(&pose, vec3(2.0, 0.0, 1.0), 1.0, ALL_JOINTS, 0, 1000.0);
        for _ in 0..60 * 8 {
            w.step(1.0 / 60.0, &NoCollider);
        }
        let b = &w.bodies[0];
        assert!(finite(b));
        assert!(b.sleeping, "body should fall asleep within 8s");
        for j in 0..JOINTS {
            assert!(b.pos[j].y >= 0.0 && b.pos[j].y < 0.8, "joint {j} at {}", b.pos[j].y);
        }
    }

    #[test]
    fn bones_keep_their_length_under_violent_impulse() {
        let mut w = RagdollWorld::new(50.0, 8);
        w.spawn(&rest_pose(1.0), Vec3::ZERO, 1.0, ALL_JOINTS, 0, 1000.0);
        w.blast(vec3(0.3, 0.2, 0.0), 6.0, 30.0);
        let rest = rest_pose(1.0);
        for _ in 0..240 {
            w.step(1.0 / 60.0, &NoCollider);
            assert!(finite(&w.bodies[0]));
        }
        let b = &w.bodies[0];
        for &(a, c) in &[(L_SHO, L_ELB), (R_HIP, R_KNEE), (L_KNEE, L_FOOT), (NECK, CHEST)] {
            let want = rest[a].distance(rest[c]);
            let got = b.pos[a].distance(b.pos[c]);
            assert!((want - got).abs() < want * 0.15, "bone {a}-{c}: {got} vs {want}");
        }
    }

    #[test]
    fn severed_limb_separates_from_body() {
        let mut w = RagdollWorld::new(50.0, 8);
        w.spawn(&rest_pose(1.0), Vec3::ZERO, 1.0, ALL_JOINTS, 0, 1000.0);
        assert!(w.bodies[0].sever(Limb::Head));
        assert!(!w.bodies[0].sever(Limb::Head), "severing twice is a no-op");
        for j in [HEAD] {
            w.bodies[0].vel[j] = vec3(12.0, 4.0, 0.0);
        }
        for _ in 0..30 {
            w.step(1.0 / 60.0, &NoCollider);
        }
        let b = &w.bodies[0];
        assert!(b.pos[HEAD].distance(b.pos[NECK]) > 1.0);
    }

    #[test]
    fn two_bone_preserves_lengths() {
        let k = two_bone(Vec3::ZERO, vec3(0.3, -0.5, 0.1), 0.42, 0.43, Vec3::Z);
        assert!((k.length() - 0.42).abs() < 1e-3);
        assert!((k.distance(vec3(0.3, -0.5, 0.1)) - 0.43).abs() < 1e-3);
    }

    #[test]
    fn pile_of_bodies_stays_stable() {
        let mut w = RagdollWorld::new(50.0, 64);
        for i in 0..40 {
            let mut pose = rest_pose(1.0);
            for p in &mut pose {
                p.y += i as f32 * 0.35;
            }
            w.spawn(&pose, Vec3::ZERO, 1.0, ALL_JOINTS, 0, 1000.0);
        }
        for _ in 0..360 {
            w.step(1.0 / 60.0, &NoCollider);
        }
        assert!(w.bodies.iter().all(finite));
    }
}
