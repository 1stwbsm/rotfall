//! Procedural mesh construction. Everything visible in the game is built here from
//! primitives, with flat shaded faces and per-vertex albedo plus an emissive mask.
use bytemuck::{Pod, Zeroable};
use glam::{vec3, Mat4, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [u8; 4],
}

#[derive(Default)]
pub struct Builder {
    pub v: Vec<Vertex>,
    pub i: Vec<u32>,
    xf: Option<Mat4>,
}

pub type Rgb = [f32; 3];
pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn col(c: Rgb, e: f32) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (e.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

impl Builder {
    pub fn with(&mut self, xf: Mat4, f: impl FnOnce(&mut Builder)) {
        let old = self.xf;
        self.xf = Some(old.map_or(xf, |o| o * xf));
        f(self);
        self.xf = old;
    }

    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [u8; 4]) {
        let (a, b, c, d) = match self.xf {
            Some(m) => (m.transform_point3(a), m.transform_point3(b), m.transform_point3(c), m.transform_point3(d)),
            None => (a, b, c, d),
        };
        let n = (b - a).cross(d - a).try_normalize().or_else(|| (c - b).cross(a - b).try_normalize()).unwrap_or(Vec3::Y);
        let base = self.v.len() as u32;
        for p in [a, b, c, d] {
            self.v.push(Vertex { pos: p.to_array(), normal: n.to_array(), color });
        }
        self.i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Axis-aligned box from center and full size.
    pub fn cube(&mut self, c: Vec3, s: Vec3, color: Rgb) {
        self.cube_e(c, s, color, 0.0)
    }

    pub fn cube_e(&mut self, c: Vec3, s: Vec3, color: Rgb, emissive: f32) {
        self.frustum(c - Vec3::Y * s.y * 0.5, c + Vec3::Y * s.y * 0.5, s.x, s.z, s.x, s.z, color, emissive);
    }

    /// A box tapered along Y: bottom at `a`, top at `b`.
    #[allow(clippy::too_many_arguments)]
    pub fn frustum(&mut self, a: Vec3, b: Vec3, wa: f32, da: f32, wb: f32, db: f32, color: Rgb, emissive: f32) {
        let c = col(color, emissive);
        let (hx, hz, tx, tz) = (wa * 0.5, da * 0.5, wb * 0.5, db * 0.5);
        let b0 = a + vec3(-hx, 0.0, -hz);
        let b1 = a + vec3(hx, 0.0, -hz);
        let b2 = a + vec3(hx, 0.0, hz);
        let b3 = a + vec3(-hx, 0.0, hz);
        let t0 = b + vec3(-tx, 0.0, -tz);
        let t1 = b + vec3(tx, 0.0, -tz);
        let t2 = b + vec3(tx, 0.0, tz);
        let t3 = b + vec3(-tx, 0.0, tz);
        self.quad(t3, t2, t1, t0, c); // top
        self.quad(b0, b1, b2, b3, c); // bottom
        self.quad(b3, b2, t2, t3, c); // +z
        self.quad(b1, b0, t0, t1, c); // -z
        self.quad(b2, b1, t1, t2, c); // +x
        self.quad(b0, b3, t3, t0, c); // -x
    }

    /// Faceted cylinder or cone around the segment a → b.
    pub fn cyl(&mut self, a: Vec3, b: Vec3, ra: f32, rb: f32, seg: u32, color: Rgb, emissive: f32) {
        let c = col(color, emissive);
        let axis = (b - a).try_normalize().unwrap_or(Vec3::Y);
        let u = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y }.cross(axis).normalize();
        let v = axis.cross(u);
        let at = |k: u32, r: f32, o: Vec3| {
            let t = k as f32 / seg as f32 * std::f32::consts::TAU;
            o + (u * t.cos() + v * t.sin()) * r
        };
        for k in 0..seg {
            let (p0, p1) = (at(k, ra, a), at(k + 1, ra, a));
            let (q0, q1) = (at(k, rb, b), at(k + 1, rb, b));
            self.quad(p0, p1, q1, q0, c);
            if ra > 0.0 {
                self.quad(a, p1, p0, p0, c);
            }
            if rb > 0.0 {
                self.quad(b, q0, q1, q1, c);
            }
        }
    }

    /// Faceted ellipsoid.
    pub fn sphere(&mut self, c: Vec3, r: Vec3, lat: u32, lon: u32, color: Rgb, emissive: f32) {
        let col = col(color, emissive);
        let pt = |i: u32, j: u32| {
            let th = i as f32 / lat as f32 * std::f32::consts::PI;
            let ph = j as f32 / lon as f32 * std::f32::consts::TAU;
            c + vec3(th.sin() * ph.cos() * r.x, th.cos() * r.y, th.sin() * ph.sin() * r.z)
        };
        for i in 0..lat {
            for j in 0..lon {
                self.quad(pt(i, j), pt(i, j + 1), pt(i + 1, j + 1), pt(i + 1, j), col);
            }
        }
    }
}

macro_rules! mesh_ids {
    ($($name:ident),* $(,)?) => {
        mesh_ids!(@step 0usize, $($name,)*);
    };
    (@step $n:expr, $head:ident, $($tail:ident,)*) => {
        pub const $head: usize = $n;
        mesh_ids!(@step $n + 1usize, $($tail,)*);
    };
    (@step $n:expr,) => { pub const MESH_COUNT: usize = $n; };
}

