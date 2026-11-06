//! An AI player. It drives the title-screen demo, the automated tests and the bench mode.
use super::data::*;
use super::player::PlayerInput;
use super::{tile_of, Phase, World};
use glam::{vec3, Vec2};

pub struct Bot {
    think_t: f32,
    orbit: f32,
    build_t: f32,
    next_ring: usize,
    fire_hold: f32,
    /// Which player this bot controls.
    pub idx: usize,
}

// Rings of buildings to place, from the inside out.
const RING: [(SKind, f32, f32); 20] = [
    (SKind::SentryMg, 8.0, 0.0),
    (SKind::SentryMg, 8.0, 3.14),
    (SKind::Floodlight, 6.0, 1.57),
    (SKind::SentryShotgun, 8.0, 1.57),
    (SKind::SentryShotgun, 8.0, 4.71),
    (SKind::AmmoDepot, 5.0, 0.78),
    (SKind::SentryMg, 11.0, 0.78),
    (SKind::SentryMg, 11.0, 2.35),
    (SKind::SentryFlame, 9.0, 3.92),
    (SKind::SentryFlame, 9.0, 5.49),
    (SKind::SentryTesla, 10.0, 0.4),
    (SKind::SentryTesla, 10.0, 3.5),
    (SKind::RepairBay, 6.0, 3.9),
    (SKind::SentryRocket, 7.0, 2.6),
    (SKind::SentryRocket, 7.0, 5.7),
    (SKind::SentryMini, 12.0, 1.2),
    (SKind::SentryMini, 12.0, 4.4),
    (SKind::SentryMortar, 6.0, 5.2),
    (SKind::SentryMortar, 6.0, 2.1),
    (SKind::SentryMini, 13.0, 0.0),
];

impl Bot {
    pub fn new() -> Self {
        Bot { think_t: 0.0, orbit: 0.4, build_t: 2.0, next_ring: 0, fire_hold: 0.0, idx: 0 }
    }

    pub fn think(&mut self, w: &mut World, dt: f32) -> PlayerInput {
        let mut input = PlayerInput::default();
        self.think_t -= dt;
        self.build_t -= dt;
        self.orbit += dt * 0.32;
        let me = w.players[self.idx].pos2();

        // Nearest threat within sight.
        let mut nearest: Option<(Vec2, f32)> = None;
        for z in &w.zombies {
            if z.spawn_t > 0.5 {
                continue;
            }
            let d = z.pos2().distance_squared(me);
            if nearest.is_none_or(|n| d < n.1) {
                nearest = Some((z.pos2(), d));
            }
        }

        let home = Vec2::new(self.orbit.cos(), self.orbit.sin()) * 6.5;
        let mut wish = home - me;
        if wish.length() < 1.0 {
            wish = Vec2::ZERO;
        }
        input.aim = w.players[self.idx].aim;
        if let Some((zp, d2)) = nearest {
            let d = d2.sqrt();
            input.aim = vec3(zp.x, 1.2, zp.y);
            if d < 7.0 {
                // Back away while still drifting around the base.
                let away = (me - zp).normalize_or_zero();
                wish = wish.normalize_or_zero() * 0.5 + away * 1.1 * (1.0 + (7.0 - d) * 0.25);
                if d < 2.5 {
                    input.dodge = true;
                    input.sprint = true;
                }
            }
            if d < 24.0 {
                self.fire_hold = 0.3;
            }
            if d < 14.0 && w.zombies.len() > 24 && w.players[self.idx].grenades > 0 && (w.time * 0.3).fract() < 0.01 {
                input.grenade = true;
            }
        } else {
            input.aim = vec3(me.x + self.orbit.cos() * 6.0, 1.2, me.y + self.orbit.sin() * 6.0);
        }
        self.fire_hold -= dt;
        input.fire = self.fire_hold > 0.0;
        input.move_dir = wish.clamp_length_max(1.0);

        let wi = w.players[self.idx].weapon;
        if w.players[self.idx].mag[wi] < WEAPONS[wi].mag / 4 && nearest.is_none_or(|n| n.1 > 36.0) {
            input.reload = true;
        }

        if self.think_t <= 0.0 {
            self.think_t = 1.0;
            self.manage_economy(w);
            input.select = self.best_weapon(w);
        }
        if self.build_t <= 0.0 {
            self.build_t = 0.8;
            self.build(w);
        }
        // Skip the rest of a break once the defences are funded.
        if let Phase::Build { timer } = w.phase {
            if timer > 10.0 && w.scrap < 150 {
                w.call_next_wave();
            }
        }
        input
    }

    fn manage_economy(&mut self, w: &mut World) {
        // Save for defences first, then the most expensive affordable gun.
        let mut best: Option<usize> = None;
        for (i, def) in WEAPONS.iter().enumerate() {
            if !w.players[self.idx].owned[i] && def.unlock_wave <= w.wave.max(1) && def.price + 200 <= w.scrap {
                best = Some(i);
            }
        }
        if let Some(i) = best {
            let _ = w.buy_weapon(self.idx, i);
        }
        let wi = w.players[self.idx].weapon;
        if w.players[self.idx].reserve[wi] == 0 {
            let _ = w.buy_ammo(self.idx, wi);
        }
        // Mend what the horde has chewed.
        let hurt: Vec<(i32, i32)> = w.structures.iter().filter(|s| s.hp < s.max_hp * 0.5).map(|s| s.tile).take(3).collect();
        for t in hurt {
            let _ = w.repair_at(t);
        }
    }

    fn best_weapon(&self, w: &World) -> Option<usize> {
        let mut best = None;
        for (i, def) in WEAPONS.iter().enumerate() {
            if w.players[self.idx].owned[i] && (def.reserve == u32::MAX || w.players[self.idx].reserve[i] > 0 || w.players[self.idx].mag[i] > 0) {
                best = Some(i);
            }
        }
        best
    }

    fn build(&mut self, w: &mut World) {
        if self.next_ring >= RING.len() {
            self.next_ring = 0;
        }
        let (kind, r, a) = RING[self.next_ring];
        let d = sdef(kind);
        if d.unlock_wave > w.wave.max(1) || w.scrap < d.cost + 60 {
            return;
        }
        let p = Vec2::new(a.cos(), a.sin()) * r;
        let t = tile_of(p);
        // Either it got built, or the tile is taken or blocked: move on regardless.
        let _ = w.try_build(kind, t);
        self.next_ring += 1;
    }
}
