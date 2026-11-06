//! Immediate-mode UI: HUD, armory, build bar, and menus.
use super::app::{Action, App, Page, RawInput, Screen, Session};
use super::data::*;
use super::map::HALF;
use super::net::{self, Command, RosterEntry};
use super::save::Slot;
use super::{tile_center, Phase, World};
use crate::audio::Sfx;
use crate::render::font;
use crate::render::{FrameData, UiVertex};
use glam::{vec3, Vec3};

pub struct Ui<'a> {
    pub out: &'a mut Vec<UiVertex>,
    pub w: f32,
    pub h: f32,
    pub s: f32,
    pub mx: f32,
    pub my: f32,
    pub click: bool,
    pub over: bool,
    pub typed: Vec<char>,
    pub backspace: bool,
    pub time: f32,
}

const VERSION: &str = concat!("V", env!("CARGO_PKG_VERSION"));
pub type C = [f32; 4];
const WHITE: C = [0.93, 0.92, 0.88, 1.0];
const DIM: C = [0.60, 0.60, 0.58, 1.0];
const ACCENT: C = [1.0, 0.72, 0.2, 1.0];
const RED: C = [0.95, 0.2, 0.14, 1.0];
const GREEN: C = [0.38, 0.92, 0.45, 1.0];
const PANEL: C = [0.045, 0.05, 0.06, 0.82];
const PANEL_HI: C = [0.16, 0.14, 0.09, 0.92];
const BLUE: C = [0.35, 0.8, 1.0, 1.0];

impl Ui<'_> {
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: C) {
        let uv = font::solid_uv();
        let v = |px, py| UiVertex { pos: [px, py], uv, color: c };
        self.out.extend_from_slice(&[v(x, y), v(x + w, y), v(x, y + h), v(x, y + h), v(x + w, y), v(x + w, y + h)]);
    }

    pub fn frame_rect(&mut self, x: f32, y: f32, w: f32, h: f32, t: f32, c: C) {
        self.rect(x, y, w, t, c);
        self.rect(x, y + h - t, w, t, c);
        self.rect(x, y, t, h, c);
        self.rect(x + w - t, y, t, h, c);
    }

    pub fn text_w(&self, t: &str, scale: f32) -> f32 {
        t.len() as f32 * font::CELL_W as f32 * scale.round().max(1.0)
    }

    pub fn text(&mut self, x: f32, y: f32, scale: f32, t: &str, c: C) {
        // Integer pixel snapping keeps the bitmap font crisp.
        let scale = scale.round().max(1.0);
        let (x0, y0) = (x.round(), y.round());
        let (cw, ch) = (5.0 * scale, 7.0 * scale);
        let mut px = x0;
        for ch_ in t.chars() {
            if ch_ != ' ' {
                let (a, b) = font::glyph_uv(ch_);
                // The 5x7 glyph occupies the upper-left of its atlas cell.
                let u1 = a[0] + (b[0] - a[0]) * 5.0 / (font::CELL_W - 1) as f32;
                let v1 = a[1] + (b[1] - a[1]) * 7.0 / (font::CELL_H - 1) as f32;
                let vert = |vx, vy, u, v| UiVertex { pos: [vx, vy], uv: [u, v], color: c };
                self.out.extend_from_slice(&[
                    vert(px, y0, a[0], a[1]),
                    vert(px + cw, y0, u1, a[1]),
                    vert(px, y0 + ch, a[0], v1),
                    vert(px, y0 + ch, a[0], v1),
                    vert(px + cw, y0, u1, a[1]),
                    vert(px + cw, y0 + ch, u1, v1),
                ]);
            }
            px += font::CELL_W as f32 * scale;
        }
    }

    pub fn text_shadow(&mut self, x: f32, y: f32, scale: f32, t: &str, c: C) {
        let o = scale.round().max(1.0);
        self.text(x + o, y + o, scale, t, [0.0, 0.0, 0.0, c[3] * 0.8]);
        self.text(x, y, scale, t, c);
    }

    pub fn text_center(&mut self, cx: f32, y: f32, scale: f32, t: &str, c: C) {
        let sc = scale.round().max(1.0);
        let w = self.text_w(t, sc);
        self.text_shadow(cx - w * 0.5, y, sc, t, c);
    }

    pub fn hovered(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.mx >= x && self.mx < x + w && self.my >= y && self.my < y + h
    }

    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if self.hovered(x, y, w, h) {
            self.over = true;
        }
        self.rect(x, y, w, h, PANEL);
        self.frame_rect(x, y, w, h, 1.0, [0.3, 0.28, 0.22, 0.6]);
    }

    pub fn button(&mut self, x: f32, y: f32, w: f32, h: f32, label: &str, scale: f32, enabled: bool) -> bool {
        let hover = self.hovered(x, y, w, h);
        if hover {
            self.over = true;
        }
        self.rect(x, y, w, h, if hover && enabled { PANEL_HI } else { PANEL });
        self.frame_rect(x, y, w, h, 2.0, if hover && enabled { ACCENT } else { [0.4, 0.38, 0.3, 0.8] });
        let sc = scale.round().max(1.0);
        let tw = self.text_w(label, sc);
        let color = if enabled { WHITE } else { DIM };
        self.text(x + (w - tw) * 0.5, y + (h - 7.0 * sc) * 0.5, sc, label, color);
        hover && self.click && enabled
    }

    pub fn bar(&mut self, x: f32, y: f32, w: f32, h: f32, k: f32, c: C) {
        self.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.65]);
        let k = k.clamp(0.0, 1.0);
        self.rect(x + 1.0, y + 1.0, (w - 2.0) * k, h - 2.0, c);
    }
}

fn hp_color(k: f32) -> C {
    if k > 0.55 {
        GREEN
    } else if k > 0.25 {
        ACCENT
    } else {
        RED
    }
}

pub fn draw(app: &mut App, frame: &mut FrameData, raw: &RawInput, size: (u32, u32)) {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let s = ((h / 1080.0).min(w / 1600.0) * 2.0).round().max(1.0) * 0.5 + 0.5;
    let click = raw.lmb_pressed;
    let typed = raw.typed.clone();
    let backspace = raw.hit(winit::keyboard::KeyCode::Backspace);
    let time = app.menu_t;
    let mut ui = Ui { out: &mut frame.ui, w, h, s, mx: raw.mouse.0, my: raw.mouse.1, click, over: false, typed, backspace, time };
    match app.screen {
        Screen::Title => title(app, &mut ui),
        Screen::Playing => {
            ingame(app, &mut ui);
            if app.help_open {
                help(app, &mut ui);
            } else if app.armory_open {
                armory(app, &mut ui);
            } else if app.build_sel.is_some() {
                build_bar(app, &mut ui);
            } else if app.panel_open {
                server_panel(app, &mut ui);
            }
            cursor(app, &mut ui);
        }
        Screen::Paused => {
            ingame(app, &mut ui);
            pause(app, &mut ui);
        }
        Screen::GameOver => {
            ingame(app, &mut ui);
            game_over(app, &mut ui);
        }
    }
    if app.show_fps {
        let t = format!("{:>4.0} FPS {:>5.1} MS  Z:{} R:{} P:{}", app.fps, app.frame_ms, app.world.zombies.len(), app.world.ragdolls.bodies.len(), app.world.fx.p.len());
        ui.text_shadow(8.0, h - 18.0, 1.0, &t, WHITE);
    }
    app.ui_over = ui.over;
}

fn cursor(app: &App, ui: &mut Ui) {
    let (mx, my) = (ui.mx, ui.my);
    if app.armory_open || app.help_open || ui.over {
        // Plain arrow in menus.
        ui.rect(mx - 1.0, my - 8.0, 2.0, 16.0, WHITE);
        ui.rect(mx - 8.0, my - 1.0, 16.0, 2.0, WHITE);
        return;
    }
    let spread = 8.0 + app.world.me().spread * 16.0 + app.world.me().recoil * 12.0;
    let c = if app.world.me().reload > 0.0 { ACCENT } else { [1.0, 1.0, 1.0, 0.95] };
    let (l, t) = (10.0, 2.0);
    ui.rect(mx - spread - l, my - t * 0.5, l, t, c);
    ui.rect(mx + spread, my - t * 0.5, l, t, c);
    ui.rect(mx - t * 0.5, my - spread - l, t, l, c);
    ui.rect(mx - t * 0.5, my + spread, t, l, c);
    ui.rect(mx - 1.0, my - 1.0, 2.0, 2.0, c);
    let def = &WEAPONS[app.world.me().weapon];
    if app.world.me().reload > 0.0 {
        let k = 1.0 - app.world.me().reload / def.reload;
        ui.bar(mx - 24.0, my + spread + 14.0, 48.0, 5.0, k, ACCENT);
    }
}

