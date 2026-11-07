//! Translates the world into renderer instances, particles and lights.
use super::data::*;
use super::player::gun_mesh;
use super::structures::Structure;
use super::zombie::{style_colors, Zombie, STYLE_PLAYER};
use super::{tile_center, World, TILE};
use crate::ragdoll::*;
use crate::render::mesh as m;
use crate::render::{look_at_rh, perspective_rh, FrameData, Flashlight, Instance, PointLight};
use glam::{vec3, Mat4, Quat, Vec3};

pub struct Camera {
    pub focus: Vec3,
    pub zoom: f32,
    /// Extra zoom applied by events, eased toward 1. Below 1 pulls in close.
    pub punch: f32,
    pub eye: Vec3,
    pub view_proj: Mat4,
    pub inv: Mat4,
    /// Rotation around the player, in radians. Zero looks north, as the minimap does.
    pub yaw: f32,
    /// Easing back to zero after a double right click.
    pub yaw_home: bool,
}

const CAM_OFFSET: Vec3 = Vec3::new(0.0, 22.5, 16.5);

impl Camera {
    pub fn new() -> Self {
        Camera { focus: Vec3::ZERO, zoom: 1.0, punch: 1.0, eye: CAM_OFFSET, view_proj: Mat4::IDENTITY, inv: Mat4::IDENTITY, yaw: 0.0, yaw_home: false }
    }

    /// The straight line through a window position, as an eye position and a unit direction.
    pub fn cursor_ray(&self, mx: f32, my: f32, w: f32, h: f32) -> (Vec3, Vec3) {
        let ndc = vec3(mx / w * 2.0 - 1.0, 1.0 - my / h * 2.0, 0.5);
        (self.eye, (self.inv.project_point3(ndc) - self.eye).normalize_or(Vec3::NEG_Y))
    }

    pub fn update(&mut self, target: Vec3, dt: f32, aspect: f32, shake: f32, time: f32) {
        self.focus = self.focus.lerp(target, (dt * 7.0).min(1.0));
        if self.yaw_home {
            // The short way round, however many turns the player has wound up.
            let tau = std::f32::consts::TAU;
            self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
            self.yaw *= 1.0 - (dt * 10.0).min(1.0);
            if self.yaw.abs() < 0.002 {
                self.yaw = 0.0;
                self.yaw_home = false;
            }
        }
        let s = shake * shake;
        let off = vec3(
            ((time * 41.0).sin() + (time * 73.0).sin() * 0.5) * s * 1.1,
            ((time * 57.0).sin()) * s * 0.5,
            ((time * 47.0).cos() + (time * 91.0).sin() * 0.5) * s * 1.1,
        );
        let focus = self.focus + off;
        self.eye = focus + super::player::rotate_y(CAM_OFFSET, self.yaw) * self.zoom * self.punch;
        let view = look_at_rh(self.eye, focus, Vec3::Y);
        let proj = perspective_rh(38f32.to_radians(), aspect, 1.0, 180.0);
        self.view_proj = proj * view;
        self.inv = self.view_proj.inverse();
    }

    /// Project a window position onto the horizontal plane at height `y`.
    pub fn cursor_to_plane(&self, mx: f32, my: f32, w: f32, h: f32, y: f32) -> Vec3 {
        let ndc = vec3(mx / w * 2.0 - 1.0, 1.0 - my / h * 2.0, 0.5);
        let p = self.inv.project_point3(ndc);
        let d = p - self.eye;
        let t = if d.y.abs() > 1e-4 { (y - self.eye.y) / d.y } else { 1.0 };
        self.eye + d * t.max(0.0)
    }

    pub fn world_to_screen(&self, p: Vec3, w: f32, h: f32) -> Option<(f32, f32)> {
        let c = self.view_proj * p.extend(1.0);
        if c.w <= 0.0 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some(((n.x * 0.5 + 0.5) * w, (0.5 - n.y * 0.5) * h))
    }
}