mesh_ids!(
    CUBE, CYL, SPHERE, LIMB, TORSO, HEAD_Z, HEAD_Z2, HEAD_P, HAND, FOOT, BLOATER_BELLY, BOSS_HEAD,
    BARREL, CAR, CAR2, TREE, ROCK, LAMP, CRATE, DUMPSTER, TIRE, RUBBLE, FENCE,
    WALL_WOOD, WALL_STEEL, GATE, SPIKES, MINE, WIRE, SENTRY_BASE,
    TURRET_MG, TURRET_SHOTGUN, TURRET_FLAME, TURRET_TESLA, TURRET_ROCKET, TURRET_MINI, TURRET_MORTAR,
    FLOODLIGHT, AMMO_DEPOT, REPAIR_BAY, CORE, CORE_RING,
    GIB, CASING, PLANK, SHARD, PICK_SCRAP, PICK_AMMO, PICK_MED, GRENADE, ROCKET, SAWBLADE,
    GUN0, GUN1, GUN2, GUN3, GUN4, GUN5, GUN6, GUN7, GUN8, GUN9,
    GUN10, GUN11, GUN12, GUN13, GUN14, GUN15, GUN16, GUN17,
);

const STEEL: Rgb = rgb(62, 66, 74);
const DARK: Rgb = rgb(30, 31, 36);
const WOOD: Rgb = rgb(105, 72, 44);
const OLIVE: Rgb = rgb(72, 84, 52);
const HAZARD: Rgb = rgb(208, 164, 38);

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    vec3(x, y, z)
}