fn ingame(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    let world = &app.world;
    let m = 14.0 * s;

    // ---- world-space bars for damaged structures and big zombies
    for st in &world.structures {
        if st.hp >= st.max_hp - 0.5 || st.kind == SKind::Core {
            continue;
        }
        if let Some((sx, sy)) = app.cam.world_to_screen(st.pos + Vec3::Y * 2.7, w, h) {
            if sx > 0.0 && sy > 0.0 && sx < w && sy < h {
                let k = st.hp / st.max_hp;
                ui.bar(sx - 20.0 * s, sy, 40.0 * s, 4.0 * s, k, hp_color(k));
            }
        }
    }
    for z in &world.zombies {
        if !matches!(z.kind, ZKind::Brute) || z.hp >= z.max_hp {
            continue;
        }
        if let Some((sx, sy)) = app.cam.world_to_screen(z.pos + Vec3::Y * 2.9 * z.scale.min(1.6), w, h) {
            if sx > 0.0 && sy > 0.0 && sx < w && sy < h {
                ui.bar(sx - 24.0 * s, sy, 48.0 * s, 4.0 * s, z.hp / z.max_hp, RED);
            }
        }
    }

    // ---- vitals (bottom left)
    let p = world.me();
    let (px, py) = (m, h - m - 62.0 * s);
    ui.panel(px, py, 300.0 * s, 62.0 * s);
    ui.text_shadow(px + 8.0 * s, py + 7.0 * s, 2.0 * s, "HP", WHITE);
    let hpk = p.hp / p.max_hp;
    ui.bar(px + 40.0 * s, py + 6.0 * s, 250.0 * s, 16.0 * s, hpk, hp_color(hpk));
    let hp_txt = format!("{:.0}", p.hp.max(0.0));
    ui.text_shadow(px + 46.0 * s, py + 9.0 * s, 1.0 * s, &hp_txt, WHITE);
    ui.bar(px + 40.0 * s, py + 25.0 * s, 250.0 * s, 6.0 * s, p.stamina, BLUE);
    let line = format!("SCRAP {}", world.scrap);
    ui.text_shadow(px + 8.0 * s, py + 38.0 * s, 2.0 * s, &line, ACCENT);
    let g = format!("GRENADES {}", p.grenades);
    ui.text_shadow(px + 190.0 * s, py + 42.0 * s, 1.0 * s, &g, WHITE);

    // ---- weapon (bottom right)
    let def = &WEAPONS[p.weapon];
    let (wx, wy, ww) = (w - m - 320.0 * s, h - m - 62.0 * s, 320.0 * s);
    ui.panel(wx, wy, ww, 62.0 * s);
    ui.text_shadow(wx + 10.0 * s, wy + 8.0 * s, 2.0 * s, def.name, WHITE);
    let reserve = if def.reserve == u32::MAX { "INF".to_string() } else { p.reserve[p.weapon].to_string() };
    let ammo = format!("{}", p.mag[p.weapon]);
    let ammo_col = if p.mag[p.weapon] * 4 < def.mag { RED } else { WHITE };
    ui.text_shadow(wx + 10.0 * s, wy + 30.0 * s, 3.0 * s, &ammo, ammo_col);
    let ax = wx + 10.0 * s + (ammo.len() as f32 + 0.5) * 18.0 * s;
    ui.text_shadow(ax, wy + 40.0 * s, 1.0 * s, &format!("/ {reserve}"), DIM);
    if def.spinup > 0.0 {
        ui.bar(wx + 150.0 * s, wy + 34.0 * s, 150.0 * s, 6.0 * s, p.spin, ACCENT);
        ui.text_shadow(wx + 150.0 * s, wy + 44.0 * s, 1.0 * s, "SPIN-UP", DIM);
    } else if p.reload > 0.0 {
        ui.text_shadow(wx + 150.0 * s, wy + 38.0 * s, 2.0 * s, "RELOADING", ACCENT);
    } else if p.on_depot {
        ui.text_shadow(wx + 150.0 * s, wy + 38.0 * s, 2.0 * s, "RESUPPLY", GREEN);
    }

    // Owned weapon strip.
    let owned: Vec<usize> = (0..WEAPONS.len()).filter(|&i| p.owned[i]).collect();
    let slot = 18.0 * s;
    let mut sx = w - m - owned.len() as f32 * (slot + 2.0);
    for (n, &i) in owned.iter().enumerate() {
        let c = if i == p.weapon { PANEL_HI } else { PANEL };
        ui.rect(sx, wy - slot - 4.0, slot, slot, c);
        ui.frame_rect(sx, wy - slot - 4.0, slot, slot, 1.0, if i == p.weapon { ACCENT } else { [0.35, 0.35, 0.3, 0.7] });
        if n < 9 {
            ui.text(sx + 4.0 * s, wy - slot + 1.0 * s, 1.0 * s, &format!("{}", n + 1), WHITE);
        }
        sx += slot + 2.0;
    }

    // ---- wave banner (top center)
    let cx = w * 0.5;
    ui.panel(cx - 170.0 * s, m, 340.0 * s, 48.0 * s);
    match world.phase {
        Phase::Build { timer } => {
            let t = if world.hold_timer {
                "LOBBY".to_string()
            } else if world.wave == 0 {
                "PREPARE".to_string()
            } else {
                format!("WAVE {} CLEAR", world.wave)
            };
            ui.text_center(cx, m + 7.0 * s, 2.0 * s, &t, GREEN);
            let c = if world.hold_timer {
                if app.is_client() { "WAITING FOR THE HOST TO START".to_string() } else { "[ENTER] START THE MATCH".to_string() }
            } else {
                format!("NEXT WAVE {:.0}S   [ENTER] START NOW", timer.ceil())
            };
            ui.text_center(cx, m + 30.0 * s, 1.0 * s, &c, WHITE);
        }
        Phase::Assault => {
            let t = format!("WAVE {}", world.wave);
            ui.text_center(cx, m + 7.0 * s, 2.0 * s, &t, RED);
            let n = world.zombies.len();
            let rest = (world.spawn_budget.max(0.0) / 1.3).ceil() as u32;
            let c = format!("ALIVE {n}  +{rest} INBOUND");
            ui.text_center(cx, m + 30.0 * s, 1.0 * s, &c, WHITE);
        }
    }

    // ---- reactor health (under banner)
    if let Some(core) = world.core() {
        let k = core.hp / core.max_hp;
        let by = m + 54.0 * s;
        ui.bar(cx - 170.0 * s, by, 340.0 * s, 10.0 * s, k, if k > 0.3 { BLUE } else { RED });
        if world.core_alert > 0.0 && (world.time * 6.0).sin() > 0.0 {
            ui.text_center(cx, by + 14.0 * s, 1.0 * s, "REACTOR UNDER ATTACK", RED);
        }
    }

    // ---- titan health
    if let Some(t) = world.zombies.iter().find(|z| z.kind == ZKind::Titan) {
        let by = m + 84.0 * s;
        ui.text_center(cx, by, 1.0 * s, "TITAN", RED);
        ui.bar(cx - 300.0 * s, by + 12.0 * s, 600.0 * s, 14.0 * s, t.hp / t.max_hp, RED);
    }

    // ---- kill counter + streak
    let tx = m;
    ui.panel(tx, m, 170.0 * s, 48.0 * s);
    ui.text_shadow(tx + 8.0 * s, m + 7.0 * s, 2.0 * s, &format!("KILLS {}", world.stats.kills), WHITE);
    if world.combo > 1 {
        let mult = 1.0 + (world.combo as f32 / 120.0).min(0.5);
        ui.text_shadow(tx + 8.0 * s, m + 30.0 * s, 1.0 * s, &format!("STREAK {}  X{:.1}", world.combo, mult), ACCENT);
    }

    minimap(world, ui, m);

    // ---- messages
    // Below the reactor and Titan bars so that nothing overlaps.
    let mut y = m + 124.0 * s;
    // Menus drawn over the battle have their own headings in that spot.
    let show_messages = app.screen == Screen::Playing;
    for msg in world.messages.iter().filter(|m| m.big && show_messages) {
        let a = (msg.ttl * 2.0).min(1.0);
        ui.text_center(cx, y, 4.0 * s, &msg.text, [msg.color[0], msg.color[1], msg.color[2], a]);
        y += 40.0 * s;
    }
    for msg in world.messages.iter().filter(|m| !m.big && show_messages) {
        let a = (msg.ttl * 2.0).min(1.0);
        ui.text_center(cx, y, 2.0 * s, &msg.text, [msg.color[0], msg.color[1], msg.color[2], a]);
        y += 22.0 * s;
    }
    if p.out && !world.game_over {
        ui.text_center(cx, h * 0.8, 2.0 * s, "YOU ARE OUT - SPECTATING YOUR TEAM", [1.0, 0.45, 0.35, 1.0]);
    } else if !p.alive && !world.game_over {
        ui.text_center(cx, h * 0.45, 3.0 * s, &format!("REDEPLOY IN {:.0}", p.respawn.ceil()), WHITE);
    }
    if world.permadeath {
        ui.text_shadow(m + 8.0 * s, m + 54.0 * s, 1.0 * s, "PERMADEATH", RED);
    }

    // ---- key hints
    if app.screen == Screen::Playing && !app.armory_open && app.build_sel.is_none() {
        let hint = if app.is_multiplayer() { "[B] BUILD   [TAB] ARMORY   [P] PLAYERS   [H] HELP" } else { "[B] BUILD   [TAB] ARMORY   [F5] SAVE   [H] HELP" };
        ui.text_center(cx, h - m - 12.0 * s, 1.0 * s, hint, [0.8, 0.8, 0.76, 0.8]);
        // Contextual: what is under the cursor?
        hover_info(app, ui);
    }
    if let Some(note) = &app.ui_note {
        ui.text_center(cx, h * 0.72, 2.0 * s, &note.0, [1.0, 0.55, 0.25, (note.1 * 2.0).min(1.0)]);
    }
}