#[inline]
fn bone_matrix(a: Vec3, b: Vec3, wx: f32, wz: f32, right: Vec3) -> Mat4 {
    let y = b - a;
    let l = y.length();
    let yn = if l > 1e-5 { y / l } else { Vec3::Y };
    let y = if l > 1e-5 { y } else { Vec3::Y * 0.01 };
    let x = (right - yn * right.dot(yn)).try_normalize().unwrap_or_else(|| yn.any_orthonormal_vector());
    let z = x.cross(yn);
    Mat4::from_cols((x * wx).extend(0.0), y.extend(0.0), (z * wz).extend(0.0), a.extend(1.0))
}

struct BodyLook {
    skin: [f32; 3],
    shirt: [f32; 3],
    pants: [f32; 3],
    head_mesh: usize,
    eye_glow: f32,
    girth: f32,
    belly: bool,
}

fn look_for(style: u32) -> BodyLook {
    if style == STYLE_PLAYER {
        return BodyLook {
            skin: [0.35, 0.36, 0.28],
            shirt: [0.21, 0.27, 0.18],
            pants: [0.13, 0.15, 0.12],
            head_mesh: m::HEAD_P,
            eye_glow: 4.0,
            girth: 1.0,
            belly: false,
        };
    }
    let (skin, shirt, pants) = style_colors(style);
    let kind = style >> 24;
    let mut l = BodyLook { skin, shirt, pants, head_mesh: if style & 1 == 0 { m::HEAD_Z } else { m::HEAD_Z2 }, eye_glow: 5.0, girth: 1.0, belly: false };
    match kind {
        1 => {
            // Runner: lean, bare armed.
            l.girth = 0.86;
            l.eye_glow = 9.0;
        }
        3 => {
            // Brute: slab of muscle.
            l.skin = [0.42, 0.34, 0.30];
            l.shirt = l.skin;
            l.pants = [0.1, 0.09, 0.08];
            l.girth = 1.45;
        }
        4 => {
            // Bloater: sickly and swollen.
            l.skin = [0.42, 0.50, 0.20];
            l.shirt = [0.46, 0.54, 0.22];
            l.belly = true;
            l.girth = 1.1;
        }
        5 => {
            // Spitter: acid stained.
            l.skin = [0.26, 0.50, 0.18];
            l.shirt = [0.18, 0.32, 0.12];
            l.eye_glow = 8.0;
        }
        6 => {
            // Screamer: bloodless and pale.
            l.skin = [0.72, 0.70, 0.68];
            l.shirt = [0.55, 0.52, 0.5];
            l.pants = [0.32, 0.3, 0.3];
            l.girth = 0.85;
            l.eye_glow = 11.0;
        }
        8 => {
            // A dead player, back on their feet: their own kit over rotting skin.
            l.skin = [0.36, 0.44, 0.26];
            l.shirt = [0.17, 0.22, 0.14];
            l.pants = [0.11, 0.13, 0.1];
            l.head_mesh = m::HEAD_P;
            l.eye_glow = 6.0;
        }
        7 => {
            // Titan.
            l.skin = [0.28, 0.18, 0.16];
            l.shirt = [0.3, 0.17, 0.15];
            l.pants = [0.12, 0.09, 0.08];
            l.girth = 1.3;
            l.head_mesh = m::BOSS_HEAD;
            l.eye_glow = 12.0;
        }
        _ => {}
    }
    l
}

#[inline]
fn tinted(c: [f32; 3], t: [f32; 3]) -> [f32; 3] {
    [c[0] * t[0], c[1] * t[1], c[2] * t[2]]
}

