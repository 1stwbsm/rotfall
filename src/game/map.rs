//! Procedural battlefield: a ruined crossroads with static geometry, collision and a
//! baked ground texture.
use crate::ragdoll::Collider;
use crate::render::mesh as m;
use crate::util::Rng;
use glam::{vec3, Mat4, Quat, Vec2, Vec3};

pub const HALF: f32 = 80.0;
const GRID_CELL: f32 = 4.0;
const GRID_DIM: usize = (2.0 * HALF / GRID_CELL) as usize + 1;

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

pub struct Prop {
    pub mesh: usize,
    pub m: Mat4,
    pub tint: [f32; 3],
    pub glow: f32,
    pub center: Vec3,
    pub radius: f32,
}

pub struct Map {
    pub solids: Vec<Aabb>,
    grid: Vec<Vec<u32>>,
    pub props: Vec<Prop>,
    pub lamps: Vec<Vec3>,
    pub barrels: Vec<Vec3>,
    pub floors: Vec<(Vec2, Vec2)>,
}

/// Grid-agnostic circle push-out against a box. Returns a corrected position.
#[inline]
pub fn push_out_circle(p: Vec2, r: f32, min: Vec2, max: Vec2) -> Option<Vec2> {
    let c = p.clamp(min, max);
    let d = p - c;
    let l2 = d.length_squared();
    if l2 >= r * r {
        return None;
    }
    if l2 > 1e-8 {
        let l = l2.sqrt();
        Some(c + d / l * r)
    } else {
        // Center is inside: exit through the nearest side.
        let (dl, dr, db, dt) = (p.x - min.x, max.x - p.x, p.y - min.y, max.y - p.y);
        let m = dl.min(dr).min(db).min(dt);
        Some(if m == dl {
            Vec2::new(min.x - r, p.y)
        } else if m == dr {
            Vec2::new(max.x + r, p.y)
        } else if m == db {
            Vec2::new(p.x, min.y - r)
        } else {
            Vec2::new(p.x, max.y + r)
        })
    }
}

impl Map {
    fn cell(x: f32, z: f32) -> (usize, usize) {
        let f = |v: f32| (((v + HALF) / GRID_CELL).floor().max(0.0) as usize).min(GRID_DIM - 1);
        (f(x), f(z))
    }

    pub fn add_solid(&mut self, a: Aabb) {
        let i = self.solids.len() as u32;
        self.solids.push(a);
        let (x0, z0) = Self::cell(a.min.x, a.min.z);
        let (x1, z1) = Self::cell(a.max.x, a.max.z);
        for z in z0..=z1 {
            for x in x0..=x1 {
                self.grid[z * GRID_DIM + x].push(i);
            }
        }
    }

    fn near(&self, x: f32, z: f32) -> &[u32] {
        let (cx, cz) = Self::cell(x, z);
        &self.grid[cz * GRID_DIM + cx]
    }

    /// Push a 2D circle out of static solids. Boxes at most `step` tall are ignored.
    pub fn resolve_circle(&self, mut p: Vec2, r: f32) -> Vec2 {
        for iter in 0..2 {
            let mut moved = false;
            // The query circle can straddle a cell border, so test its 4 corner cells.
            for (ox, oz) in [(-r, -r), (r, -r), (-r, r), (r, r)] {
                for &i in self.near(p.x + ox, p.y + oz) {
                    let a = &self.solids[i as usize];
                    if let Some(np) = push_out_circle(p, r, Vec2::new(a.min.x, a.min.z), Vec2::new(a.max.x, a.max.z)) {
                        p = np;
                        moved = true;
                    }
                }
            }
            if !moved || iter == 1 {
                break;
            }
        }
        p
    }

    pub fn is_blocked(&self, p: Vec2, r: f32) -> bool {
        self.near(p.x, p.y).iter().any(|&i| {
            let a = &self.solids[i as usize];
            p.x + r > a.min.x && p.x - r < a.max.x && p.y + r > a.min.z && p.y - r < a.max.z
        })
    }