fn hover_info(app: &App, ui: &mut Ui) {
    let (w, s) = (ui.w, ui.s);
    let world = &app.world;
    let t = app.hover_tile;
    let Some(i) = world.structure_at(t) else { return };
    let st = &world.structures[i];
    let d = sdef(st.kind);
    let c = tile_center(t);
    let Some((sx, sy)) = app.cam.world_to_screen(vec3(c.x, 2.0, c.y), w, ui.h) else { return };
    let (bw, bh) = (210.0 * s, 64.0 * s);
    let (x, y) = ((sx + 22.0).min(w - bw - 4.0), (sy - bh).max(4.0));
    ui.rect(x, y, bw, bh, PANEL);
    ui.frame_rect(x, y, bw, bh, 1.0, [0.5, 0.45, 0.3, 0.7]);
    let lv = if d.turret.is_some() { format!("{} LV{}", d.name, st.level + 1) } else { d.name.to_string() };
    ui.text_shadow(x + 6.0 * s, y + 6.0 * s, 1.0 * s, &lv, WHITE);
    let k = st.hp / st.max_hp;
    ui.bar(x + 6.0 * s, y + 18.0 * s, bw - 12.0 * s, 6.0 * s, k, hp_color(k));
    let mut ly = y + 29.0 * s;
    let rc = world.repair_cost(i);
    if rc > 0 {
        ui.text(x + 6.0 * s, ly, 1.0 * s, &format!("[T] REPAIR {rc}"), GREEN);
        ly += 11.0 * s;
    }
    if let Some(u) = world.upgrade_cost(i) {
        ui.text(x + 6.0 * s, ly, 1.0 * s, &format!("[U] UPGRADE {u}"), ACCENT);
        ly += 11.0 * s;
    }
    if st.kind != SKind::Core {
        ui.text(x + 6.0 * s, ly, 1.0 * s, "[X] SELL", DIM);
    }
}

fn minimap(world: &World, ui: &mut Ui, m: f32) {
    let s = ui.s;
    let size = 170.0 * s;
    let (x, y) = (ui.w - m - size, m);
    let k = size / (2.0 * HALF);
    let to = |wx: f32, wz: f32| (x + (wx + HALF) * k, y + (wz + HALF) * k);
    ui.rect(x, y, size, size, [0.03, 0.04, 0.045, 0.78]);
    ui.frame_rect(x, y, size, size, 1.0, [0.35, 0.33, 0.25, 0.7]);
    let (a, b) = to(-7.5, -HALF);
    ui.rect(a, b, 15.0 * k, size, [0.15, 0.15, 0.15, 0.6]);
    let (a, b) = to(-HALF, -7.5);
    ui.rect(a, b, size, 15.0 * k, [0.15, 0.15, 0.15, 0.6]);
    for sol in &world.map.solids {
        if sol.max.y < 1.5 {
            continue;
        }
        let (a, b) = to(sol.min.x, sol.min.z);
        ui.rect(a, b, ((sol.max.x - sol.min.x) * k).max(1.0), ((sol.max.z - sol.min.z) * k).max(1.0), [0.38, 0.36, 0.33, 0.8]);
    }
    for st in &world.structures {
        let (a, b) = to(st.pos.x, st.pos.z);
        let col: C = if st.kind == SKind::Core { BLUE } else if sdef(st.kind).turret.is_some() { GREEN } else { [0.7, 0.62, 0.4, 1.0] };
        let r = if st.kind == SKind::Core { 3.0 } else { 1.5 } * s;
        ui.rect(a - r, b - r, r * 2.0, r * 2.0, col);
    }
    for z in &world.zombies {
        let (a, b) = to(z.pos.x, z.pos.z);
        let big = matches!(z.kind, ZKind::Titan);
        let r = if big { 3.0 * s } else { 1.0 };
        ui.rect(a - r, b - r, r * 2.0, r * 2.0, RED);
    }
    if world.me().alive {
        let (a, b) = to(world.me().pos.x, world.me().pos.z);
        ui.rect(a - 2.5 * s, b - 2.5 * s, 5.0 * s, 5.0 * s, WHITE);
    }
}

fn build_bar(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    let n = BUILDABLE.len();
    let slot = ((w - 40.0) / n as f32).min(84.0 * s);
    let total = slot * n as f32;
    let x0 = (w - total) * 0.5;
    let y0 = h - 200.0 * s;
    let sel = app.build_sel.unwrap_or(0);
    let wave = app.world.wave.max(1);
    let mut picked = None;
    ui.panel(x0 - 6.0, y0 - 6.0, total + 12.0, 78.0 * s + 12.0);
    for (i, &kind) in BUILDABLE.iter().enumerate() {
        let d = sdef(kind);
        let x = x0 + i as f32 * slot;
        let locked = d.unlock_wave > wave;
        let afford = app.world.scrap >= d.cost;
        let hover = ui.hovered(x, y0, slot - 4.0, 78.0 * s);
        ui.rect(x, y0, slot - 4.0, 78.0 * s, if i == sel { PANEL_HI } else if hover { [0.11, 0.11, 0.1, 0.9] } else { [0.03, 0.03, 0.035, 0.85] });
        ui.frame_rect(x, y0, slot - 4.0, 78.0 * s, if i == sel { 2.0 } else { 1.0 }, if i == sel { ACCENT } else { [0.3, 0.3, 0.27, 0.7] });
        if i < 10 {
            ui.text(x + 4.0, y0 + 4.0, 1.0 * s, &format!("{}", (i + 1) % 10), DIM);
        }
        let name = d.name.split(' ').collect::<Vec<_>>();
        let mut ly = y0 + 18.0 * s;
        for part in name.iter().take(2) {
            let tw = ui.text_w(part, 1.0 * s);
            let tcol = if locked { DIM } else { WHITE };
            let fit: String = part.chars().take(((slot - 8.0) / (6.0 * s.round().max(1.0))) as usize).collect();
            ui.text(x + ((slot - 4.0 - tw.min(slot - 8.0)) * 0.5).max(2.0), ly, 1.0 * s, &fit, tcol);
            ly += 11.0 * s;
        }
        let cost = if locked { format!("WAVE {}", d.unlock_wave) } else { format!("{}", d.cost) };
        let cw = ui.text_w(&cost, 1.0 * s);
        ui.text(x + (slot - 4.0 - cw) * 0.5, y0 + 60.0 * s, 1.0 * s, &cost, if locked { RED } else if afford { ACCENT } else { RED });
        if hover && ui.click && !locked {
            picked = Some(i);
        }
    }
    if let Some(i) = picked {
        app.build_sel = Some(i);
        app.world.snd_ui(Sfx::Click, 0.5);
    }
    let sel = app.build_sel.unwrap_or(0);
    let d = sdef(BUILDABLE[sel]);
    ui.text_center(w * 0.5, y0 - 36.0 * s, 2.0 * s, d.name, ACCENT);
    ui.text_center(w * 0.5, y0 - 18.0 * s, 1.0 * s, d.blurb, WHITE);
    ui.text_center(w * 0.5, y0 + 92.0 * s, 1.0 * s, "[LMB] PLACE   [RMB/B] EXIT   [WHEEL] SELECT   [X] SELL   [U] UPGRADE   [T] REPAIR", [0.85, 0.85, 0.8, 0.9]);
    hover_info(app, ui);
}