/// Draw a 16-joint body. Shared by living characters and ragdolls.
#[allow(clippy::too_many_arguments)]
fn draw_body(frame: &mut FrameData, pose: &Pose, enabled: u16, severed: u8, scale: f32, right_hint: Vec3, look: &BodyLook, tint: [f32; 3], glow_mul: f32) {
    let on = |j: usize| enabled & (1 << j) != 0;
    let s = scale;
    let g = look.girth;
    let (skin, shirt, pants) = (tinted(look.skin, tint), tinted(look.shirt, tint), tinted(look.pants, tint));
    let core_on = on(CHEST) && on(PELVIS) && on(NECK);
    let up = (pose[NECK] - pose[PELVIS]).try_normalize().unwrap_or(Vec3::Y);
    let right = (right_hint - up * right_hint.dot(up)).try_normalize().unwrap_or(Vec3::X);
    if core_on {
        let mm = bone_matrix(pose[PELVIS], pose[NECK], 0.44 * s * g, 0.27 * s * g, right);
        frame.instances[m::TORSO].push(Instance::new(&mm, shirt, 0.0));
        if look.belly {
            let c = pose[PELVIS].lerp(pose[CHEST], 0.7) + up.cross(right) * 0.08 * s;
            let mm = Mat4::from_scale_rotation_translation(Vec3::splat(0.7 * s), Quat::IDENTITY, c);
            frame.instances[m::BLOATER_BELLY].push(Instance::new(&mm, shirt, 1.5));
        }
        // Red wound caps on anything torn away.
        let wound = |j: Vec3, r: f32, frame: &mut FrameData| {
            let mm = Mat4::from_scale_rotation_translation(Vec3::splat(r), Quat::IDENTITY, j);
            frame.instances[m::SPHERE].push(Instance::new(&mm, [0.3, 0.02, 0.02], 0.0));
        };
        if severed & (1 << Limb::Head as u8) != 0 || !on(HEAD) {
            wound(pose[NECK], 0.16 * s, frame);
        }
        if severed & (1 << Limb::LArm as u8) != 0 || !on(L_SHO) {
            wound(pose[NECK] * 0.75 + pose[PELVIS] * 0.25 - right * 0.2 * s, 0.13 * s, frame);
        }
        if severed & (1 << Limb::RArm as u8) != 0 || !on(R_SHO) {
            wound(pose[NECK] * 0.75 + pose[PELVIS] * 0.25 + right * 0.2 * s, 0.13 * s, frame);
        }
        if severed & (1 << Limb::LLeg as u8) != 0 || !on(L_HIP) {
            wound(pose[PELVIS] - right * 0.1 * s, 0.13 * s, frame);
        }
        if severed & (1 << Limb::RLeg as u8) != 0 || !on(R_HIP) {
            wound(pose[PELVIS] + right * 0.1 * s, 0.13 * s, frame);
        }
    }
    if on(HEAD) {
        let attached = severed & (1 << Limb::Head as u8) == 0 && on(NECK);
        let head_up = if attached { (pose[HEAD] - pose[NECK]).try_normalize().unwrap_or(up) } else { up };
        let fwd = right.cross(head_up).try_normalize().unwrap_or(Vec3::Z);
        let r2 = head_up.cross(fwd).try_normalize().unwrap_or(right);
        let k = 0.27 * s * if look.head_mesh == m::BOSS_HEAD { 1.2 } else { 1.0 };
        let mm = Mat4::from_cols((r2 * k).extend(0.0), (head_up * k).extend(0.0), (fwd * k).extend(0.0), pose[HEAD].extend(1.0));
        frame.instances[look.head_mesh].push(Instance::new(&mm, skin, look.eye_glow * glow_mul));
        if attached {
            let mm = bone_matrix(pose[NECK] - up * 0.03, pose[HEAD], 0.12 * s, 0.12 * s, right);
            frame.instances[m::LIMB].push(Instance::new(&mm, skin, 0.0));
        }
    }
    let limb = |a: usize, b: usize, w: f32, col: [f32; 3], frame: &mut FrameData| {
        if on(a) && on(b) {
            let mm = bone_matrix(pose[a], pose[b], w * s * g, w * s * g, right);
            frame.instances[m::LIMB].push(Instance::new(&mm, col, 0.0));
        }
    };
    limb(L_SHO, L_ELB, 0.115, shirt, frame);
    limb(R_SHO, R_ELB, 0.115, shirt, frame);
    limb(L_ELB, L_HAND, 0.09, skin, frame);
    limb(R_ELB, R_HAND, 0.09, skin, frame);
    limb(L_HIP, L_KNEE, 0.15, pants, frame);
    limb(R_HIP, R_KNEE, 0.15, pants, frame);
    limb(L_KNEE, L_FOOT, 0.11, pants, frame);
    limb(R_KNEE, R_FOOT, 0.11, pants, frame);
    let cap = |j: usize, sz: f32, col: [f32; 3], mesh: usize, along: Vec3, frame: &mut FrameData| {
        if on(j) {
            let mm = bone_matrix(pose[j], pose[j] + along * sz * s, sz * 0.8 * s, sz * 0.8 * s, right);
            frame.instances[mesh].push(Instance::new(&mm, col, 0.0));
        }
    };
    if on(L_ELB) && on(L_HAND) {
        let d = (pose[L_HAND] - pose[L_ELB]).normalize_or(Vec3::NEG_Y);
        cap(L_HAND, 0.12, skin, m::HAND, d, frame);
    }
    if on(R_ELB) && on(R_HAND) {
        let d = (pose[R_HAND] - pose[R_ELB]).normalize_or(Vec3::NEG_Y);
        cap(R_HAND, 0.12, skin, m::HAND, d, frame);
    }
    cap(L_FOOT, 0.13, [0.07, 0.06, 0.05], m::CUBE, Vec3::Y, frame);
    cap(R_FOOT, 0.13, [0.07, 0.06, 0.05], m::CUBE, Vec3::Y, frame);
}