fn build(id: usize, b: &mut Builder) {
    match id {
        CUBE => b.cube(Vec3::ZERO, Vec3::ONE, [1.0; 3]),
        CYL => b.cyl(v(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.5, 0.5, 10, [1.0; 3], 0.0),
        SPHERE => b.sphere(Vec3::ZERO, Vec3::splat(0.5), 6, 10, [1.0; 3], 0.0),
        // Unit limb: bottom at y=0 (start joint) to y=1 (end joint), tapered, width ~1.
        LIMB => b.frustum(v(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 1.0, 1.0, 0.72, 0.72, [1.0; 3], 0.0),
        TORSO => {
            b.frustum(v(0.0, 0.0, 0.0), v(0.0, 0.7, 0.0), 0.72, 0.8, 1.0, 1.0, [1.0; 3], 0.0);
            b.frustum(v(0.0, 0.7, 0.0), v(0.0, 1.0, 0.0), 1.0, 1.0, 0.6, 0.7, [1.0; 3], 0.0);
        }
        HEAD_Z | HEAD_Z2 => {
            let lean = if id == HEAD_Z2 { 0.04 } else { 0.0 };
            b.sphere(Vec3::ZERO, v(0.5, 0.55, 0.5), 4, 7, [1.0; 3], 0.0);
            b.cube(v(0.0, -0.32, 0.22 + lean), v(0.55, 0.26, 0.5), [0.75, 0.72, 0.7]);
            // Glowing eyes.
            b.cube_e(v(-0.2, 0.08, 0.44), v(0.18, 0.13, 0.12), [1.0, 0.75, 0.25], 1.0);
            b.cube_e(v(0.2, 0.08, 0.44), v(0.18, 0.13, 0.12), [1.0, 0.75, 0.25], 1.0);
        }
        BOSS_HEAD => {
            b.sphere(Vec3::ZERO, v(0.6, 0.5, 0.6), 4, 7, [1.0; 3], 0.0);
            b.cube(v(0.0, -0.32, 0.25), v(0.8, 0.28, 0.6), [0.8, 0.75, 0.7]);
            for (x, z) in [(-0.4, 0.0), (0.4, 0.0), (-0.25, -0.3), (0.25, -0.3)] {
                b.cyl(v(x, 0.25, z), v(x * 1.35, 0.85, z - 0.15), 0.1, 0.0, 5, [0.85, 0.82, 0.7], 0.0);
            }
            b.cube_e(v(-0.23, 0.06, 0.52), v(0.22, 0.14, 0.14), [1.0, 0.2, 0.08], 1.0);
            b.cube_e(v(0.23, 0.06, 0.52), v(0.22, 0.14, 0.14), [1.0, 0.2, 0.08], 1.0);
        }
        HEAD_P => {
            let skin = rgb(190, 150, 120);
            b.sphere(Vec3::ZERO, v(0.42, 0.5, 0.44), 4, 8, skin, 0.0);
            // Helmet.
            b.sphere(v(0.0, 0.14, -0.03), v(0.54, 0.46, 0.56), 4, 8, rgb(52, 62, 46), 0.0);
            b.cube(v(0.0, 0.0, 0.3), v(0.82, 0.08, 0.5), rgb(40, 48, 36));
            // Tactical visor light.
            b.cube_e(v(0.0, 0.3, 0.5), v(0.25, 0.12, 0.1), [0.5, 0.95, 1.0], 1.0);
        }
        HAND => b.cube(v(0.0, 0.5, 0.0), v(1.0, 1.0, 0.7), [1.0; 3]),
        FOOT => b.cube(v(0.0, 0.5, 0.25), v(1.0, 1.0, 1.5), [1.0; 3]),
        BLOATER_BELLY => {
            b.sphere(Vec3::ZERO, Vec3::splat(0.5), 6, 9, [1.0; 3], 0.0);
            for (x, y, z) in [(0.25, 0.2, 0.32), (-0.3, -0.1, 0.3), (0.05, -0.28, 0.36), (0.36, -0.18, 0.1), (-0.15, 0.3, 0.3)] {
                b.sphere(v(x, y, z), Vec3::splat(0.1), 3, 5, [0.75, 0.95, 0.2], 0.8);
            }
        }
        BARREL => {
            let red = rgb(158, 40, 26);
            b.cyl(v(0.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.34, 0.34, 10, red, 0.0);
            b.cyl(v(0.0, 0.32, 0.0), v(0.0, 0.36, 0.0), 0.355, 0.355, 10, rgb(60, 20, 16), 0.0);
            b.cyl(v(0.0, 0.66, 0.0), v(0.0, 0.70, 0.0), 0.355, 0.355, 10, rgb(60, 20, 16), 0.0);
            b.cube(v(0.0, 0.5, 0.345), v(0.3, 0.26, 0.03), HAZARD);
        }
        CAR | CAR2 => {
            let (bodyc, topc) = if id == CAR { (rgb(92, 50, 38), rgb(74, 42, 34)) } else { (rgb(52, 66, 78), rgb(40, 52, 62)) };
            b.cube(v(0.0, 0.62, 0.0), v(1.85, 0.56, 4.2), bodyc);
            b.frustum(v(0.0, 0.9, -0.25), v(0.0, 1.38, -0.35), 1.7, 2.3, 1.42, 1.5, topc, 0.0);
            b.cube(v(0.0, 1.14, 0.72), v(1.38, 0.3, 0.05), rgb(16, 20, 26));
            b.cube(v(0.0, 1.14, -1.28), v(1.38, 0.3, 0.05), rgb(16, 20, 26));
            for (x, z) in [(-0.9, 1.3), (0.9, 1.3), (-0.9, -1.3), (0.9, -1.3)] {
                b.cyl(v(x - 0.14, 0.34, z), v(x + 0.14, 0.34, z), 0.34, 0.34, 8, rgb(18, 18, 20), 0.0);
            }
            b.cube(v(0.0, 0.6, 2.11), v(1.7, 0.2, 0.05), rgb(120, 110, 90));
        }
        TREE => {
            let bark = rgb(46, 38, 32);
            b.cyl(v(0.0, 0.0, 0.0), v(0.1, 2.6, 0.05), 0.3, 0.16, 7, bark, 0.0);
            b.cyl(v(0.1, 2.0, 0.05), v(1.1, 3.3, 0.5), 0.13, 0.03, 5, bark, 0.0);
            b.cyl(v(0.1, 2.4, 0.05), v(-1.0, 3.5, -0.3), 0.11, 0.03, 5, bark, 0.0);
            b.cyl(v(0.1, 1.6, 0.05), v(-0.2, 2.8, 1.2), 0.1, 0.03, 5, bark, 0.0);
            b.cyl(v(0.1, 2.6, 0.05), v(0.4, 4.0, -0.9), 0.1, 0.02, 5, bark, 0.0);
        }
        ROCK => {
            b.sphere(Vec3::ZERO, v(1.0, 0.7, 0.85), 3, 5, rgb(82, 80, 76), 0.0);
        }
        LAMP => {
            b.cyl(v(0.0, 0.0, 0.0), v(0.0, 5.0, 0.0), 0.09, 0.06, 6, DARK, 0.0);
            b.cube(v(0.0, 5.0, 0.42), v(0.14, 0.09, 0.96), DARK);
            b.cube_e(v(0.0, 4.93, 0.72), v(0.24, 0.05, 0.42), [1.0, 0.86, 0.6], 1.0);
        }
        CRATE => {
            b.cube(Vec3::ZERO, Vec3::ONE, WOOD);
            for (dy, dz) in [(0.5, 0.5), (-0.5, 0.5), (0.5, -0.5), (-0.5, -0.5)] {
                b.cube(v(0.0, dy * 0.92, dz * 0.92), v(1.04, 0.1, 0.1), rgb(70, 47, 28));
                b.cube(v(dy * 0.92, 0.0, dz * 0.92), v(0.1, 1.04, 0.1), rgb(70, 47, 28));
            }
        }
        DUMPSTER => {
            b.frustum(v(0.0, 0.1, 0.0), v(0.0, 1.3, 0.0), 2.6, 1.3, 3.0, 1.6, rgb(38, 72, 56), 0.0);
            b.cube(v(0.0, 1.34, 0.0), v(3.05, 0.08, 1.62), rgb(26, 30, 28));
        }
        TIRE => b.cyl(v(0.0, 0.0, 0.0), v(0.0, 0.32, 0.0), 0.5, 0.5, 9, rgb(20, 20, 22), 0.0),
        RUBBLE => {
            let c = rgb(84, 80, 76);
            b.cube(v(-0.3, 0.13, 0.2), v(0.7, 0.26, 0.5), c);
            b.cube(v(0.35, 0.2, -0.2), v(0.6, 0.4, 0.8), rgb(70, 66, 64));
            b.cube(v(0.05, 0.1, 0.5), v(0.5, 0.2, 0.3), rgb(96, 88, 80));
            b.cube(v(-0.42, 0.08, -0.45), v(0.4, 0.16, 0.4), c);
        }
        FENCE => {
            for x in [-0.95, 0.95] {
                b.cube(v(x, 0.8, 0.0), v(0.09, 1.6, 0.09), DARK);
            }
            for y in [0.3, 0.7, 1.1, 1.5] {
                b.cube(v(0.0, y, 0.0), v(2.0, 0.03, 0.03), rgb(70, 72, 76));
            }
            for k in -4..=4 {
                b.cube(v(k as f32 * 0.22, 0.8, 0.0), v(0.03, 1.5, 0.03), rgb(70, 72, 76));
            }
        }
        // ---------------- base structures; each footprint is 2×2 units centered at origin.
        WALL_WOOD => {
            for k in 0..5 {
                let z = -0.8 + k as f32 * 0.4;
                let h = 2.0 + (k as f32 * 2.7).sin() * 0.22;
                b.frustum(v(0.0, 0.0, z), v(0.0, h, z), 1.0, 0.36, 0.85, 0.3, [WOOD[0] * (0.85 + 0.05 * k as f32), WOOD[1] * 0.92, WOOD[2]], 0.0);
                b.frustum(v(0.0, h, z), v(0.0, h + 0.25, z), 0.85, 0.3, 0.1, 0.06, rgb(130, 96, 60), 0.0);
            }
            b.cube(v(0.0, 0.5, 0.0), v(1.1, 0.16, 2.0), rgb(70, 47, 28));
            b.cube(v(0.0, 1.4, 0.0), v(1.1, 0.16, 2.0), rgb(70, 47, 28));
        }
        WALL_STEEL => {
            b.frustum(v(0.0, 0.0, 0.0), v(0.0, 2.3, 0.0), 1.9, 2.0, 1.45, 2.0, STEEL, 0.0);
            b.cube(v(0.0, 2.36, 0.0), v(1.5, 0.12, 2.0), rgb(40, 42, 48));
            for z in [-0.66, 0.66] {
                b.cube(v(0.0, 1.1, z), v(1.72, 2.1, 0.12), rgb(48, 52, 58));
            }
            b.cube(v(0.0, 0.35, 0.0), v(1.96, 0.2, 2.0), HAZARD);
        }
        GATE => {
            for z in [-0.92, 0.92] {
                b.cube(v(0.0, 1.25, z), v(1.0, 2.5, 0.2), STEEL);
            }
            b.cube(v(0.0, 2.4, 0.0), v(1.0, 0.2, 1.7), HAZARD);
            for k in 0..6 {
                let y = 0.25 + k as f32 * 0.35;
                b.cube(v(0.0, y, 0.0), v(0.2, 0.2, 1.7), rgb(90, 94, 100));
            }
            b.cube_e(v(0.0, 2.55, 0.0), v(0.25, 0.1, 0.25), [0.3, 1.0, 0.4], 1.0);
        }
        SPIKES => {
            b.cube(v(0.0, 0.03, 0.0), v(1.9, 0.06, 1.9), rgb(50, 44, 38));
            for i in 0..5 {
                for j in 0..5 {
                    let (x, z) = (-0.72 + 0.36 * i as f32, -0.72 + 0.36 * j as f32);
                    let h = 0.45 + ((i * 3 + j * 5) % 4) as f32 * 0.09;
                    b.cyl(v(x, 0.05, z), v(x + 0.03, h, z), 0.07, 0.0, 4, rgb(120, 118, 112), 0.0);
                }
            }
        }
        MINE => {
            b.cyl(v(0.0, 0.0, 0.0), v(0.0, 0.1, 0.0), 0.42, 0.34, 8, OLIVE, 0.0);
            b.cyl(v(0.0, 0.1, 0.0), v(0.0, 0.14, 0.0), 0.1, 0.08, 6, [1.0, 0.1, 0.05], 1.0);
        }
        WIRE => {
            for x in [-0.8, 0.8] {
                b.cube(v(x, 0.5, 0.0), v(0.08, 1.0, 0.08), DARK);
            }
            for k in 0..14 {
                let t = k as f32 / 14.0;
                let a = t * std::f32::consts::TAU * 3.5;
                let (x0, y0, z0) = (-0.9 + t * 1.8, 0.55 + a.sin() * 0.4, a.cos() * 0.4);
                let a2 = (t + 0.0714) * std::f32::consts::TAU * 3.5;
                let (x1, y1, z1) = (-0.9 + (t + 0.0714) * 1.8, 0.55 + a2.sin() * 0.4, a2.cos() * 0.4);
                b.cyl(v(x0, y0, z0), v(x1, y1, z1), 0.022, 0.022, 3, rgb(150, 150, 150), 0.0);
            }
        }
        SENTRY_BASE => {
            b.frustum(v(0.0, 0.0, 0.0), v(0.0, 0.2, 0.0), 1.7, 1.7, 1.45, 1.45, rgb(44, 47, 54), 0.0);
            b.cyl(v(0.0, 0.2, 0.0), v(0.0, 0.85, 0.0), 0.34, 0.24, 8, rgb(66, 70, 78), 0.0);
            for (x, z) in [(-0.65, -0.65), (0.65, -0.65), (-0.65, 0.65), (0.65, 0.65)] {
                b.cube(v(x, 0.22, z), v(0.22, 0.08, 0.22), HAZARD);
            }
        }
        // Turret heads: origin at the pivot, barrel along +Z.
        TURRET_MG => {
            b.cube(v(0.0, 0.0, 0.0), v(0.62, 0.42, 0.8), OLIVE);
            b.cyl(v(-0.12, 0.05, 0.3), v(-0.12, 0.05, 1.15), 0.05, 0.05, 6, DARK, 0.0);
            b.cyl(v(0.12, 0.05, 0.3), v(0.12, 0.05, 1.15), 0.05, 0.05, 6, DARK, 0.0);
            b.cube_e(v(0.0, 0.14, 0.41), v(0.18, 0.08, 0.04), [0.3, 1.0, 0.6], 1.0);
        }
        TURRET_SHOTGUN => {
            b.cube(v(0.0, 0.0, 0.0), v(0.74, 0.5, 0.74), rgb(92, 70, 40));
            for (x, y) in [(-0.17, 0.09), (0.17, 0.09), (-0.17, -0.09), (0.17, -0.09)] {
                b.cyl(v(x, y, 0.3), v(x * 1.2, y * 1.2, 0.98), 0.075, 0.09, 6, DARK, 0.0);
            }
        }
        TURRET_FLAME => {
            b.cube(v(0.0, 0.0, -0.1), v(0.6, 0.46, 0.6), rgb(120, 48, 30));
            b.cyl(v(-0.34, 0.0, -0.3), v(-0.34, 0.0, 0.2), 0.14, 0.14, 8, rgb(170, 60, 30), 0.0);
            b.cyl(v(0.34, 0.0, -0.3), v(0.34, 0.0, 0.2), 0.14, 0.14, 8, rgb(170, 60, 30), 0.0);
            b.cyl(v(0.0, 0.0, 0.1), v(0.0, 0.0, 0.92), 0.1, 0.14, 7, DARK, 0.0);
            b.cyl(v(0.0, 0.0, 0.92), v(0.0, 0.0, 0.94), 0.08, 0.08, 6, [1.0, 0.5, 0.15], 1.0);
        }
        TURRET_TESLA => {
            b.cyl(v(0.0, -0.18, 0.0), v(0.0, 0.5, 0.0), 0.2, 0.08, 8, rgb(70, 76, 100), 0.0);
            for y in [0.0, 0.2, 0.38] {
                b.cyl(v(0.0, y, 0.0), v(0.0, y + 0.05, 0.0), 0.3 - y * 0.35, 0.3 - y * 0.35, 9, rgb(170, 110, 60), 0.0);
            }
            b.sphere(v(0.0, 0.65, 0.0), Vec3::splat(0.2), 4, 7, [0.45, 0.8, 1.0], 1.0);
        }
        TURRET_ROCKET => {
            b.cube(v(0.0, 0.0, 0.0), v(0.86, 0.6, 1.0), rgb(60, 72, 50));
            for (x, y) in [(-0.2, 0.13), (0.2, 0.13), (-0.2, -0.13), (0.2, -0.13)] {
                b.cyl(v(x, y, 0.49), v(x, y, 0.53), 0.1, 0.1, 7, rgb(190, 40, 30), 0.5);
            }
        }
        TURRET_MINI => {
            b.cube(v(0.0, 0.0, -0.05), v(0.66, 0.5, 0.76), rgb(46, 52, 60));
            for k in 0..6 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU;
                let (x, y) = (a.cos() * 0.11, a.sin() * 0.11);
                b.cyl(v(x, y, 0.3), v(x, y, 1.26), 0.035, 0.035, 5, DARK, 0.0);
            }
            b.cyl(v(0.0, 0.0, 0.95), v(0.0, 0.0, 1.03), 0.16, 0.16, 8, rgb(100, 102, 110), 0.0);
        }
        TURRET_MORTAR => {
            b.cube(v(0.0, -0.1, 0.0), v(0.8, 0.3, 0.8), rgb(66, 70, 58));
            b.cyl(v(0.0, -0.05, -0.1), v(0.0, 1.0, 0.55), 0.21, 0.17, 9, DARK, 0.0);
        }
        FLOODLIGHT => {
            b.cube(v(0.0, 0.08, 0.0), v(1.2, 0.16, 1.2), rgb(44, 47, 54));
            b.cyl(v(0.0, 0.1, 0.0), v(0.0, 3.0, 0.0), 0.09, 0.06, 6, rgb(60, 64, 70), 0.0);
            b.cube(v(0.0, 3.0, 0.0), v(1.1, 0.34, 0.26), DARK);
            b.cube_e(v(-0.28, 2.9, 0.0), v(0.42, 0.06, 0.2), [1.0, 0.95, 0.85], 1.0);
            b.cube_e(v(0.28, 2.9, 0.0), v(0.42, 0.06, 0.2), [1.0, 0.95, 0.85], 1.0);
        }
        AMMO_DEPOT => {
            b.cube(v(0.0, 0.5, 0.0), v(1.75, 1.0, 1.4), OLIVE);
            b.cube(v(0.0, 1.05, 0.0), v(1.84, 0.12, 1.5), rgb(52, 62, 38));
            b.cube_e(v(0.0, 0.55, 0.71), v(1.0, 0.3, 0.03), [1.0, 0.75, 0.18], 0.7);
            for k in 0..4 {
                let x = -0.5 + k as f32 * 0.33;
                b.cyl(v(x, 1.11, 0.0), v(x, 1.42, 0.0), 0.06, 0.04, 6, HAZARD, 0.0);
            }
        }
        REPAIR_BAY => {
            b.cube(v(0.0, 0.3, 0.0), v(1.75, 0.6, 1.75), rgb(60, 66, 76));
            b.cube_e(v(0.0, 0.62, 0.0), v(0.9, 0.04, 0.26), [0.3, 0.85, 0.5], 1.0);
            b.cube_e(v(0.0, 0.62, 0.0), v(0.26, 0.04, 0.9), [0.3, 0.85, 0.5], 1.0);
            b.cyl(v(0.72, 0.6, 0.72), v(0.72, 1.8, 0.72), 0.05, 0.03, 5, DARK, 0.0);
            b.sphere(v(0.72, 1.9, 0.72), Vec3::splat(0.09), 3, 5, [0.35, 1.0, 0.65], 1.0);
        }
        CORE => {
            // The reactor footprint is 4×4 units.
            b.frustum(v(0.0, 0.0, 0.0), v(0.0, 0.5, 0.0), 3.9, 3.9, 3.3, 3.3, rgb(46, 49, 56), 0.0);
            for (x, z) in [(-1.45, -1.45), (1.45, -1.45), (-1.45, 1.45), (1.45, 1.45)] {
                b.frustum(v(x, 0.4, z), v(x * 0.66, 3.2, z * 0.66), 0.5, 0.5, 0.26, 0.26, STEEL, 0.0);
                b.cube(v(x, 0.55, z), v(0.62, 0.12, 0.62), HAZARD);
            }
            b.cyl(v(0.0, 0.5, 0.0), v(0.0, 3.3, 0.0), 0.72, 0.62, 12, [0.28, 0.9, 1.0], 1.0);
            b.cyl(v(0.0, 3.3, 0.0), v(0.0, 3.5, 0.0), 0.86, 0.5, 12, rgb(60, 64, 72), 0.0);
        }
        CORE_RING => {
            b.cyl(v(0.0, -0.05, 0.0), v(0.0, 0.05, 0.0), 1.15, 1.15, 16, rgb(86, 94, 108), 0.0);
        }
        GIB => {
            b.sphere(Vec3::ZERO, v(0.5, 0.42, 0.46), 3, 5, [1.0; 3], 0.0);
        }
        CASING => b.cyl(v(0.0, -0.5, 0.0), v(0.0, 0.5, 0.0), 0.5, 0.5, 5, [0.85, 0.68, 0.25], 0.0),
        PLANK => b.cube(Vec3::ZERO, v(1.0, 0.25, 0.5), [1.0; 3]),
        SHARD => b.cyl(v(0.0, -0.5, 0.0), v(0.0, 0.5, 0.0), 0.5, 0.0, 3, [1.0; 3], 0.0),
        PICK_SCRAP => {
            b.cube_e(Vec3::ZERO, Vec3::splat(0.7), [1.0, 0.8, 0.24], 0.8);
        }
        PICK_AMMO => {
            b.cube(Vec3::ZERO, v(1.0, 0.6, 0.7), OLIVE);
            b.cube_e(v(0.0, 0.0, 0.36), v(0.6, 0.2, 0.02), [1.0, 0.75, 0.18], 1.0);
        }
        PICK_MED => {
            b.cube(Vec3::ZERO, v(0.9, 0.5, 0.9), rgb(220, 220, 215));
            b.cube_e(v(0.0, 0.26, 0.0), v(0.6, 0.02, 0.18), [1.0, 0.14, 0.12], 1.0);
            b.cube_e(v(0.0, 0.26, 0.0), v(0.18, 0.02, 0.6), [1.0, 0.14, 0.12], 1.0);
        }
        GRENADE => b.sphere(Vec3::ZERO, Vec3::splat(0.5), 3, 6, rgb(62, 80, 50), 0.0),
        ROCKET => {
            b.cyl(v(0.0, 0.0, -0.5), v(0.0, 0.0, 0.3), 0.14, 0.14, 7, rgb(90, 96, 90), 0.0);
            b.cyl(v(0.0, 0.0, 0.3), v(0.0, 0.0, 0.6), 0.14, 0.0, 7, rgb(170, 40, 30), 0.0);
        }
        SAWBLADE => {
            b.cyl(v(0.0, -0.05, 0.0), v(0.0, 0.05, 0.0), 0.35, 0.35, 10, rgb(170, 172, 180), 0.0);
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let d = v(a.cos(), 0.0, a.sin());
                let t = v(-a.sin(), 0.0, a.cos());
                b.cyl(d * 0.3 - t * 0.08, d * 0.52 + t * 0.06, 0.05, 0.0, 3, rgb(220, 222, 230), 0.0);
            }
        }
        id if id >= GUN0 => gun(id - GUN0, b),
        _ => b.cube(Vec3::ZERO, Vec3::ONE, [1.0, 0.0, 1.0]),
    }
}

/// Guns are authored with the grip at the origin, barrel along +Z; ~1 unit is one meter.
fn gun(i: usize, b: &mut Builder) {
    let black = rgb(32, 33, 38);
    let metal = rgb(78, 82, 92);
    let polymer = rgb(48, 52, 46);
    let woodc = rgb(110, 70, 38);
    let grip = |b: &mut Builder, c: Rgb| b.cube(v(0.0, -0.06, -0.02), v(0.05, 0.13, 0.06), c);
    match i {
        0 => {
            grip(b, black);
            b.cube(v(0.0, 0.03, 0.08), v(0.045, 0.06, 0.26), metal);
        }
        1 => {
            grip(b, black);
            b.cube(v(0.0, 0.03, 0.12), v(0.06, 0.08, 0.34), black);
            b.cube(v(0.0, -0.1, 0.14), v(0.04, 0.17, 0.05), metal);
            b.cyl(v(0.0, 0.04, 0.29), v(0.0, 0.04, 0.44), 0.016, 0.016, 5, metal, 0.0);
            b.cube(v(0.0, 0.03, -0.14), v(0.03, 0.03, 0.2), metal);
        }
        2 => {
            grip(b, woodc);
            b.cube(v(0.0, 0.03, 0.16), v(0.06, 0.09, 0.5), black);
            b.cube(v(0.0, 0.03, 0.46), v(0.05, 0.05, 0.22), woodc);
            b.cyl(v(0.0, 0.05, 0.4), v(0.0, 0.05, 0.78), 0.016, 0.016, 5, metal, 0.0);
            b.cube(v(0.0, -0.1, 0.2), v(0.04, 0.2, 0.08), rgb(50, 44, 38));
            b.cube(v(0.0, 0.01, -0.2), v(0.05, 0.1, 0.28), woodc);
        }
        3 => {
            grip(b, woodc);
            b.cube(v(0.0, 0.03, 0.14), v(0.06, 0.08, 0.3), black);
            b.cyl(v(0.0, 0.05, 0.25), v(0.0, 0.05, 0.8), 0.024, 0.024, 6, metal, 0.0);
            b.cube(v(0.0, -0.005, 0.46), v(0.06, 0.05, 0.2), woodc);
            b.cube(v(0.0, 0.0, -0.22), v(0.055, 0.1, 0.32), woodc);
        }
        4 => {
            grip(b, polymer);
            b.cube(v(0.0, 0.03, 0.18), v(0.07, 0.1, 0.46), polymer);
            b.cyl(v(0.0, -0.1, 0.18), v(0.0, -0.1, 0.2), 0.12, 0.12, 9, black, 0.0);
            b.cyl(v(0.0, 0.05, 0.4), v(0.0, 0.05, 0.76), 0.028, 0.028, 6, metal, 0.0);
            b.cube(v(0.0, 0.02, -0.18), v(0.05, 0.09, 0.22), polymer);
        }
        5 => {
            grip(b, polymer);
            b.cube(v(0.0, 0.03, 0.14), v(0.06, 0.08, 0.5), polymer);
            b.cyl(v(0.0, 0.05, 0.39), v(0.0, 0.05, 1.15), 0.016, 0.013, 6, black, 0.0);
            b.cyl(v(0.0, 0.14, 0.03), v(0.0, 0.14, 0.33), 0.03, 0.03, 7, black, 0.0);
            b.cube(v(0.0, 0.0, -0.28), v(0.05, 0.11, 0.36), polymer);
            b.cube(v(0.04, 0.06, 0.05), v(0.08, 0.016, 0.016), metal);
        }
        6 => {
            grip(b, black);
            b.cube(v(0.0, 0.04, 0.18), v(0.08, 0.11, 0.54), polymer);
            b.cyl(v(0.0, 0.05, 0.45), v(0.0, 0.05, 0.98), 0.024, 0.024, 6, black, 0.0);
            b.cube(v(0.0, -0.1, 0.18), v(0.13, 0.15, 0.14), rgb(70, 76, 50));
            b.cube(v(0.0, 0.02, -0.2), v(0.06, 0.1, 0.24), polymer);
            b.cube(v(-0.04, -0.06, 0.8), v(0.015, 0.2, 0.015), metal);
            b.cube(v(0.04, -0.06, 0.8), v(0.015, 0.2, 0.015), metal);
        }
        7 => {
            grip(b, black);
            b.cube(v(0.0, 0.0, 0.12), v(0.16, 0.16, 0.4), rgb(60, 64, 72));
            for k in 0..6 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU;
                let (x, y) = (a.cos() * 0.055, a.sin() * 0.055);
                b.cyl(v(x, y, 0.3), v(x, y, 1.02), 0.014, 0.014, 4, black, 0.0);
            }
            b.cyl(v(0.0, 0.0, 0.7), v(0.0, 0.0, 0.74), 0.08, 0.08, 8, metal, 0.0);
            b.cube(v(0.12, -0.05, 0.1), v(0.1, 0.2, 0.28), rgb(70, 76, 50));
        }
        8 => {
            grip(b, polymer);
            b.cube(v(0.0, 0.02, 0.1), v(0.07, 0.08, 0.26), polymer);
            b.cyl(v(0.0, 0.04, 0.05), v(0.0, 0.04, 0.3), 0.075, 0.075, 8, black, 0.0);
            b.cyl(v(0.0, 0.05, 0.28), v(0.0, 0.05, 0.62), 0.05, 0.05, 8, rgb(66, 76, 50), 0.0);
            b.cube(v(0.0, 0.0, -0.2), v(0.04, 0.09, 0.28), polymer);
        }
        9 => {
            b.cyl(v(0.0, 0.08, -0.5), v(0.0, 0.08, 0.72), 0.062, 0.062, 8, rgb(66, 76, 50), 0.0);
            b.cyl(v(0.0, 0.08, 0.72), v(0.0, 0.08, 0.95), 0.05, 0.1, 8, rgb(170, 40, 30), 0.0);
            b.cyl(v(0.0, 0.08, -0.72), v(0.0, 0.08, -0.5), 0.1, 0.062, 8, black, 0.0);
            grip(b, black);
        }
        10 => {
            grip(b, black);
            b.cube(v(0.0, 0.03, 0.14), v(0.07, 0.08, 0.42), rgb(120, 48, 30));
            b.cyl(v(0.0, -0.12, -0.1), v(0.0, -0.12, 0.3), 0.075, 0.075, 8, rgb(180, 64, 30), 0.0);
            b.cyl(v(0.0, 0.05, 0.34), v(0.0, 0.05, 0.8), 0.03, 0.05, 7, metal, 0.0);
            b.cyl(v(0.0, 0.05, 0.8), v(0.0, 0.05, 0.82), 0.03, 0.03, 6, [1.0, 0.5, 0.15], 1.0);
        }
        11 => {
            grip(b, black);
            b.cube(v(0.0, 0.03, 0.12), v(0.08, 0.1, 0.36), rgb(56, 62, 88));
            for k in 0..3 {
                let z = 0.34 + k as f32 * 0.09;
                b.cyl(v(0.0, 0.05, z), v(0.0, 0.05, z + 0.03), 0.07 - k as f32 * 0.012, 0.07 - k as f32 * 0.012, 8, rgb(170, 110, 60), 0.0);
            }
            b.sphere(v(0.0, 0.05, 0.66), Vec3::splat(0.05), 3, 6, [0.45, 0.8, 1.0], 1.0);
        }
        12 => {
            grip(b, rgb(200, 200, 205));
            b.cube(v(0.0, 0.04, 0.2), v(0.07, 0.1, 0.6), rgb(190, 192, 200));
            b.cyl(v(0.0, 0.04, 0.5), v(0.0, 0.04, 0.82), 0.03, 0.018, 7, metal, 0.0);
            b.cube_e(v(0.0, 0.1, 0.22), v(0.03, 0.02, 0.5), [1.0, 0.36, 0.2], 1.0);
        }
        13 => {
            grip(b, black);
            b.cube(v(0.0, 0.03, 0.14), v(0.07, 0.08, 0.4), rgb(120, 96, 30));
            b.cyl(v(0.0, 0.05, 0.3), v(0.0, 0.08, 0.3), 0.22, 0.22, 10, rgb(170, 172, 180), 0.0);
            b.cube(v(0.0, 0.04, 0.56), v(0.04, 0.03, 0.26), metal);
        }
        14 => {
            grip(b, black);
            b.cube(v(0.0, 0.04, 0.2), v(0.08, 0.13, 0.66), rgb(40, 46, 60));
            for x in [-0.03, 0.03] {
                b.cube(v(x, 0.06, 0.72), v(0.02, 0.07, 0.56), metal);
            }
            b.cube_e(v(0.042, 0.04, 0.2), v(0.002, 0.05, 0.5), [0.3, 0.75, 1.0], 1.0);
            b.cube_e(v(-0.042, 0.04, 0.2), v(0.002, 0.05, 0.5), [0.3, 0.75, 1.0], 1.0);
        }
        15 => {
            grip(b, woodc);
            b.cyl(v(0.0, 0.04, 0.0), v(0.0, 0.04, 0.12), 0.04, 0.04, 7, metal, 0.0);
            b.cyl(v(0.0, 0.06, 0.1), v(0.0, 0.06, 0.34), 0.02, 0.02, 6, metal, 0.0);
        }
        16 => {
            for x in [-0.09, 0.09] {
                b.with(Mat4::from_translation(v(x, 0.0, 0.0)), |b| {
                    b.cube(v(0.0, -0.06, -0.02), v(0.04, 0.13, 0.05), black);
                    b.cube(v(0.0, 0.03, 0.09), v(0.05, 0.07, 0.24), black);
                    b.cube(v(0.0, -0.1, 0.1), v(0.03, 0.15, 0.03), metal);
                    b.cyl(v(0.0, 0.03, 0.2), v(0.0, 0.03, 0.3), 0.012, 0.012, 5, metal, 0.0);
                });
            }
        }
        _ => {
            grip(b, rgb(220, 230, 240));
            b.cube(v(0.0, 0.03, 0.15), v(0.08, 0.09, 0.42), rgb(190, 215, 230));
            b.cyl(v(0.0, -0.1, -0.05), v(0.0, -0.1, 0.34), 0.06, 0.06, 8, [0.5, 0.85, 1.0], 0.9);
            b.cyl(v(0.0, 0.04, 0.36), v(0.0, 0.04, 0.72), 0.03, 0.055, 7, metal, 0.0);
        }
    }
}