fn armory(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    let cols = 3;
    let rows = WEAPONS.len().div_ceil(cols);
    let cw = (w - 80.0).min(1500.0 * s) / cols as f32;
    let ch = 56.0 * s;
    let total_w = cw * cols as f32;
    let total_h = ch * rows as f32 + 112.0 * s;
    let (x0, y0) = ((w - total_w) * 0.5, ((h - total_h) * 0.5).max(4.0));
    ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.35]);
    ui.panel(x0 - 10.0, y0 - 10.0, total_w + 20.0, total_h + 20.0);
    ui.text_shadow(x0, y0, 3.0 * s, "ARMORY", ACCENT);
    let scrap = format!("SCRAP {}", app.world.scrap);
    let sw = ui.text_w(&scrap, 2.0 * s);
    ui.text_shadow(x0 + total_w - sw, y0 + 2.0 * s, 2.0 * s, &scrap, ACCENT);
    let wave = app.world.wave.max(1);
    let top = y0 + 30.0 * s;
    let mut note: Option<&'static str> = None;
    let mut buy: Option<(usize, bool)> = None;
    for (i, def) in WEAPONS.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let x = x0 + c as f32 * cw;
        let y = top + r as f32 * ch;
        let (bw, bh) = (cw - 6.0, ch - 6.0);
        let owned = app.world.me().owned[i];
        let locked = !owned && def.unlock_wave > wave;
        let current = app.world.me().weapon == i;
        let hover = ui.hovered(x, y, bw, bh);
        ui.rect(x, y, bw, bh, if current { PANEL_HI } else if hover { [0.11, 0.11, 0.1, 0.92] } else { [0.03, 0.03, 0.035, 0.88] });
        ui.frame_rect(x, y, bw, bh, if current { 2.0 } else { 1.0 }, if current { ACCENT } else { [0.3, 0.3, 0.27, 0.7] });
        ui.text(x + 8.0 * s, y + 6.0 * s, 2.0 * s, def.name, if locked { DIM } else { WHITE });
        ui.text(x + 8.0 * s, y + 25.0 * s, 1.0 * s, def.blurb, DIM);
        let dps = match def.fire {
            Fire::Hitscan { pellets, .. } => def.damage * pellets as f32 * def.rate,
            _ => def.damage * def.rate,
        };
        ui.text(x + 8.0 * s, y + 37.0 * s, 1.0 * s, &format!("{:.0} DPS  MAG {}", dps, def.mag), BLUE);
        let (label, color) = if owned {
            if def.reserve == u32::MAX {
                ("OWNED".to_string(), GREEN)
            } else {
                (format!("AMMO {}", def.ammo_price), GREEN)
            }
        } else if locked {
            (format!("WAVE {}", def.unlock_wave), RED)
        } else {
            (format!("{}", def.price), if app.world.scrap >= def.price { ACCENT } else { RED })
        };
        let lw = ui.text_w(&label, 2.0 * s);
        ui.text(x + bw - lw - 8.0 * s, y + 6.0 * s, 2.0 * s, &label, color);
        if owned && def.reserve != u32::MAX {
            let k = format!("{}", app.world.me().reserve[i]);
            let kw = ui.text_w(&k, 1.0 * s);
            ui.text(x + bw - kw - 8.0 * s, y + 37.0 * s, 1.0 * s, &k, DIM);
        }
        if hover && ui.click && !locked {
            buy = Some((i, owned));
        }
        if hover && owned && ui.mx > x + bw * 0.6 {
            note = Some("CLICK THE PRICE AREA TO BUY AMMO, ELSEWHERE TO EQUIP");
        }
    }
    if let Some((i, owned)) = buy {
        let result = if owned {
            let bx = x0 + (i % cols) as f32 * cw;
            if ui.mx > bx + (cw - 6.0) * 0.6 && WEAPONS[i].reserve != u32::MAX {
                app.do_cmd(Command::BuyAmmo(i as u8))
            } else {
                app.queued_select = Some(i);
                app.world.snd_ui(Sfx::Click, 0.6);
                Ok(())
            }
        } else {
            app.do_cmd(Command::BuyWeapon(i as u8))
        };
        if let Err(e) = result {
            app.ui_note = Some((e, 1.5));
            app.world.snd_ui(Sfx::Denied, 0.7);
        }
    }
    let by = top + rows as f32 * ch + 8.0 * s;
    let label = format!("BUY 2 GRENADES (120)  HOLDING {}", app.world.me().grenades);
    if ui.button(x0, by, 360.0 * s, 32.0 * s, &label, 1.0 * s, true) {
        if let Err(e) = app.do_cmd(Command::BuyGrenades) {
            app.ui_note = Some((e, 1.5));
            app.world.snd_ui(Sfx::Denied, 0.7);
        }
    }
    if ui.button(x0 + total_w - 170.0 * s, by, 170.0 * s, 32.0 * s, "CLOSE [TAB]", 1.0 * s, true) {
        app.armory_open = false;
    }
    let msg = note.unwrap_or("THE HORDE DOES NOT WAIT WHILE YOU SHOP");
    ui.text_center(w * 0.5, by + 42.0 * s, 1.0 * s, msg, DIM);
}

fn help(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.45]);
    let (bw, bh) = (720.0 * s, 510.0 * s);
    let (x, y) = ((w - bw) * 0.5, (h - bh) * 0.5);
    ui.panel(x, y, bw, bh);
    ui.text_center(w * 0.5, y + 12.0 * s, 3.0 * s, "FIELD MANUAL", ACCENT);
    let lines: [(&str, &str); 19] = [
        ("WASD", "MOVE"),
        ("MOUSE", "AIM"),
        ("LMB", "SHOOT"),
        ("R", "RELOAD"),
        ("SHIFT", "SPRINT"),
        ("SPACE", "DODGE ROLL (BRIEF INVULNERABILITY)"),
        ("G", "THROW GRENADE AT THE CURSOR"),
        ("1-9, WHEEL, Q/E", "SWITCH WEAPON"),
        ("F", "FLASHLIGHT"),
        ("TAB", "ARMORY: BUY GUNS AND AMMO"),
        ("B", "BUILD MODE: WALLS, TRAPS, SENTRIES"),
        ("U / T / X", "UPGRADE / REPAIR / SELL WHAT THE CURSOR IS ON"),
        ("ENTER", "START THE NEXT WAVE EARLY FOR A BONUS"),
        ("F5 / F9", "QUICKSAVE / QUICKLOAD. WAVE CLEARS AUTOSAVE"),
        ("P", "PLAYER LIST AND SERVER PANEL IN MULTIPLAYER"),
        ("- / =", "CAMERA ZOOM"),
        ("M", "TOGGLE MUSIC"),
        ("F3", "PERFORMANCE COUNTERS"),
        ("ESC", "PAUSE"),
    ];
    let mut ly = y + 50.0 * s;
    for (k, v) in lines {
        ui.text(x + 30.0 * s, ly, 1.0 * s, k, ACCENT);
        ui.text(x + 210.0 * s, ly, 1.0 * s, v, WHITE);
        ly += 19.0 * s;
    }
    ui.text_center(w * 0.5, y + bh - 66.0 * s, 1.0 * s, "DEFEND THE REACTOR. IF IT FALLS, YOU LOSE. IF YOU FALL, YOU REDEPLOY.", GREEN);
    ui.text_center(w * 0.5, y + bh - 50.0 * s, 1.0 * s, "EXPLOSIVE BARRELS CHAIN. BURNING ZOMBIES SPREAD FIRE. FROZEN ONES SHATTER.", WHITE);
    if ui.button(w * 0.5 - 80.0 * s, y + bh - 34.0 * s, 160.0 * s, 26.0 * s, "BACK [H]", 1.0 * s, true) {
        app.help_open = false;
    }
}