fn draw_zombie(z: &Zombie, time: f32, frame: &mut FrameData) {
    let pose = z.pose(time);
    let look = look_for(z.style);
    let mut tint = [1.0f32; 3];
    if z.burn > 0.0 {
        let k = (z.burn * 0.3).min(0.6);
        tint = [1.0 - k * 0.7, 1.0 - k * 0.8, 1.0 - k * 0.85];
    }
    if z.frost > 0.0 {
        let k = (z.frost * 0.55).min(1.0);
        tint = [tint[0] * (1.0 - 0.55 * k) + 0.2 * k, tint[1] * (1.0 - 0.1 * k) + 0.45 * k, tint[2] * (1.0 + 0.5 * k) + 0.8 * k];
    }
    if z.rage > 0.0 {
        tint[0] *= 1.35;
        tint[1] *= 0.8;
        tint[2] *= 0.8;
    }
    // Wounds redden the character as HP falls.
    let hurt = 1.0 - (z.hp / z.max_hp).clamp(0.0, 1.0);
    tint[0] *= 1.0 + hurt * 0.25;
    tint[1] *= 1.0 - hurt * 0.45;
    tint[2] *= 1.0 - hurt * 0.45;
    if z.flash > 0.0 {
        let f = 1.0 + z.flash * 4.0;
        tint = [tint[0] * f, tint[1] * (1.0 + z.flash * 2.5), tint[2] * (1.0 + z.flash * 2.5)];
    }
    let right = vec3(z.facing.y, 0.0, -z.facing.x);
    let enabled = ALL_JOINTS & !z.missing;
    draw_body(frame, &pose, enabled, 0, z.scale, right, &look, tint, 1.0);
}

