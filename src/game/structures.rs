//! Base building and the automated defences.
use super::data::*;
use super::map::HALF;
use super::zombie::Zombie;
use super::{tile_center, tile_index, World, TILE};
use crate::audio::Sfx;
use crate::render::mesh as m;
use glam::{vec3, Vec2, Vec3};

pub struct Structure {
    pub kind: SKind,
    pub tile: (i32, i32),
    pub pos: Vec3,
    pub hp: f32,
    pub max_hp: f32,
    pub level: usize,
    pub yaw: f32,
    pub target: i32,
    pub target_id: u32,
    pub cooldown: f32,
    pub scan: f32,
    pub recoil: f32,
    pub flash: f32,
    pub spin: f32,
    pub spawn_anim: f32,
    pub invested: u32,
    pub tick: f32,
}

pub const CORE_TILES: [(i32, i32); 4] = [(39, 39), (40, 39), (39, 40), (40, 40)];

impl Structure {
    /// A copy for rebuilding the client's list, keeping per-structure animation state.
    pub fn clone_light(&self) -> Structure {
        Structure {
            kind: self.kind,
            tile: self.tile,
            pos: self.pos,
            hp: self.hp,
            max_hp: self.max_hp,
            level: self.level,
            yaw: self.yaw,
            target: -1,
            target_id: 0,
            cooldown: 0.0,
            scan: 0.0,
            recoil: self.recoil,
            flash: self.flash,
            spin: self.spin,
            spawn_anim: self.spawn_anim,
            invested: self.invested,
            tick: 0.0,
        }
    }
}

impl World {
    fn make_structure(&mut self, kind: SKind, tile: (i32, i32), pos: Vec3) -> Structure {
        let d = sdef(kind);
        Structure {
            kind,
            tile,
            pos,
            hp: d.hp,
            max_hp: d.hp,
            level: 0,
            yaw: self.rng.range(0.0, 6.28),
            target: -1,
            target_id: 0,
            cooldown: 0.0,
            scan: self.rng.range(0.0, 0.2),
            recoil: 0.0,
            flash: 0.0,
            spin: 0.0,
            spawn_anim: 1.0,
            invested: d.cost,
            tick: 0.0,
        }
    }

    pub fn make_structure_for_load(&mut self, kind: SKind, tile: (i32, i32)) -> Structure {
        self.make_structure(kind, tile, Vec3::ZERO)
    }

    pub fn add_structure_at_center(&mut self, kind: SKind) {
        let s = self.make_structure(kind, CORE_TILES[0], Vec3::ZERO);
        self.structures.push(s);
        self.rebuild_tiles();
    }

    /// Adds without charging. Returns false if the tile is taken.
    pub fn add_structure(&mut self, kind: SKind, tile: (i32, i32)) -> bool {
        let Some(i) = tile_index(tile) else { return false };
        if self.tiles[i] >= 0 {
            return false;
        }
        let c = tile_center(tile);
        let s = self.make_structure(kind, tile, vec3(c.x, 0.0, c.y));
        self.structures.push(s);
        self.rebuild_tiles();
        true
    }

    pub fn rebuild_tiles(&mut self) {
        self.tiles.iter_mut().for_each(|t| *t = -1);
        self.blocked_tiles.iter_mut().for_each(|t| *t = false);
        for (i, s) in self.structures.iter().enumerate() {
            let blocks = sdef(s.kind).blocks;
            if s.kind == SKind::Core {
                for t in CORE_TILES {
                    let idx = tile_index(t).unwrap();
                    self.tiles[idx] = i as i32;
                    self.blocked_tiles[idx] = true;
                }
            } else if let Some(idx) = tile_index(s.tile) {
                self.tiles[idx] = i as i32;
                self.blocked_tiles[idx] = blocks;
            }
        }
        self.nav_dirty = true;
    }

    pub fn structure_at(&self, tile: (i32, i32)) -> Option<usize> {
        tile_index(tile).and_then(|i| {
            let s = self.tiles[i];
            (s >= 0).then_some(s as usize)
        })
    }