/// Where the rendered logo sits, in pixels.
pub fn logo_rect(size: (u32, u32)) -> [f32; 4] {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let lw = (w * 0.40).min(h * 0.86);
    [w * 0.045, h * 0.035, lw, lw * 400.0 / 1400.0]
}

fn crosshair(ui: &mut Ui) {
    ui.rect(ui.mx - 1.0, ui.my - 8.0, 2.0, 16.0, WHITE);
    ui.rect(ui.mx - 8.0, ui.my - 1.0, 16.0, 2.0, WHITE);
}

/// Sparks rising through the frame, pixel-sized to match the UI.
fn embers(ui: &mut Ui) {
    let t = ui.time;
    for k in 0..46u32 {
        let seed = k as f32 * 12.9898;
        let speed = 0.015 + (seed.sin().abs()) * 0.03;
        let y = 1.0 - (t * speed + (seed * 3.1).sin().abs()).fract();
        let x = (seed * 7.7).sin().abs() + (t * 0.3 + seed).sin() * 0.012;
        let fade = (y * 2.2).min(1.0) * ((1.0 - y) * 7.0).min(1.0);
        let twinkle = 0.55 + 0.45 * (t * 5.0 + seed).sin();
        let px = if k % 5 == 0 { 3.0 } else { 2.0 };
        ui.rect(x * ui.w, y * ui.h, px, px, [1.0, 0.45 + twinkle * 0.25, 0.1, fade * twinkle * 0.75]);
    }
}

/// A menu entry: left aligned, with a bar that lights up under the cursor.
fn menu_item(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, label: &str, sub: &str, enabled: bool) -> bool {
    let s = ui.s;
    let hover = ui.hovered(x, y, w, h) && enabled;
    if hover {
        ui.over = true;
        ui.rect(x, y, w, h, [0.5, 0.24, 0.06, 0.28]);
        ui.rect(x, y, 4.0 * s, h, ACCENT);
    } else {
        ui.rect(x, y, 2.0 * s, h, [0.5, 0.45, 0.35, 0.45]);
    }
    let color = if !enabled { DIM } else if hover { [1.0, 0.86, 0.55, 1.0] } else { WHITE };
    let tx = x + if hover { 18.0 * s } else { 12.0 * s };
    if sub.is_empty() {
        ui.text_shadow(tx, y + (h - 14.0 * s.round()) * 0.5, 2.0 * s, label, color);
    } else {
        ui.text_shadow(tx, y + 4.0 * s, 2.0 * s, label, color);
        ui.text_shadow(tx, y + h - 13.0 * s, 1.0 * s, sub, [0.72, 0.72, 0.68, 1.0]);
    }
    hover && ui.click
}

#[allow(clippy::too_many_arguments)]
fn field(ui: &mut Ui, focus: &mut Option<u8>, id: u8, x: f32, y: f32, w: f32, label: &str, value: &mut String, masked: bool, max: usize) {
    let s = ui.s;
    let h = 24.0 * s;
    ui.text_shadow(x, y, 1.0 * s, label, [0.78, 0.76, 0.7, 1.0]);
    let by = y + 12.0 * s;
    let hover = ui.hovered(x, by, w, h);
    if hover {
        ui.over = true;
        if ui.click {
            *focus = Some(id);
        }
    }
    let focused = *focus == Some(id);
    if focused {
        for c in std::mem::take(&mut ui.typed) {
            if !c.is_control() && c.is_ascii() && value.chars().count() < max {
                value.push(c);
            }
        }
        if ui.backspace {
            value.pop();
        }
    }
    ui.rect(x, by, w, h, [0.02, 0.02, 0.025, 0.88]);
    ui.frame_rect(x, by, w, h, if focused { 2.0 } else { 1.0 }, if focused { ACCENT } else if hover { [0.7, 0.62, 0.42, 0.9] } else { [0.4, 0.38, 0.32, 0.7] });
    let shown: String = if masked { "*".repeat(value.chars().count()) } else { value.clone() };
    let scale = (1.5 * s).round().max(1.0);
    let room = ((w - 14.0) / (6.0 * scale)) as usize;
    let tail: String = shown.chars().skip(shown.chars().count().saturating_sub(room.saturating_sub(1))).collect();
    ui.text(x + 6.0, by + (h - 7.0 * scale) * 0.5, scale, &tail, WHITE);
    if focused && (ui.time * 2.5).fract() < 0.6 {
        let cx = x + 6.0 + tail.chars().count() as f32 * 6.0 * scale;
        ui.rect(cx, by + 4.0 * s, scale, h - 8.0 * s, ACCENT);
    }
}

fn toggle(ui: &mut Ui, x: f32, y: f32, w: f32, label: &str, on: &mut bool) -> bool {
    let s = ui.s;
    let h = 22.0 * s;
    let hover = ui.hovered(x, y, w, h);
    if hover {
        ui.over = true;
    }
    let bx = 16.0 * s;
    ui.rect(x, y + 3.0 * s, bx, bx, [0.02, 0.02, 0.025, 0.88]);
    ui.frame_rect(x, y + 3.0 * s, bx, bx, 1.0, if hover { ACCENT } else { [0.5, 0.46, 0.36, 0.8] });
    if *on {
        ui.rect(x + 3.0 * s, y + 6.0 * s, bx - 6.0 * s, bx - 6.0 * s, ACCENT);
    }
    ui.text_shadow(x + bx + 8.0 * s, y + 7.0 * s, 1.0 * s, label, WHITE);
    if hover && ui.click {
        *on = !*on;
        return true;
    }
    false
}

fn slider(ui: &mut Ui, x: f32, y: f32, w: f32, label: &str, value: &mut f32) -> bool {
    let s = ui.s;
    ui.text_shadow(x, y, 1.0 * s, &format!("{label}  {:.0}%", *value * 100.0), WHITE);
    let (by, h) = (y + 12.0 * s, 12.0 * s);
    ui.bar(x, by, w, h, *value, ACCENT);
    let hover = ui.hovered(x, by - 3.0, w, h + 6.0);
    if hover {
        ui.over = true;
        if ui.click {
            *value = ((ui.mx - x) / w).clamp(0.0, 1.0);
            return true;
        }
    }
    false
}

fn clock(seconds: f32) -> String {
    let t = seconds as u32;
    format!("{}:{:02}", t / 60, t % 60)
}

fn title(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    embers(ui);
    let logo = logo_rect((w as u32, h as u32));
    let x = logo[0] + 4.0 * s;
    let top = logo[1] + logo[3] + 12.0 * s;
    let cw = (340.0 * s).min(w * 0.42);
    // A soft shadow behind the column keeps text readable over the bright parts of the art.
    let edge = x + cw + 30.0 * s;
    ui.rect(0.0, 0.0, edge, h, [0.0, 0.0, 0.0, 0.42]);
    // Whole-pixel strips that meet exactly: any overlap would darken twice and show as a line.
    let edge = edge.round();
    let fade = (110.0 * s).round() as i32;
    for px in 0..fade {
        let f = px as f32 / fade as f32;
        ui.rect(edge + px as f32, 0.0, 1.0, h, [0.0, 0.0, 0.0, 0.42 * (1.0 - f) * (1.0 - f)]);
    }
    match app.page {
        Page::Main => page_main(app, ui, x, top, cw),
        Page::NewGame => page_new_game(app, ui, x, top, cw),
        Page::Multiplayer => page_multiplayer(app, ui, x, top, cw),
        Page::Host => page_host(app, ui, x, top, cw),
        Page::Join => page_join(app, ui, x, top, cw),
        Page::Connecting => {
            let dots = ".".repeat((ui.time * 3.0) as usize % 4);
            ui.text_shadow(x, top + 20.0 * s, 2.0 * s, &format!("CONNECTING{dots}"), ACCENT);
            ui.text_shadow(x, top + 44.0 * s, 1.0 * s, &app.settings.address.clone(), WHITE);
            if menu_item(ui, x, top + 70.0 * s, cw, 30.0 * s, "CANCEL", "", true) {
                app.session = Session::Solo;
                app.page = Page::Join;
            }
        }
        Page::Records => page_records(app, ui, x, top, cw),
        Page::Options => page_options(app, ui, x, top, cw),
        Page::Manual => page_manual(app, ui, x, top),
    }
    if let Some(e) = &app.menu_error {
        ui.text_shadow(x, h - 46.0 * s, 2.0 * s, e, RED);
    }
    ui.text_shadow(w - ui.text_w(VERSION, 1.0 * s) - 10.0 * s, h - 16.0 * s, 1.0 * s, VERSION, DIM);
    let notice = if cfg!(target_arch = "wasm32") {
        "FREE SOFTWARE, GPL-3.0. SOURCE AND DESKTOP VERSION WITH CO-OP: GITHUB.COM/1STWBSM/ROTFALL"
    } else {
        "FREE SOFTWARE, GPL-3.0. SOURCE: GITHUB.COM/1STWBSM/ROTFALL"
    };
    ui.text_shadow(10.0 * s, h - 16.0 * s, 1.0 * s, notice, DIM);
    crosshair(ui);
}