    /// Distance along a horizontal ray to the first box that is tall enough to stop bullets.
    pub fn raycast(&self, from: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        let lim = max;
        for a in &self.solids {
            if a.max.y < from.y {
                continue;
            }
            // Quick reject using distance from the ray start.
            let mut t0 = 0.0f32;
            let mut t1 = lim;
            let mut hit = true;
            for k in [0usize, 2usize] {
                let (o, d, mn, mx) = (from[k], dir[k], a.min[k], a.max[k]);
                if d.abs() < 1e-5 {
                    if o < mn || o > mx {
                        hit = false;
                        break;
                    }
                } else {
                    let (mut ta, mut tb) = ((mn - o) / d, (mx - o) / d);
                    if ta > tb {
                        std::mem::swap(&mut ta, &mut tb);
                    }
                    t0 = t0.max(ta);
                    t1 = t1.min(tb);
                    if t0 > t1 {
                        hit = false;
                        break;
                    }
                }
            }
            if hit && best.is_none_or(|b| t0 < b) {
                best = Some(t0);
            }
        }
        best
    }

    pub fn generate(seed: u64) -> Map {
        let mut map = Map {
            solids: Vec::new(),
            grid: vec![Vec::new(); GRID_DIM * GRID_DIM],
            props: Vec::new(),
            lamps: Vec::new(),
            barrels: Vec::new(),
            floors: Vec::new(),
        };
        let mut r = Rng::new(seed);
        let concrete = [[0.34, 0.33, 0.31], [0.40, 0.37, 0.33], [0.30, 0.27, 0.25], [0.38, 0.26, 0.21]];

        // Ruined buildings ring the crossroads, keeping the roads and the center open.
        let mut placed: Vec<(Vec2, Vec2)> = Vec::new();
        let mut attempts = 0;
        while placed.len() < 18 && attempts < 300 {
            attempts += 1;
            let c = Vec2::new(r.range(-68.0, 68.0), r.range(-68.0, 68.0));
            let he = Vec2::new(r.range(4.0, 9.0), r.range(4.0, 9.0));
            if c.x.abs() < he.x + 8.0 || c.y.abs() < he.y + 8.0 || c.length() < 30.0 + he.length() {
                continue;
            }
            if placed.iter().any(|(oc, oh)| (oc.x - c.x).abs() < oh.x + he.x + 6.0 && (oc.y - c.y).abs() < oh.y + he.y + 6.0) {
                continue;
            }
            placed.push((c, he));
            map.floors.push((c - he, c + he));
            let tint = concrete[r.int(concrete.len())];
            map.building(c, he, tint, &mut r);
        }

        // Street lamps along the road shoulders.
        for k in [-60.0f32, -38.0, 38.0, 60.0] {
            map.lamp(vec3(k, 0.0, 6.2), std::f32::consts::PI);
            map.lamp(vec3(k + 8.0, 0.0, -6.2), 0.0);
            map.lamp(vec3(6.2, 0.0, k), -std::f32::consts::FRAC_PI_2);
            map.lamp(vec3(-6.2, 0.0, k + 8.0), std::f32::consts::FRAC_PI_2);
        }

        // Wrecks parked on the roads, oriented roughly with traffic.
        for _ in 0..22 {
            let along = r.range(20.0, 74.0) * if r.chance(0.5) { 1.0 } else { -1.0 };
            let across = r.range(-4.5, 4.5);
            let horiz = r.chance(0.5);
            let p = if horiz { Vec2::new(along, across) } else { Vec2::new(across, along) };
            if map.is_blocked(p, 3.0) {
                continue;
            }
            let yaw = r.range(-0.5, 0.5) + if horiz { std::f32::consts::FRAC_PI_2 } else { 0.0 };
            let mesh = if r.chance(0.5) { m::CAR } else { m::CAR2 };
            let mm = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), vec3(p.x, 0.0, p.y));
            map.prop(mesh, mm, [1.0; 3], 0.0, vec3(p.x, 1.0, p.y), 3.0);
            // Conservative footprint around the rotated car.
            let (hx, hz) = {
                let (s, c) = yaw.sin_cos();
                (0.95 * c.abs() + 2.1 * s.abs(), 0.95 * s.abs() + 2.1 * c.abs())
            };
            map.add_solid(Aabb { min: vec3(p.x - hx * 0.86, 0.0, p.y - hz * 0.86), max: vec3(p.x + hx * 0.86, 1.4, p.y + hz * 0.86) });
        }

        // Clutter.
        for _ in 0..70 {
            let p = Vec2::new(r.range(-76.0, 76.0), r.range(-76.0, 76.0));
            if p.length() < 21.0 || map.is_blocked(p, 1.8) {
                continue;
            }
            let on_road = p.x.abs() < 8.0 || p.y.abs() < 8.0;
            let roll = r.f();
            let yaw = r.range(0.0, 6.28);
            let at = vec3(p.x, 0.0, p.y);
            if roll < 0.25 && !on_road {
                let s = r.range(0.85, 1.3);
                let mm = Mat4::from_scale_rotation_translation(Vec3::splat(s), Quat::from_rotation_y(yaw), at);
                map.prop(m::TREE, mm, [1.0; 3], 0.0, at + Vec3::Y * 2.0, 4.0 * s);
                map.add_solid(Aabb { min: vec3(p.x - 0.3, 0.0, p.y - 0.3), max: vec3(p.x + 0.3, 3.5, p.y + 0.3) });
            } else if roll < 0.42 {
                let s = r.range(0.7, 1.5);
                let mm = Mat4::from_scale_rotation_translation(vec3(s, s * 0.75, s), Quat::from_rotation_y(yaw), at + Vec3::Y * 0.2);
                map.prop(m::ROCK, mm, [1.0; 3], 0.0, at, 1.5 * s);
                map.add_solid(Aabb { min: vec3(p.x - 0.7 * s, 0.0, p.y - 0.7 * s), max: vec3(p.x + 0.7 * s, 0.7 * s, p.y + 0.7 * s) });
            } else if roll < 0.58 {
                let s = r.range(0.8, 1.3);
                let mm = Mat4::from_scale_rotation_translation(Vec3::splat(s), Quat::from_rotation_y(yaw.round()), at + Vec3::Y * s * 0.5);
                map.prop(m::CRATE, mm, [1.0; 3], 0.0, at, s);
                map.add_solid(Aabb { min: vec3(p.x - s * 0.65, 0.0, p.y - s * 0.65), max: vec3(p.x + s * 0.65, s, p.y + s * 0.65) });
            } else if roll < 0.66 && !on_road {
                let mm = Mat4::from_rotation_translation(Quat::from_rotation_y((yaw / 1.57).round() * 1.5708), at);
                map.prop(m::DUMPSTER, mm, [1.0; 3], 0.0, at, 2.0);
                map.add_solid(Aabb { min: vec3(p.x - 1.45, 0.0, p.y - 1.45), max: vec3(p.x + 1.45, 1.35, p.y + 1.45) });
            } else if roll < 0.82 {
                let mm = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), at);
                map.prop(m::RUBBLE, mm, [1.0; 3], 0.0, at, 1.0);
            } else if roll < 0.9 {
                let mm = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), at);
                map.prop(m::TIRE, mm, [1.0; 3], 0.0, at, 0.6);
            } else {
                map.barrels.push(at);
            }
        }
        // Explosive barrels the player can exploit near the base perimeter.
        for k in 0..10 {
            let a = k as f32 / 10.0 * std::f32::consts::TAU + 0.3;
            let p = Vec2::new(a.cos(), a.sin()) * r.range(22.0, 28.0);
            if !map.is_blocked(p, 1.0) {
                map.barrels.push(vec3(p.x, 0.0, p.y));
            }
        }
        map
    }

    fn prop(&mut self, mesh: usize, mm: Mat4, tint: [f32; 3], glow: f32, center: Vec3, radius: f32) {
        self.props.push(Prop { mesh, m: mm, tint, glow, center, radius });
    }

    fn lamp(&mut self, at: Vec3, yaw: f32) {
        if self.is_blocked(Vec2::new(at.x, at.z), 0.5) {
            return;
        }
        let mm = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), at);
        self.prop(m::LAMP, mm, [1.0; 3], 6.0, at + Vec3::Y * 2.5, 6.0);
        let head = mm.transform_point3(vec3(0.0, 4.8, 0.72));
        self.lamps.push(head);
        self.add_solid(Aabb { min: at - vec3(0.15, 0.0, 0.15), max: at + vec3(0.15, 5.0, 0.15) });
    }

    /// A roofless ruined shell: wall runs with randomized heights and breaches.
    fn building(&mut self, c: Vec2, he: Vec2, tint: [f32; 3], r: &mut Rng) {
        let seg_len = 2.0;
        let thick = 0.5;
        let mut side = |map: &mut Map, start: Vec2, step: Vec2, count: usize| {
            let door = r.int(count.max(1));
            let mut h = r.range(1.5, 3.2);
            for i in 0..count {
                h = (h + r.range(-0.8, 0.8)).clamp(0.7, 3.6);
                // Deliberate breaches so that the horde and the player can path through.
                if i == door || i == (door + 1) % count || r.chance(0.13) {
                    if r.chance(0.7) {
                        let p = start + step * (i as f32 + 0.5);
                        let at = vec3(p.x, 0.0, p.y);
                        let yaw = r.range(0.0, 6.28);
                        map.prop(m::RUBBLE, Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), at), tint, 0.0, at, 1.0);
                    }
                    continue;
                }
                let p = start + step * (i as f32 + 0.5);
                let horiz = step.x.abs() > 0.0;
                let size = if horiz { vec3(seg_len, h, thick) } else { vec3(thick, h, seg_len) };
                let center = vec3(p.x, h * 0.5, p.y);
                let shade = r.range(0.85, 1.12);
                let col = [tint[0] * shade, tint[1] * shade, tint[2] * shade];
                map.prop(m::CUBE, Mat4::from_scale_rotation_translation(size, Quat::IDENTITY, center), col, 0.0, center, h.max(seg_len));
                map.add_solid(Aabb { min: center - size * 0.5, max: center + size * 0.5 });
            }
        };
        let nx = ((he.x * 2.0) / seg_len).floor() as usize;
        let nz = ((he.y * 2.0) / seg_len).floor() as usize;
        let (x0, x1, z0, z1) = (c.x - he.x, c.x + he.x, c.y - he.y, c.y + he.y);
        side(self, Vec2::new(x0, z0), Vec2::new(seg_len, 0.0), nx);
        side(self, Vec2::new(x0, z1), Vec2::new(seg_len, 0.0), nx);
        side(self, Vec2::new(x0, z0), Vec2::new(0.0, seg_len), nz);
        side(self, Vec2::new(x1, z0), Vec2::new(0.0, seg_len), nz);
    }

    /// Paint the ground: roads, paint lines, dirt, dead grass, concrete pads.
    pub fn bake_ground(&self, size: u32) -> Vec<u8> {
        let mut out = vec![0u8; (size * size * 4) as usize];
        let paint = |ci: usize, rows: usize, chunk: &mut [u8]| {
            for (i, px) in chunk.chunks_exact_mut(4).enumerate() {
                let x = i % size as usize;
                let y = ci * rows + i / size as usize;
                let wx = (x as f32 + 0.5) / size as f32 * 2.0 * HALF - HALF;
                let wz = (y as f32 + 0.5) / size as f32 * 2.0 * HALF - HALF;
                let (c, spec) = self.ground_color(wx, wz);
                px[0] = (c[0].clamp(0.0, 1.0) * 255.0) as u8;
                px[1] = (c[1].clamp(0.0, 1.0) * 255.0) as u8;
                px[2] = (c[2].clamp(0.0, 1.0) * 255.0) as u8;
                px[3] = (spec.clamp(0.0, 1.0) * 255.0) as u8;
            }
        };
        #[cfg(target_arch = "wasm32")]
        paint(0, size as usize, &mut out);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let rows = (size as usize / threads).max(1);
            let paint = &paint;
            std::thread::scope(|sc| {
                for (ci, chunk) in out.chunks_mut(rows * size as usize * 4).enumerate() {
                    sc.spawn(move || paint(ci, rows, chunk));
                }
            });
        }
        out
    }

    fn ground_color(&self, x: f32, z: f32) -> ([f32; 3], f32) {
        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| {
            let t = t.clamp(0.0, 1.0);
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
        };
        let ss = |a: f32, b: f32, v: f32| {
            let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let n1 = fbm(x * 0.06, z * 0.06, 4);
        let n2 = fbm(x * 0.33 + 40.0, z * 0.33 - 13.0, 3);
        let n3 = fbm(x * 1.1 - 5.0, z * 1.1 + 9.0, 2);

        // Natural ground: dirt with patches of dry grass.
        let dirt = lerp3([0.27, 0.22, 0.17], [0.20, 0.16, 0.13], n2);
        let grass = lerp3([0.27, 0.29, 0.14], [0.36, 0.33, 0.17], n3);
        let mut c = lerp3(dirt, grass, ss(0.45, 0.62, n1 + (n3 - 0.5) * 0.25));
        let mut spec = 0.0;

        // Roads are slightly wobbly at their edges, with cracks.
        let edge = 7.5 + (n2 - 0.5) * 1.6;
        let road_d = x.abs().min(z.abs());
        let road = 1.0 - ss(edge - 0.5, edge + 0.3, road_d);
        if road > 0.0 {
            let asphalt = lerp3([0.13, 0.13, 0.14], [0.18, 0.18, 0.18], n3);
            let crack = ss(0.035, 0.0, (fbm(x * 0.5 + 77.0, z * 0.5 + 21.0, 3) - 0.5).abs());
            let asphalt = lerp3(asphalt, [0.05, 0.05, 0.05], crack * 0.8);
            c = lerp3(c, asphalt, road);
            spec = road * 0.5 * (1.0 - crack);
            // Dashed centerline along each road, away from the intersection.
            let worn = ss(0.3, 0.55, n2);
            if z.abs() < 0.14 && x.abs() > 9.0 && (x * 0.25).rem_euclid(1.0) < 0.55 {
                c = lerp3(c, [0.55, 0.48, 0.22], worn);
            }
            if x.abs() < 0.14 && z.abs() > 9.0 && (z * 0.25).rem_euclid(1.0) < 0.55 {
                c = lerp3(c, [0.55, 0.48, 0.22], worn);
            }
            // Shoulder lines.
            for (d, other) in [(z, x), (x, z)] {
                if (d.abs() - 6.8).abs() < 0.09 && other.abs() > 8.0 {
                    c = lerp3(c, [0.42, 0.42, 0.40], worn);
                }
            }
        }

        // Concrete floors under the ruins.
        for (a, b) in &self.floors {
            if x > a.x && x < b.x && z > a.y && z < b.y {
                let tile = ((x * 0.5).rem_euclid(1.0) < 0.03) || ((z * 0.5).rem_euclid(1.0) < 0.03);
                let base = lerp3([0.25, 0.24, 0.22], [0.31, 0.29, 0.27], n3);
                c = if tile { lerp3(base, [0.1, 0.1, 0.1], 0.6) } else { base };
                spec = 0.15;
            }
        }

        // The base pad: stained concrete with hazard stripes around the reactor.
        let (ax, az) = (x.abs(), z.abs());
        let pad = ax.max(az);
        if pad < 15.5 {
            let base = lerp3([0.29, 0.29, 0.28], [0.36, 0.35, 0.33], n3);
            let seam = (x * 0.25).rem_euclid(1.0) < 0.012 || (z * 0.25).rem_euclid(1.0) < 0.012;
            c = if seam { lerp3(base, [0.08, 0.08, 0.08], 0.7) } else { base };
            spec = 0.12;
            if pad > 14.7 {
                let stripe = ((x + z) * 0.8).rem_euclid(1.0) < 0.5;
                let worn = ss(0.28, 0.5, n2);
                c = lerp3(c, if stripe { [0.72, 0.56, 0.1] } else { [0.05, 0.05, 0.05] }, worn);
            }
            let core = pad;
            if (core - 3.4).abs() < 0.12 {
                c = lerp3(c, [0.70, 0.56, 0.1], 0.9);
            }
        }
        (c, spec)
    }
}