    pub fn can_place(&self, kind: SKind, tile: (i32, i32)) -> Result<(), &'static str> {
        let Some(i) = tile_index(tile) else { return Err("OUT OF BOUNDS") };
        if self.tiles[i] >= 0 {
            return Err("OCCUPIED");
        }
        let c = tile_center(tile);
        if c.x.abs() > HALF - 4.0 || c.y.abs() > HALF - 4.0 {
            return Err("OUT OF BOUNDS");
        }
        if self.map.is_blocked(c, TILE * 0.5 - 0.15) {
            return Err("OBSTRUCTED");
        }
        if self.barrels.iter().any(|b| Vec2::new(b.pos.x, b.pos.z).distance_squared(c) < 1.8) {
            return Err("OBSTRUCTED");
        }
        if sdef(kind).unlock_wave > self.wave.max(1) {
            return Err("LOCKED");
        }
        Ok(())
    }

    pub fn try_build(&mut self, kind: SKind, tile: (i32, i32)) -> Result<(), &'static str> {
        self.can_place(kind, tile)?;
        let cost = sdef(kind).cost;
        if self.scrap < cost {
            return Err("NOT ENOUGH SCRAP");
        }
        // Don't let a solid wall drop on the player.
        if sdef(kind).blocks && kind != SKind::Gate {
            let c = tile_center(tile);
            for pl in &self.players {
                let d = (pl.pos2() - c).abs();
                if pl.connected && d.x < TILE * 0.5 + 0.35 && d.y < TILE * 0.5 + 0.35 {
                    return Err("SOMEONE IS STANDING THERE");
                }
            }
        }
        self.scrap -= cost;
        self.stats.built += 1;
        self.add_structure(kind, tile);
        let c = tile_center(tile);
        let at = vec3(c.x, 0.0, c.y);
        self.snd(Sfx::Build, at, 0.8);
        for _ in 0..6 {
            let d = self.rng.dir2();
            self.fx.smoke(at + vec3(d.x, 0.2, d.y) * 0.8, vec3(d.x, 0.4, d.y), 0.3, 0.6, 0.2, 0.35);
        }
        Ok(())
    }

    pub fn sell_at(&mut self, tile: (i32, i32)) -> bool {
        let Some(i) = self.structure_at(tile) else { return false };
        if self.structures[i].kind == SKind::Core {
            return false;
        }
        let s = &self.structures[i];
        let refund = ((s.invested as f32 * 0.6) * (s.hp / s.max_hp).max(0.25)) as u32;
        let at = s.pos;
        self.scrap += refund;
        self.structures.swap_remove(i);
        self.rebuild_tiles();
        self.snd(Sfx::Sell, at, 0.8);
        true
    }

    pub fn upgrade_cost(&self, i: usize) -> Option<u32> {
        let s = &self.structures[i];
        let d = sdef(s.kind);
        d.turret.as_ref()?;
        if s.level >= 2 {
            return None;
        }
        Some((d.cost as f32 * LEVEL_COST[s.level + 1]) as u32)
    }

    pub fn upgrade_at(&mut self, tile: (i32, i32)) -> Result<(), &'static str> {
        let Some(i) = self.structure_at(tile) else { return Err("NOTHING THERE") };
        let Some(cost) = self.upgrade_cost(i) else { return Err("CANNOT UPGRADE") };
        if self.scrap < cost {
            return Err("NOT ENOUGH SCRAP");
        }
        self.scrap -= cost;
        let s = &mut self.structures[i];
        s.level += 1;
        s.invested += cost;
        s.max_hp *= 1.35;
        s.hp = s.max_hp;
        let at = s.pos;
        self.snd(Sfx::Upgrade, at, 0.9);
        self.fx.light(at + Vec3::Y, 7.0, vec3(2.0, 6.0, 3.0), 0.3);
        Ok(())
    }

    pub fn repair_cost(&self, i: usize) -> u32 {
        let s = &self.structures[i];
        let missing = 1.0 - s.hp / s.max_hp;
        let base = if s.kind == SKind::Core { 900 } else { sdef(s.kind).cost };
        ((base as f32) * missing * 0.5).ceil() as u32
    }

    pub fn repair_at(&mut self, tile: (i32, i32)) -> Result<(), &'static str> {
        let Some(i) = self.structure_at(tile) else { return Err("NOTHING THERE") };
        let cost = self.repair_cost(i);
        if cost == 0 {
            return Err("NOT DAMAGED");
        }
        if self.scrap < cost {
            return Err("NOT ENOUGH SCRAP");
        }
        self.scrap -= cost;
        let s = &mut self.structures[i];
        s.hp = s.max_hp;
        let at = s.pos;
        self.snd(Sfx::Upgrade, at, 0.6);
        Ok(())
    }

    pub fn hurt_structure(&mut self, i: usize, amount: f32, from: Vec3) {
        let s = &mut self.structures[i];
        s.hp -= amount;
        s.flash = 1.0;
        let at = s.pos + Vec3::Y * 1.1;
        let kind = s.kind;
        if kind == SKind::Core {
            self.core_alert = 3.0;
        }
        if self.rng.chance(0.5) {
            self.snd(Sfx::WallHit, at, 0.5);
        }
        let dir = (from - at).normalize_or_zero();
        for _ in 0..3 {
            let v = (dir + self.rng.dir3()) * 4.0;
            self.fx.spark(at, v, [6.0, 4.0, 1.5], 0.2);
        }
    }

    pub fn remove_dead_structures(&mut self) {
        let mut changed = false;
        let mut i = 0;
        while i < self.structures.len() {
            if self.structures[i].hp > 0.0 {
                i += 1;
                continue;
            }
            let s = self.structures.swap_remove(i);
            changed = true;
            self.on_structure_destroyed(&s);
        }
        if changed {
            self.rebuild_tiles();
        }
    }

    fn on_structure_destroyed(&mut self, s: &Structure) {
        let at = s.pos + Vec3::Y * 0.6;
        self.snd(Sfx::WallBreak, at, 1.0);
        self.fx.decal(s.pos.x, s.pos.z, 1.6, [0.03, 0.03, 0.03, 0.8], 1.0);
        let wood = s.kind == SKind::WoodWall;
        let tint = if wood { [0.4, 0.27, 0.16] } else { [0.3, 0.32, 0.36] };
        for _ in 0..16 {
            let v = self.rng.dir3() * self.rng.range(3.0, 8.0) + Vec3::Y * 4.0;
            let sc = vec3(self.rng.range(0.2, 0.6), self.rng.range(0.1, 0.3), self.rng.range(0.1, 0.3));
            self.fx.debris(at, v, m::PLANK, sc, tint, 4.0, false);
        }
        for _ in 0..8 {
            let v = self.rng.dir3() + Vec3::Y;
            self.fx.smoke(at, v, 0.5, 1.4, 0.1, 0.5);
        }
        match s.kind {
            SKind::Core => {
                self.game_over = true;
                self.announce("THE REACTOR IS LOST", [1.0, 0.2, 0.1], 8.0, true);
                self.shake(1.0);
                for k in 0..6 {
                    let d = self.rng.dir2() * k as f32 * 1.5;
                    self.explosions.push(super::combat::Explosion { pos: vec3(d.x, 1.0, d.y), radius: 11.0, damage: 2000.0, from_enemy: true, force: 34.0 });
                }
            }
            SKind::SentryRocket | SKind::SentryMortar => {
                self.explosions.push(super::combat::Explosion { pos: at, radius: 4.0, damage: 120.0, from_enemy: false, force: 14.0 });
            }
            _ => {}
        }
    }

    pub fn update_structures(&mut self, dt: f32) {
        let n = self.structures.len();
        for i in 0..n {
            self.update_structure(i, dt);
        }
    }

    fn update_structure(&mut self, i: usize, dt: f32) {
        let time = self.time;
        let s = &mut self.structures[i];
        s.flash = (s.flash - dt * 4.0).max(0.0);
        s.recoil = (s.recoil - dt * 6.0).max(0.0);
        s.spawn_anim = (s.spawn_anim - dt * 3.0).max(0.0);
        s.cooldown -= dt;
        s.scan -= dt;
        s.tick -= dt;
        let kind = s.kind;
        let pos = s.pos;
        let tick = s.tick <= 0.0;
        if tick {
            s.tick = 0.25;
        }
        match kind {
            SKind::Spikes | SKind::Wire | SKind::Mine => {
                let r = if kind == SKind::Mine { 1.2 } else { TILE * 0.55 };
                let mut triggered = false;
                let mut wear = 0.0;
                let p2 = Vec2::new(pos.x, pos.z);
                let list = self.query_zombies(p2, r + 1.0);
                for &zi in &list {
                    let z = &mut self.zombies[zi as usize];
                    let d = (z.pos2() - p2).abs();
                    if d.x > r || d.y > r || z.spawn_t > 0.0 {
                        continue;
                    }
                    match kind {
                        SKind::Mine => triggered = true,
                        SKind::Spikes => {
                            if tick {
                                let at = z.pos + Vec3::Y * 0.3;
                                let dir = Vec3::Y;
                                self.damage_zombie(zi as usize, 26.0, at, dir, 0.0, 0.2, false);
                                wear += 2.0;
                            }
                        }
                        _ => {
                            z.frost = z.frost.max(0.9);
                            if tick {
                                let at = z.pos + Vec3::Y * 0.5;
                                self.damage_zombie(zi as usize, 9.0, at, Vec3::Y, 0.0, 0.1, false);
                                wear += 1.5;
                            }
                        }
                    }
                }
                self.give_back_query(list);
                if wear > 0.0 {
                    self.structures[i].hp -= wear;
                }
                if triggered {
                    self.structures[i].hp = 0.0;
                    self.explosions.push(super::combat::Explosion { pos: pos + Vec3::Y * 0.2, radius: 5.5, damage: 340.0, from_enemy: false, force: 24.0 });
                }
                return;
            }
            SKind::RepairBay => {
                if tick {
                    for o in &mut self.structures {
                        if o.pos.distance_squared(pos) < 100.0 && o.hp < o.max_hp {
                            o.hp = (o.hp + o.max_hp * 0.008 + 2.0).min(o.max_hp);
                        }
                    }
                    if ((time * 4.0) as i32) % 8 == 0 {
                        self.fx.light(pos + Vec3::Y, 4.0, vec3(0.5, 3.0, 1.5), 0.3);
                    }
                }
                return;
            }
            _ => {}
        }
        let Some(td) = sdef(kind).turret else { return };
        self.update_turret(i, dt, &td);
    }

    fn turret_target_valid(&self, s: &Structure, range: f32) -> bool {
        if s.target < 0 {
            return false;
        }
        match self.zombies.get(s.target as usize) {
            Some(z) => z.id == s.target_id && z.hp > 0.0 && z.pos.distance_squared(s.pos) < range * range,
            None => false,
        }
    }

    fn update_turret(&mut self, i: usize, dt: f32, td: &TurretDef) {
        let level = self.structures[i].level;
        let range = td.range * (1.0 + level as f32 * 0.08);
        let (pos, mut scan_now) = {
            let s = &self.structures[i];
            (s.pos, s.scan <= 0.0)
        };
        if !self.turret_target_valid(&self.structures[i], range) {
            self.structures[i].target = -1;
            scan_now = true;
        }
        if scan_now {
            self.structures[i].scan = 0.2;
            let eye = pos + Vec3::Y * 1.0;
            let mut best: Option<(usize, f32)> = None;
            let min2 = td.min_range * td.min_range;
            for (zi, z) in self.zombies.iter().enumerate() {
                if z.spawn_t > 0.3 {
                    continue;
                }
                let d2 = z.pos.distance_squared(pos);
                if d2 > range * range || d2 < min2 {
                    continue;
                }
                let threat = d2 - if z.kind == ZKind::Titan { 80.0 } else { 0.0 };
                if best.is_some_and(|b| threat >= b.1) {
                    continue;
                }
                // Line of sight against the static map only. Mortars lob over walls.
                if self.structures[i].kind != SKind::SentryMortar {
                    let d = z.pos + Vec3::Y * 1.0 - eye;
                    let l = d.length();
                    if self.map.raycast(eye, d / l.max(0.01), l).is_some() {
                        continue;
                    }
                }
                best = Some((zi, threat));
            }
            let s = &mut self.structures[i];
            match best {
                Some((zi, _)) => {
                    s.target = zi as i32;
                    s.target_id = self.zombies[zi].id;
                }
                None => s.target = -1,
            }
        }

        let target = self.structures[i].target;
        if target < 0 {
            let s = &mut self.structures[i];
            s.yaw += dt * 0.5; // Idle sweep.
            s.spin = (s.spin - dt).max(0.0);
            return;
        }
        let z: &Zombie = &self.zombies[target as usize];
        let (tpos, tvel, tscale) = (z.pos, z.vel, z.scale);
        // Lead a little.
        let aim_pos = tpos + vec3(tvel.x, 0.0, tvel.y) * 0.12;
        let to = aim_pos - pos;
        let want = to.x.atan2(to.z);
        let s = &mut self.structures[i];
        let mut diff = (want - s.yaw).rem_euclid(std::f32::consts::TAU);
        if diff > std::f32::consts::PI {
            diff -= std::f32::consts::TAU;
        }
        let turn = td.turn * dt;
        s.yaw += diff.clamp(-turn, turn);
        let aligned = diff.abs() < 0.14;
        if s.kind == SKind::SentryMini {
            s.spin = (s.spin + dt * 2.0).min(1.0);
        }
        if !aligned || s.cooldown > 0.0 || (s.kind == SKind::SentryMini && s.spin < 0.6) {
            return;
        }
        let rate = td.rate * LEVEL_RATE[level];
        s.cooldown = 1.0 / rate;
        s.recoil = 1.0;
        let kind = s.kind;
        let yaw = s.yaw;
        let dmg = td.damage * LEVEL_DMG[level];
        let dir = vec3(yaw.sin(), 0.0, yaw.cos());
        let right = vec3(dir.z, 0.0, -dir.x);
        let head_y = 1.05;
        let from = pos + Vec3::Y * head_y + dir * td.muzzle;
        let tracer = [8.0, 6.0, 2.5];
        match kind {
            SKind::SentryMg => {
                let side = if (self.time * rate) as i32 % 2 == 0 { 0.12 } else { -0.12 };
                let f = from + right * side;
                let d = super::player::rotate_y(dir, self.rng.range(-0.045, 0.045));
                self.fx.muzzle_flash(f, d, 0.26, vec3(1.0, 0.75, 0.4));
                self.hitscan(f, d, range + 3.0, dmg, 0, 1.2, 0.1, tracer, 0.02);
                self.snd(Sfx::Sentry, from, 0.45);
            }
            SKind::SentryMini => {
                let d = super::player::rotate_y(dir, self.rng.range(-0.07, 0.07));
                self.fx.muzzle_flash(from, d, 0.28, vec3(1.0, 0.75, 0.4));
                self.hitscan(from, d, range + 3.0, dmg, 0, 1.0, 0.2, tracer, 0.02);
                self.fx.casing(from - dir, right);
                self.snd(Sfx::Sentry, from, 0.45);
            }
            SKind::SentryShotgun => {
                self.fx.muzzle_flash(from, dir, 0.6, vec3(1.0, 0.7, 0.35));
                for _ in 0..8 {
                    let d = super::player::rotate_y(dir, self.rng.range(-0.2, 0.2));
                    self.hitscan(from, d, range + 1.0, dmg, 0, 3.0, 0.7, tracer, 0.022);
                }
                self.snd(Sfx::SentryHeavy, from, 0.7);
                self.shake(0.02);
            }
            SKind::SentryFlame => {
                self.flame_stream(from, dir, range, dmg, 0.5);
                if self.rng.chance(0.2) {
                    self.snd(Sfx::Flame, from, 0.4);
                }
            }
            SKind::SentryTesla => {
                let top = pos + Vec3::Y * 1.75;
                self.chain_from(top, target as usize, dmg, 4 + level as u32, [3.0, 7.0, 16.0]);
                self.snd(Sfx::Tesla, top, 0.75);
                self.fx.light(top, 9.0, vec3(3.0, 7.0, 16.0), 0.1);
            }
            SKind::SentryRocket => {
                self.fx.muzzle_flash(from, dir, 0.5, vec3(1.0, 0.65, 0.3));
                self.spawn_explosive(from, dir, 30.0, 4.5, dmg, false, false);
                self.snd(Sfx::Rocket, from, 0.7);
            }
            SKind::SentryMortar => {
                let up = (dir + Vec3::Y).normalize();
                self.fx.muzzle_flash(pos + Vec3::Y * 2.0 + dir * 0.55, up, 0.6, vec3(1.0, 0.7, 0.35));
                let spread = self.rng.dir2() * 1.3;
                let at = tpos + vec3(spread.x, 0.1, spread.y);
                self.spawn_shell(pos + Vec3::Y * 2.0 + dir * 0.55, at, 5.0 + 0.3 * tscale, dmg);
                self.snd(Sfx::Launcher, from, 0.8);
                self.shake(0.05);
            }
            _ => {}
        }
    }

}