fn page_main(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    let rh = 38.0 * s;
    let step = rh + 5.0 * s;
    if let Some(info) = app.slot_info(Slot::Solo).cloned() {
        let sub = format!("WAVE {}  SCRAP {}  {}", info.wave, info.scrap, clock(info.seconds));
        if menu_item(ui, x, y, cw, rh, "CONTINUE", &sub, true) {
            app.action = Some(Action::Continue(Slot::Solo));
        }
        y += step;
    }
    if let Some(info) = app.slot_info(Slot::Permadeath).cloned() {
        let sub = format!("WAVE {}  {} KILLS  ONE LIFE", info.wave, info.kills);
        if menu_item(ui, x, y, cw, rh, "RESUME PERMADEATH", &sub, true) {
            app.action = Some(Action::Continue(Slot::Permadeath));
        }
        y += step;
    }
    // Browsers have no sockets a game server could use, so the web build is single player.
    let multiplayer = cfg!(not(target_arch = "wasm32"));
    let rows: [(&str, &str, Page); 5] = [
        ("NEW GAME", "STANDARD OR PERMADEATH", Page::NewGame),
        ("MULTIPLAYER", "CO-OP: HOST OR JOIN", Page::Multiplayer),
        ("RECORDS", "YOUR BEST RUNS", Page::Records),
        ("OPTIONS", "", Page::Options),
        ("FIELD MANUAL", "", Page::Manual),
    ];
    for (label, sub, page) in rows {
        if page == Page::Multiplayer && !multiplayer {
            continue;
        }
        if menu_item(ui, x, y, cw, rh, label, sub, true) {
            app.page = page;
            app.menu_error = None;
        }
        y += step;
    }
    if cfg!(not(target_arch = "wasm32")) && menu_item(ui, x, y, cw, rh, "QUIT", "", true) {
        app.action = Some(Action::Quit);
    }
}

fn back_item(app: &mut App, ui: &mut Ui, x: f32, y: f32, cw: f32, to: Page) {
    let s = ui.s;
    if menu_item(ui, x, y, cw, 30.0 * s, "BACK", "", true) {
        app.page = to;
        app.focus = None;
        app.menu_error = None;
        app.store_settings();
    }
}

fn page_new_game(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "NEW GAME", ACCENT);
    y += 26.0 * s;
    let overwrite = app.slot_info(Slot::Solo).is_some();
    let sub = if overwrite { "REDEPLOY WHEN YOU DIE. REPLACES YOUR SAVE AT WAVE CLEAR" } else { "REDEPLOY WHEN YOU DIE. HOLD THE REACTOR" };
    if menu_item(ui, x, y, cw * 1.25, 42.0 * s, "STANDARD", sub, true) {
        app.action = Some(Action::NewGame { permadeath: false });
    }
    y += 48.0 * s;
    let sub = if app.slot_info(Slot::Permadeath).is_some() { "ONE LIFE. STARTING ENDS YOUR SUSPENDED RUN" } else { "ONE LIFE. DEATH ENDS THE RUN AND ERASES ITS SAVE" };
    if menu_item(ui, x, y, cw * 1.25, 42.0 * s, "PERMADEATH", sub, true) {
        app.action = Some(Action::NewGame { permadeath: true });
    }
    y += 56.0 * s;
    back_item(app, ui, x, y, cw, Page::Main);
}

fn page_multiplayer(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "MULTIPLAYER", ACCENT);
    y += 26.0 * s;
    if menu_item(ui, x, y, cw, 42.0 * s, "HOST A SERVER", "UP TO 8 PLAYERS, YOU PLAY TOO", true) {
        app.page = Page::Host;
        app.menu_error = None;
    }
    y += 48.0 * s;
    if menu_item(ui, x, y, cw, 42.0 * s, "JOIN A SERVER", "BY ADDRESS OR FROM YOUR NETWORK", true) {
        app.page = Page::Join;
        app.menu_error = None;
    }
    y += 56.0 * s;
    back_item(app, ui, x, y, cw, Page::Main);
}

fn page_host(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "HOST A SERVER", ACCENT);
    y += 24.0 * s;
    let gap = 42.0 * s;
    field(ui, &mut app.focus, 1, x, y, cw, "SERVER NAME", &mut app.settings.server_name, false, 24);
    y += gap;
    field(ui, &mut app.focus, 2, x, y, cw, "YOUR NAME", &mut app.settings.name, false, 16);
    y += gap;
    let half = (cw - 10.0 * s) * 0.5;
    field(ui, &mut app.focus, 3, x, y, half, "PORT", &mut app.settings.port, false, 5);
    field(ui, &mut app.focus, 4, x + half + 10.0 * s, y, half, "PASSWORD (OPTIONAL)", &mut app.password, true, 32);
    y += gap;
    // Player cap.
    ui.text_shadow(x, y + 7.0 * s, 1.0 * s, &format!("MAX PLAYERS  {}", app.settings.max_players), WHITE);
    let bx = x + 150.0 * s;
    if ui.button(bx, y, 28.0 * s, 22.0 * s, "-", 2.0 * s, app.settings.max_players > 1) {
        app.settings.max_players -= 1;
    }
    if ui.button(bx + 34.0 * s, y, 28.0 * s, 22.0 * s, "+", 2.0 * s, app.settings.max_players < net::MAX_PLAYERS_LIMIT) {
        app.settings.max_players += 1;
    }
    y += 28.0 * s;
    if toggle(ui, x, y, cw, "PERMADEATH: DEAD PLAYERS STAY DEAD", &mut app.settings.host_permadeath) {
        app.settings.host_continue = false;
    }
    y += 24.0 * s;
    let slot = Slot::for_mode(true, app.settings.host_permadeath);
    if let Some(info) = app.slot_info(slot).cloned() {
        let label = format!("CONTINUE SAVE: WAVE {}, {} PLAYERS", info.wave, info.players);
        toggle(ui, x, y, cw, &label, &mut app.settings.host_continue);
        y += 24.0 * s;
    } else {
        app.settings.host_continue = false;
    }
    y += 4.0 * s;
    if menu_item(ui, x, y, cw, 38.0 * s, "START SERVER", "PLAYERS CAN JOIN BEFORE THE FIRST WAVE", true) {
        app.action = Some(Action::StartHost);
        app.focus = None;
    }
    y += 44.0 * s;
    back_item(app, ui, x, y, cw, Page::Multiplayer);

    // Help on the far side: how friends actually reach you.
    let (px, pw) = (x + cw + 24.0 * s, 380.0 * s);
    if px + pw < ui.w {
        let py = ui.h * 0.32;
        ui.panel(px, py, pw, 118.0 * s);
        ui.text_shadow(px + 10.0 * s, py + 8.0 * s, 1.0 * s, "SAME NETWORK", ACCENT);
        let ip = net::local_ip().unwrap_or_else(|| "?".into());
        ui.text_shadow(px + 10.0 * s, py + 22.0 * s, 1.0 * s, &format!("FRIENDS JOIN {}:{}", ip, app.settings.port), WHITE);
        ui.text_shadow(px + 10.0 * s, py + 34.0 * s, 1.0 * s, "OR PICK YOU FROM THEIR JOIN LIST", WHITE);
        ui.text_shadow(px + 10.0 * s, py + 56.0 * s, 1.0 * s, "OVER THE INTERNET", ACCENT);
        ui.text_shadow(px + 10.0 * s, py + 70.0 * s, 1.0 * s, &format!("FORWARD TCP PORT {} ON YOUR ROUTER", app.settings.port), WHITE);
        ui.text_shadow(px + 10.0 * s, py + 82.0 * s, 1.0 * s, "AND SHARE YOUR PUBLIC ADDRESS.", WHITE);
        ui.text_shadow(px + 10.0 * s, py + 100.0 * s, 1.0 * s, "PLAY WITH PEOPLE YOU TRUST.", DIM);
    }
}