pub struct Library {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Per mesh: first index, index count.
    pub ranges: Vec<(u32, u32)>,
}

pub fn build_library() -> Library {
    let mut lib = Library { vertices: Vec::new(), indices: Vec::new(), ranges: Vec::with_capacity(MESH_COUNT) };
    for id in 0..MESH_COUNT {
        let mut b = Builder::default();
        build(id, &mut b);
        let base = lib.vertices.len() as u32;
        let first = lib.indices.len() as u32;
        lib.vertices.append(&mut b.v);
        lib.indices.extend(b.i.iter().map(|i| i + base));
        lib.ranges.push((first, b.i.len() as u32));
    }
    lib
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_mesh_has_geometry() {
        let lib = build_library();
        assert_eq!(lib.ranges.len(), MESH_COUNT);
        for (i, r) in lib.ranges.iter().enumerate() {
            assert!(r.1 > 0, "mesh {i} is empty");
        }
        assert!(lib.vertices.iter().all(|v| v.pos.iter().all(|x| x.is_finite())));
        // Outward winding makes a closed mesh's signed volume positive, and back-face
        // culling depends on it. Every mesh here is a union of closed shapes.
        for (id, &(first, count)) in lib.ranges.iter().enumerate() {
            let mut vol = 0.0f32;
            for t in lib.indices[first as usize..(first + count) as usize].chunks_exact(3) {
                let p = |i: u32| Vec3::from(lib.vertices[i as usize].pos);
                vol += p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0;
            }
            assert!(vol > 0.0, "mesh {id} is inside out (signed volume {vol})");
        }
        let max = lib.vertices.len() as u32;
        assert!(lib.indices.iter().all(|&i| i < max));
    }
}