fn draw_structure(w: &World, s: &Structure, frame: &mut FrameData) {
    let d = sdef(s.kind);
    let hpk = (s.hp / s.max_hp).clamp(0.0, 1.0);
    let dmg = 0.45 + 0.55 * hpk;
    let f = 1.0 + s.flash * 1.4;
    let tint = [dmg * f, dmg * f, dmg * f];
    // Grow out of the ground on placement.
    let sy = 1.0 - s.spawn_anim * 0.9;
    let mut yaw = 0.0;
    if matches!(s.kind, SKind::WoodWall | SKind::SteelWall | SKind::Gate) {
        // Walls orient along their neighbours so runs of tiles line up.
        let east = w.structure_at((s.tile.0 + 1, s.tile.1)).is_some_and(|i| is_wall(w.structures[i].kind));
        let west = w.structure_at((s.tile.0 - 1, s.tile.1)).is_some_and(|i| is_wall(w.structures[i].kind));
        let north = w.structure_at((s.tile.0, s.tile.1 + 1)).is_some_and(|i| is_wall(w.structures[i].kind));
        let south = w.structure_at((s.tile.0, s.tile.1 - 1)).is_some_and(|i| is_wall(w.structures[i].kind));
        if (east || west) && !(north || south) {
            yaw = std::f32::consts::FRAC_PI_2;
        }
    }
    let mm = Mat4::from_scale_rotation_translation(vec3(1.0, sy, 1.0), Quat::from_rotation_y(yaw), s.pos);
    let glow = match s.kind {
        SKind::Core => 1.6 + (w.time * 2.0).sin() * 0.4 + if w.core_alert > 0.0 { (w.time * 14.0).sin().abs() * 2.5 } else { 0.0 },
        SKind::Floodlight => 9.0,
        SKind::RepairBay => 1.2 + (w.time * 3.0).sin() * 0.4,
        SKind::Mine => if (w.time * 3.0).fract() < 0.5 { 6.0 } else { 0.5 },
        SKind::AmmoDepot => 2.0,
        SKind::Gate => 3.0,
        _ => 2.0,
    };
    frame.instances[d.mesh].push(Instance::new(&mm, tint, glow));
    if s.kind == SKind::Core {
        let rot = Quat::from_rotation_y(w.time * 0.8);
        for (y, sc) in [(1.2, 1.0), (2.2, 0.85)] {
            let at = s.pos + Vec3::Y * (y + (w.time * 1.3 + y).sin() * 0.25);
            let mm = Mat4::from_scale_rotation_translation(Vec3::splat(sc), rot, at);
            frame.instances[m::CORE_RING].push(Instance::new(&mm, tint, 0.0));
        }
    }
    if let Some(head) = d.head {
        let q = Quat::from_rotation_y(s.yaw);
        let back = q * Vec3::Z * (-s.recoil * 0.1);
        let at = s.pos + Vec3::Y * (1.05 * sy) + back;
        let scale = 1.0 + s.level as f32 * 0.1;
        let mm = Mat4::from_scale_rotation_translation(Vec3::splat(scale), q, at);
        frame.instances[head].push(Instance::new(&mm, tint, 5.0));
        // Pips show the upgrade level.
        for l in 0..s.level {
            let p = s.pos + vec3(-0.25 + 0.5 * l as f32, 0.28, 0.75);
            let mm = Mat4::from_scale_rotation_translation(vec3(0.2, 0.05, 0.12), Quat::IDENTITY, p);
            frame.instances[m::CUBE].push(Instance::new(&mm, [1.0, 0.8, 0.2], 0.0));
        }
    }
}

pub fn is_wall(k: SKind) -> bool {
    matches!(k, SKind::WoodWall | SKind::SteelWall | SKind::Gate)
}