fn page_join(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "JOIN A SERVER", ACCENT);
    y += 24.0 * s;
    let gap = 42.0 * s;
    field(ui, &mut app.focus, 2, x, y, cw, "YOUR NAME", &mut app.settings.name, false, 16);
    y += gap;
    field(ui, &mut app.focus, 5, x, y, cw, "ADDRESS (HOST OR HOST:PORT)", &mut app.settings.address, false, 64);
    y += gap;
    field(ui, &mut app.focus, 4, x, y, cw, "PASSWORD (IF ANY)", &mut app.password, true, 32);
    y += gap + 2.0 * s;
    if menu_item(ui, x, y, cw, 34.0 * s, "JOIN", "", true) {
        app.action = Some(Action::Join);
        app.focus = None;
    }
    y += 40.0 * s;
    back_item(app, ui, x, y, cw, Page::Multiplayer);

    // Servers announcing themselves on the local network.
    let (px, pw) = (x + cw + 24.0 * s, 420.0 * s);
    let py = ui.h * 0.32;
    let list: Vec<net::LanServer> = app.lan.as_ref().map_or(Vec::new(), |l| l.servers.clone());
    let rows = list.len().clamp(1, 6) as f32;
    ui.panel(px, py, pw, 28.0 * s + rows * 34.0 * s);
    ui.text_shadow(px + 10.0 * s, py + 8.0 * s, 1.0 * s, "ON YOUR NETWORK", ACCENT);
    if list.is_empty() {
        let dots = ".".repeat((ui.time * 2.0) as usize % 4);
        ui.text_shadow(px + 10.0 * s, py + 34.0 * s, 1.0 * s, &format!("SEARCHING{dots}"), DIM);
    }
    for (i, sv) in list.iter().take(6).enumerate() {
        let ry = py + 26.0 * s + i as f32 * 34.0 * s;
        let sub = format!(
            "{}/{} PLAYERS  WAVE {}{}{}",
            sv.players, sv.max, sv.wave,
            if sv.permadeath { "  PERMADEATH" } else { "" },
            if sv.password { "  LOCKED" } else { "" }
        );
        if menu_item(ui, px + 8.0 * s, ry, pw - 16.0 * s, 30.0 * s, &sv.name.chars().take(20).collect::<String>(), &sub, true) {
            app.settings.address = sv.addr.clone();
            if !sv.password || !app.password.is_empty() {
                app.action = Some(Action::Join);
            } else {
                app.focus = Some(4);
                app.menu_error = Some("THAT SERVER NEEDS A PASSWORD".into());
            }
        }
    }
}

fn page_records(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "RECORDS", ACCENT);
    y += 26.0 * s;
    let pw = (620.0 * s).min(ui.w - x - 10.0);
    let rows = app.records.len().max(1) as f32;
    ui.panel(x, y, pw, 24.0 * s + rows * 16.0 * s);
    let cols = [8.0, 50.0, 110.0, 190.0, 250.0];
    for (c, t) in cols.iter().zip(["#", "WAVE", "KILLS", "TIME", "MODE"]) {
        ui.text_shadow(x + c * s, y + 7.0 * s, 1.0 * s, t, DIM);
    }
    if app.records.is_empty() {
        ui.text_shadow(x + 8.0 * s, y + 24.0 * s, 1.0 * s, "NO FINISHED RUNS YET. GO DIE.", WHITE);
    }
    for (i, r) in app.records.iter().enumerate() {
        let ry = y + 24.0 * s + i as f32 * 16.0 * s;
        let c = if r.mode.contains("PERMADEATH") { RED } else { WHITE };
        ui.text_shadow(x + cols[0] * s, ry, 1.0 * s, &format!("{}", i + 1), c);
        ui.text_shadow(x + cols[1] * s, ry, 1.0 * s, &format!("{}", r.wave), c);
        ui.text_shadow(x + cols[2] * s, ry, 1.0 * s, &format!("{}", r.kills), c);
        ui.text_shadow(x + cols[3] * s, ry, 1.0 * s, &clock(r.seconds as f32), c);
        ui.text_shadow(x + cols[4] * s, ry, 1.0 * s, &format!("{} {}", r.mode, r.name), c);
    }
    y += 34.0 * s + rows * 16.0 * s;
    back_item(app, ui, x, y, cw, Page::Main);
}

fn page_options(app: &mut App, ui: &mut Ui, x: f32, mut y: f32, cw: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "OPTIONS", ACCENT);
    y += 28.0 * s;
    if slider(ui, x, y, cw, "MUSIC", &mut app.settings.music) {
        app.apply_volume();
    }
    y += 34.0 * s;
    if slider(ui, x, y, cw, "EFFECTS", &mut app.settings.sfx) {
        app.apply_volume();
    }
    y += 36.0 * s;
    let mut full = app.settings.fullscreen;
    if toggle(ui, x, y, cw, "FULLSCREEN (F11)", &mut full) {
        app.settings.fullscreen = full;
        app.want_fullscreen = Some(full);
    }
    y += 26.0 * s;
    field(ui, &mut app.focus, 2, x, y, cw, "YOUR NAME", &mut app.settings.name, false, 16);
    y += 50.0 * s;
    let saves = if cfg!(target_arch = "wasm32") { "THIS BROWSER'S STORAGE".to_string() } else { app.data_dir.display().to_string() };
    ui.text_shadow(x, y, 1.0 * s, &format!("SAVES: {saves}"), DIM);
    y += 18.0 * s;
    back_item(app, ui, x, y, cw, Page::Main);
}

fn page_manual(app: &mut App, ui: &mut Ui, x: f32, mut y: f32) {
    let s = ui.s;
    ui.text_shadow(x, y, 2.0 * s, "FIELD MANUAL", ACCENT);
    y += 24.0 * s;
    let lines: [(&str, &str); 15] = [
        ("WASD / MOUSE", "MOVE / AIM"),
        ("LMB", "SHOOT"),
        ("R", "RELOAD"),
        ("SHIFT / SPACE", "SPRINT / DODGE ROLL"),
        ("G", "GRENADE AT THE CURSOR"),
        ("1-9, WHEEL, Q/E", "SWITCH WEAPON"),
        ("F", "FLASHLIGHT"),
        ("TAB", "ARMORY: GUNS, AMMO, GRENADES"),
        ("B", "BUILD MODE. DRAG TO LAY WALLS AND TRAPS"),
        ("U / T / X", "UPGRADE / REPAIR / SELL UNDER THE CURSOR"),
        ("ENTER", "NEXT WAVE EARLY, FOR A BONUS"),
        ("F5 / F9", "QUICKSAVE / QUICKLOAD"),
        ("P", "PLAYERS AND SERVER PANEL"),
        ("F11 / M / F3", "FULLSCREEN / MUSIC / PERFORMANCE"),
        ("ESC", "PAUSE. NETWORK GAMES DO NOT STOP"),
    ];
    let pw = (640.0 * s).min(ui.w - x - 10.0);
    ui.panel(x, y, pw, 8.0 * s + lines.len() as f32 * 15.0 * s);
    for (i, (k, v)) in lines.iter().enumerate() {
        let ry = y + 7.0 * s + i as f32 * 15.0 * s;
        ui.text_shadow(x + 8.0 * s, ry, 1.0 * s, k, ACCENT);
        ui.text_shadow(x + 170.0 * s, ry, 1.0 * s, v, WHITE);
    }
    y += 16.0 * s + lines.len() as f32 * 15.0 * s;
    ui.text_shadow(x, y, 1.0 * s, "IF THE REACTOR FALLS, IT IS OVER. BARRELS CHAIN. FIRE SPREADS. THE FROZEN SHATTER.", GREEN);
    y += 16.0 * s;
    back_item(app, ui, x, y, 300.0 * s, Page::Main);
}