fn hash2(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6B343) ^ (y as u32).wrapping_mul(0xD8163841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash2(ix, iy) + (hash2(ix + 1, iy) - hash2(ix, iy)) * sx;
    let b = hash2(ix, iy + 1) + (hash2(ix + 1, iy + 1) - hash2(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

fn fbm(x: f32, y: f32, oct: u32) -> f32 {
    let (mut v, mut a, mut f, mut t) = (0.0, 0.5, 1.0, 0.0);
    for _ in 0..oct {
        v += value_noise(x * f, y * f) * a;
        t += a;
        a *= 0.5;
        f *= 2.03;
    }
    v / t
}

/// Solid view combining the map with placed structures, used for ragdoll collisions.
pub struct WorldCollider<'a> {
    pub map: &'a Map,
    pub blocked_cells: &'a [bool],
}

impl Collider for WorldCollider<'_> {
    fn resolve(&self, p: Vec3, r: f32) -> Option<Vec3> {
        let mut out: Option<Vec3> = None;
        let (cx, cz) = Map::cell(p.x, p.z);
        for &i in &self.map.grid[cz * GRID_DIM + cx] {
            let a = &self.map.solids[i as usize];
            let q = out.unwrap_or(p);
            if q.y - r > a.max.y {
                continue;
            }
            if let Some(n) = push_out_circle(Vec2::new(q.x, q.z), r, Vec2::new(a.min.x, a.min.z), Vec2::new(a.max.x, a.max.z)) {
                // Resting on top of low objects is nicer than being ejected sideways.
                let up = a.max.y + r - q.y;
                let side = Vec2::new(q.x, q.z).distance(n);
                out = Some(if up < side && a.max.y < 1.6 {
                    vec3(q.x, a.max.y + r, q.z)
                } else {
                    vec3(n.x, q.y, n.y)
                });
            }
        }
        // Placed structures are 2×2 tiles, 2 m tall as far as bodies care.
        if p.y < 2.0 {
            let q = out.unwrap_or(p);
            let c = super::tile_of(Vec2::new(q.x, q.z));
            if let Some(i) = super::tile_index(c) {
                if self.blocked_cells[i] {
                    let min = super::tile_min(c);
                    if let Some(n) = push_out_circle(Vec2::new(q.x, q.z), r, min, min + Vec2::splat(super::TILE)) {
                        let up = 2.0 + r - q.y;
                        let side = Vec2::new(q.x, q.z).distance(n);
                        out = Some(if up < side { vec3(q.x, 2.0 + r, q.z) } else { vec3(n.x, q.y, n.y) });
                    }
                }
            }
        }
        out
    }
}