/// Build the frame's scene. Decals are moved out of the world's queue here.
pub fn draw_world(w: &mut World, cam: &Camera, frame: &mut FrameData, ghost: Option<(SKind, (i32, i32), bool)>) {
    let focus = cam.focus;
    let range = 52.0 * cam.zoom.max(1.0);
    let range2 = range * range;
    frame.view_proj = cam.view_proj;
    frame.cam_pos = cam.eye;
    frame.focus = focus;
    frame.time = w.time;

    // Time of day.
    let n = w.night.clamp(0.0, 1.0);
    let mix = |a: Vec3, b: Vec3| a.lerp(b, n);
    frame.sun_dir = mix(vec3(0.62, 0.52, 0.3), vec3(-0.35, 0.82, 0.42)).normalize();
    frame.sun_color = mix(vec3(1.75, 1.05, 0.62), vec3(0.20, 0.28, 0.46));
    frame.amb_sky = mix(vec3(0.34, 0.33, 0.40), vec3(0.075, 0.10, 0.17));
    frame.amb_ground = mix(vec3(0.16, 0.13, 0.11), vec3(0.03, 0.035, 0.05));
    frame.fog = mix(vec3(0.16, 0.10, 0.09), vec3(0.008, 0.012, 0.022));
    frame.exposure = 1.0;
    frame.bloom = 0.11;
    frame.vignette = 2.1 - n * 0.4;
    // Death closes in: the world dims and the edges darken over the first couple of seconds.
    if !w.me().alive {
        let k = (w.me().death_t * 0.6).min(1.0);
        frame.exposure = 1.0 - 0.3 * k;
        frame.vignette += 2.4 * k;
    }
    let p = w.me();
    let hp_k: f32 = (p.hp / p.max_hp).clamp(0.0, 1.0);
    frame.damage = (p.hurt * 0.55 + (1.0 - hp_k).powi(2) * 0.3 + if p.alive { 0.0 } else { 0.25 }).min(0.75);
    frame.chroma = 0.004 + w.trauma * 0.016 + p.hurt * 0.012;
    frame.desaturate = if w.game_over { (w.game_over_timer * 0.3).min(0.85) } else if !p.alive { (0.3 + p.death_t * 0.35).min(0.8) } else { (1.0 - hp_k) * 0.3 };

    // Static props.
    for pr in &w.map.props {
        if (pr.center - focus).length_squared() < (range + pr.radius) * (range + pr.radius) {
            frame.instances[pr.mesh].push(Instance::new(&pr.m, pr.tint, pr.glow));
        }
    }
    // Lamps. Those that are off-screen are skipped by the same range check.
    for &l in &w.map.lamps {
        if (l - focus).length_squared() < range2 {
            frame.lights.push(PointLight { pos: l.to_array(), radius: 15.0, color: [3.6, 2.5, 1.5], _pad: 0.0 });
        }
    }

    for b in &w.barrels {
        if (b.pos - focus).length_squared() < range2 {
            let mm = Mat4::from_translation(b.pos);
            let t = if b.burn > 0.0 { [1.7, 1.0, 0.7] } else { [1.0, 1.0, 1.0] };
            frame.instances[m::BARREL].push(Instance::new(&mm, t, 0.0));
        }
    }

    // Structures and their lights.
    for s in &w.structures {
        if (s.pos - focus).length_squared() > range2 {
            continue;
        }
        draw_structure(w, s, frame);
        match s.kind {
            SKind::Core => {
                let pulse = 1.0 + (w.time * 2.0).sin() * 0.2;
                let alert = if w.core_alert > 0.0 { (w.time * 14.0).sin().abs() } else { 0.0 };
                let col = vec3(0.5, 1.7, 2.3) * pulse + vec3(6.0, -1.0, -1.5) * alert;
                frame.lights.push(PointLight { pos: (s.pos + Vec3::Y * 3.0).to_array(), radius: 15.0, color: col.max(Vec3::ZERO).to_array(), _pad: 0.0 });
            }
            SKind::Floodlight => {
                frame.lights.push(PointLight { pos: (s.pos + Vec3::Y * 2.8).to_array(), radius: 21.0, color: [7.0, 6.2, 5.0], _pad: 0.0 });
            }
            SKind::RepairBay => {
                frame.lights.push(PointLight { pos: (s.pos + Vec3::Y * 1.2).to_array(), radius: 6.0, color: [0.15, 0.8, 0.4], _pad: 0.0 });
            }
            _ => {}
        }
    }
    if let Some((kind, tile, ok)) = ghost {
        let c = tile_center(tile);
        let d = sdef(kind);
        let col = if ok { [0.25, 1.0, 0.45] } else { [1.0, 0.18, 0.12] };
        let mm = Mat4::from_translation(vec3(c.x, 0.02, c.y));
        frame.ghosts.push((d.mesh, Instance::new(&mm, col, 0.0)));
        if let Some(h) = d.head {
            let mm = Mat4::from_translation(vec3(c.x, 1.05, c.y));
            frame.ghosts.push((h, Instance::new(&mm, col, 0.0)));
        }
        // Tile outline.
        for (ox, oz, sx, sz) in [(0.0, -1.0, TILE, 0.06), (0.0, 1.0, TILE, 0.06), (-1.0, 0.0, 0.06, TILE), (1.0, 0.0, 0.06, TILE)] {
            let mm = Mat4::from_scale_rotation_translation(vec3(sx, 0.04, sz), Quat::IDENTITY, vec3(c.x + ox, 0.04, c.y + oz));
            frame.ghosts.push((m::CUBE, Instance::new(&mm, [col[0] * 3.0, col[1] * 3.0, col[2] * 3.0], 0.0)));
        }
        // Range ring for turrets.
        if let Some(t) = d.turret {
            let steps = 48;
            for k in 0..steps {
                if frame.ghosts.len() >= 200 {
                    break;
                }
                let a = k as f32 / steps as f32 * std::f32::consts::TAU;
                let at = vec3(c.x + a.cos() * t.range, 0.05, c.y + a.sin() * t.range);
                let mm = Mat4::from_scale_rotation_translation(vec3(0.5, 0.03, 0.09), Quat::from_rotation_y(-a + std::f32::consts::FRAC_PI_2), at);
                frame.ghosts.push((m::CUBE, Instance::new(&mm, [col[0] * 1.5, col[1] * 1.5, col[2] * 1.5], 0.0)));
            }
        }
    }

    // Living zombies.
    let time = w.time;
    for z in &w.zombies {
        if (z.pos - focus).length_squared() < range2 {
            draw_zombie(z, time, frame);
        }
    }

    // Corpses, sinking away near the end of their life.
    for b in &w.ragdolls.bodies {
        let c = b.pos[PELVIS];
        if (c - focus).length_squared() > range2 {
            continue;
        }
        let look = look_for(b.style);
        let remaining = b.life - b.age;
        let mut tint = if b.char_amount > 0.0 { [0.25, 0.22, 0.2] } else { [0.85, 0.82, 0.82] };
        let glow = if b.style == STYLE_PLAYER { 2.0 } else { (1.0 - b.age * 0.8).max(0.0) };
        if remaining < 2.0 {
            let k = remaining / 2.0;
            tint = [tint[0] * k, tint[1] * k, tint[2] * k];
        }
        if remaining < 2.0 {
            let drop = (2.0 - remaining) * 0.28;
            let mut pose = b.pos;
            for j in &mut pose {
                j.y -= drop;
            }
            draw_body(frame, &pose, b.enabled, b.severed, b.scale, b.right_hint, &look, tint, glow);
        } else {
            draw_body(frame, &b.pos, b.enabled, b.severed, b.scale, b.right_hint, &look, tint, glow);
        }
    }

    // Players. The flashlight's shadow map exists once, so only the local one casts.
    for (pi, p) in w.players.iter().enumerate() {
        // While down, the ragdoll in the corpse list is the character.
        if !p.alive || !p.connected || p.down_t > 0.0 {
            continue;
        }
        let pose = p.pose();
        let right = vec3(p.facing.y, 0.0, -p.facing.x);
        let look = look_for(STYLE_PLAYER);
        let flash = if p.roll_t > 0.0 { [1.2, 1.2, 1.3] } else { [1.0, 1.0, 1.0] };
        draw_body(frame, &pose, ALL_JOINTS, 0, 1.0, right, &look, flash, 1.0);
        let fwd = vec3(p.facing.x, 0.0, p.facing.y);
        let grip = pose[R_HAND];
        // Guns are authored with the barrel along +Z.
        let barrel = p.aim_dir();
        let up = barrel.cross(right).normalize_or(Vec3::Y);
        let mm = Mat4::from_cols(right.extend(0.0), up.extend(0.0), barrel.extend(0.0), grip.extend(1.0));
        frame.instances[gun_mesh(p.weapon)].push(Instance::new(&mm, [1.0; 3], 4.0));
        // Laser sight.
        let from = p.muzzle();
        let shot = p.shot_dir();
        let mut laser = w.map.raycast(from, shot, 34.0).unwrap_or(34.0);
        if shot.y < -1e-4 {
            laser = laser.min(-from.y / shot.y);
        }
        for z in &w.zombies {
            if let Some((t, _)) = super::combat::ray_zombie(z, w.time, from, shot, true) {
                if t > 0.0 && t < laser {
                    laser = t;
                }
            }
        }
        frame.particles.push(crate::render::Particle {
            pos: (from + shot * laser).to_array(),
            size: 0.012,
            stretch: (shot * laser).to_array(),
            shape: 0.0,
            color: [2.2, 0.15, 0.12, 0.0],
        });
        frame.particles.push(crate::render::Particle {
            pos: (from + shot * laser).to_array(),
            size: 0.09,
            stretch: [0.0; 3],
            shape: 0.0,
            color: [4.0, 0.3, 0.2, 0.0],
        });
        // The flashlight comes on with the night.
        if pi == w.local && n > 0.4 && p.flashlight {
            let strength = ((n - 0.4) / 0.35).clamp(0.0, 1.0);
            let dir = (fwd + Vec3::NEG_Y * 0.22).normalize();
            frame.flashlight = Some(Flashlight {
                pos: p.pos + Vec3::Y * 1.65 + fwd * 0.15,
                dir,
                color: vec3(13.0, 11.5, 8.5) * strength,
                range: 30.0,
                cos_outer: 33f32.to_radians().cos(),
                cos_inner: 15f32.to_radians().cos(),
            });
        }
        // A soft player light so you are never lost in the dark.
        frame.lights.push(PointLight { pos: (p.pos + Vec3::Y * 2.2).to_array(), radius: 8.5, color: vec3(0.7, 0.8, 1.0).to_array(), _pad: 0.0 });
    }

    // Projectiles.
    for pr in &w.projectiles {
        use super::combat::ProjKind as K;
        let dir = pr.vel.normalize_or(Vec3::Z);
        let q = Quat::from_rotation_arc(Vec3::Z, dir);
        match pr.kind {
            K::Rocket => {
                let mm = Mat4::from_rotation_translation(q, pr.pos);
                frame.instances[m::ROCKET].push(Instance::new(&mm, [1.0; 3], 0.0));
            }
            K::Grenade | K::Shell => {
                let mm = Mat4::from_scale_rotation_translation(Vec3::splat(0.18), Quat::from_rotation_x(pr.spin), pr.pos);
                frame.instances[m::GRENADE].push(Instance::new(&mm, [1.0; 3], 0.0));
            }
            K::Saw => {
                let mm = Mat4::from_rotation_translation(Quat::from_rotation_y(pr.spin * 2.0), pr.pos);
                frame.instances[m::SAWBLADE].push(Instance::new(&mm, [1.4; 3], 0.0));
            }
            K::Spit => {}
        }
    }

    // Pickups bob and spin.
    for pk in &w.pickups {
        if (pk.pos - focus).length_squared() > range2 {
            continue;
        }
        let q = Quat::from_rotation_y(pk.spin);
        let (mesh, s, glow) = match pk.kind {
            0 => (m::PICK_SCRAP, 0.2, 2.6),
            1 => (m::PICK_AMMO, 0.5, 5.0),
            _ => (m::PICK_MED, 0.55, 5.0),
        };
        let mm = Mat4::from_scale_rotation_translation(Vec3::splat(s), q, pk.pos);
        frame.instances[mesh].push(Instance::new(&mm, [1.0; 3], glow));
    }

    w.fx.emit_debris(&mut frame.instances, focus, range);
    w.fx.emit_particles(&mut frame.particles, focus, range);
    w.fx.emit_lights(&mut frame.lights);
    frame.decals.append(&mut w.fx.decals);
    if w.fx.clear_decals {
        frame.clear_decals = true;
        w.fx.clear_decals = false;
    }
}