/// Who is here, and for hosts the controls of the server.
fn server_panel(app: &mut App, ui: &mut Ui) {
    let (w, s) = (ui.w, ui.s);
    let hosting = app.is_host();
    let (roster, title_text, max): (Vec<RosterEntry>, String, u8) = match &app.session {
        Session::Host(sv) => (sv.roster(&app.world), sv.cfg.name.clone(), sv.cfg.max_players),
        Session::Client(c) => (c.roster.clone(), c.server_name.clone(), c.max_players),
        Session::Solo => return,
    };
    let online = roster.iter().filter(|r| r.connected).count();
    let pw = 430.0 * s;
    let rows = roster.len().max(1) as f32;
    let extra = if hosting { 118.0 * s } else { 20.0 * s };
    let (x, y) = (w - pw - 14.0 * s, 196.0 * s);
    ui.panel(x, y, pw, 42.0 * s + rows * 20.0 * s + extra);
    ui.text_shadow(x + 10.0 * s, y + 8.0 * s, 2.0 * s, &title_text.chars().take(22).collect::<String>(), ACCENT);
    ui.text_shadow(x + 10.0 * s, y + 28.0 * s, 1.0 * s, &format!("{online}/{max} PLAYERS   [P] CLOSE"), DIM);
    let mut kick = None;
    for (i, r) in roster.iter().enumerate() {
        let ry = y + 42.0 * s + i as f32 * 20.0 * s;
        let c = if !r.connected { DIM } else if r.index as usize == app.world.local { ACCENT } else { WHITE };
        ui.text_shadow(x + 10.0 * s, ry + 4.0 * s, 1.0 * s, &r.name, c);
        let status = if !r.connected {
            "AWAY".to_string()
        } else if r.is_host {
            "HOST".to_string()
        } else {
            format!("{} MS", r.ping_ms)
        };
        ui.text_shadow(x + 190.0 * s, ry + 4.0 * s, 1.0 * s, &status, c);
        if hosting && r.connected && !r.is_host && ui.button(x + pw - 74.0 * s, ry, 64.0 * s, 17.0 * s, "KICK", 1.0 * s, true) {
            kick = Some(r.index as usize);
        }
    }
    let mut ly = y + 46.0 * s + rows * 20.0 * s;
    if let Session::Host(sv) = &app.session {
        let ip = net::local_ip().unwrap_or_else(|| "?".into());
        ui.text_shadow(x + 10.0 * s, ly, 1.0 * s, &format!("JOIN AT {}:{}", ip, sv.port), GREEN);
        ly += 14.0 * s;
        ui.text_shadow(x + 10.0 * s, ly, 1.0 * s, &format!("SENDING {:.0} KB/S{}", sv.rate / 1024.0, if sv.cfg.password.is_empty() { "   NO PASSWORD" } else { "   PASSWORD SET" }), DIM);
        ly += 14.0 * s;
        if let Some(l) = sv.log.last() {
            ui.text_shadow(x + 10.0 * s, ly, 1.0 * s, &l.chars().take(44).collect::<String>(), DIM);
        }
        ly += 16.0 * s;
    }
    if hosting {
        if app.world.hold_timer && ui.button(x + 10.0 * s, ly, 200.0 * s, 26.0 * s, "START THE MATCH", 1.0 * s, true) {
            let _ = app.do_cmd(Command::NextWave);
        }
        if ui.button(x + 220.0 * s, ly, 200.0 * s, 26.0 * s, "SAVE NOW", 1.0 * s, true) {
            match app.save_game() {
                Ok(()) => app.note("GAME SAVED"),
                Err(e) => app.note(&e),
            }
        }
    }
    if let Some(i) = kick {
        if let Session::Host(sv) = &mut app.session {
            sv.kick(i, &mut app.world);
        }
    }
}

fn pause(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.5]);
    let multi = app.is_multiplayer();
    ui.text_center(w * 0.5, h * 0.2, 6.0 * s, if multi { "MENU" } else { "PAUSED" }, WHITE);
    if multi {
        ui.text_center(w * 0.5, h * 0.2 + 52.0 * s, 1.0 * s, "THE GAME IS STILL RUNNING", RED);
    }
    let (bw, bh) = (320.0 * s, 40.0 * s);
    let bx = (w - bw) * 0.5;
    let mut by = h * 0.34;
    let step = bh + 8.0 * s;
    if ui.button(bx, by, bw, bh, "RESUME", 2.0 * s, true) {
        app.screen = Screen::Playing;
    }
    by += step;
    let can_save = !app.is_client();
    if can_save {
        if ui.button(bx, by, bw, bh, "SAVE GAME", 2.0 * s, true) {
            match app.save_game() {
                Ok(()) => app.note("GAME SAVED"),
                Err(e) => app.note(&e),
            }
        }
        by += step;
    }
    let solo_std = !multi && !app.world.permadeath;
    if solo_std {
        let has = app.slot_info(Slot::Solo).is_some();
        if ui.button(bx, by, bw, bh, "LOAD LAST SAVE", 2.0 * s, has) {
            app.action = Some(Action::LoadLast);
        }
        by += step;
    }
    let music = app.settings.music > 0.0;
    if ui.button(bx, by, bw, bh, if music { "MUSIC: ON" } else { "MUSIC: OFF" }, 2.0 * s, true) {
        app.settings.music = if music { 0.0 } else { 0.8 };
        app.apply_volume();
    }
    by += step;
    let label = if app.is_client() { "DISCONNECT" } else { "SAVE AND QUIT" };
    if ui.button(bx, by, bw, bh, label, 2.0 * s, true) {
        app.action = Some(Action::ToTitle { save: !app.is_client() });
    }
    if let Some(note) = &app.ui_note {
        ui.text_center(w * 0.5, by + bh + 14.0 * s, 2.0 * s, &note.0, GREEN);
    }
    crosshair(ui);
}

fn game_over(app: &mut App, ui: &mut Ui) {
    let (w, h, s) = (ui.w, ui.h, ui.s);
    let k = (app.world.game_over_timer * 0.5).min(1.0);
    ui.rect(0.0, 0.0, w, h, [0.02, 0.0, 0.0, 0.55 * k]);
    let head = if app.world.permadeath { "THE END" } else { "OVERRUN" };
    ui.text_center(w * 0.5, h * 0.14, 8.0 * s, head, [0.95, 0.15, 0.1, k]);
    if app.save_erased {
        ui.text_center(w * 0.5, h * 0.14 + 66.0 * s, 2.0 * s, "PERMADEATH: THIS RUN'S SAVE HAS BEEN ERASED", [1.0, 0.45, 0.35, k]);
    }
    let st = app.world.stats;
    let lines = [
        format!("SURVIVED TO WAVE {}", st.wave_reached),
        format!("ZOMBIES KILLED {}", st.kills),
        format!("HEADSHOTS {}", st.headshots),
        format!("BEST STREAK {}", st.best_combo),
        format!("STRUCTURES BUILT {}", st.built),
        format!("SCRAP EARNED {}", st.scrap_earned),
        format!("TIMES FALLEN {}", st.deaths),
        format!("TIME {}", clock(st.time)),
    ];
    let mut y = h * 0.31;
    for l in &lines {
        ui.text_center(w * 0.5, y, 2.0 * s, l, [0.9, 0.88, 0.8, k]);
        y += 24.0 * s;
    }
    let (bw, bh) = (320.0 * s, 40.0 * s);
    let bx = (w - bw) * 0.5;
    y += 10.0 * s;
    let solo = !app.is_multiplayer();
    if solo {
        let perm = app.world.permadeath;
        if ui.button(bx, y, bw, bh, "AGAIN", 2.0 * s, true) {
            app.action = Some(Action::NewGame { permadeath: perm });
        }
        y += bh + 8.0 * s;
        if !perm && app.slot_info(Slot::Solo).is_some() {
            if ui.button(bx, y, bw, bh, "LOAD LAST SAVE", 2.0 * s, true) {
                app.action = Some(Action::LoadLast);
            }
            y += bh + 8.0 * s;
        }
    }
    let label = if app.is_client() { "DISCONNECT" } else { "QUIT TO TITLE" };
    if ui.button(bx, y, bw, bh, label, 2.0 * s, true) {
        app.action = Some(Action::ToTitle { save: false });
    }
    crosshair(ui);
}
