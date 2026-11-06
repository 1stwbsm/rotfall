//! Touchscreen play: twin virtual sticks and on-screen buttons. Elsewhere, such as in menus,
//! taps simply act as a mouse, so all the normal UI works by touch without knowing about it.
use super::app::{App, RawInput, Screen};
use glam::{vec3, Vec2};
use std::collections::VecDeque;
use winit::keyboard::KeyCode as K;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchPhase {
    Start,
    Move,
    End,
}

#[derive(Clone, Copy, Debug)]
pub struct TouchEv {
    pub id: u64,
    pub phase: TouchPhase,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Stick {
    pub id: u64,
    pub origin: (f32, f32),
    pub cur: (f32, f32),
}

impl Stick {
    /// -1..1 on each axis, with length at most 1.
    pub fn value(&self, radius: f32) -> Vec2 {
        (Vec2::new(self.cur.0 - self.origin.0, self.cur.1 - self.origin.1) / radius.max(1.0)).clamp_length_max(1.0)
    }
}

#[derive(Clone, Copy)]
enum MouseOp {
    Move(f32, f32),
    Press,
    Release,
}

#[derive(Default)]
pub struct TouchState {
    pub enabled: bool,
    /// The "touch controls are on" notice is waiting for an answer.
    pub prompt: bool,
    pub left: Option<Stick>,
    pub right: Option<Stick>,
    pub radius: f32,
    mouse: Option<u64>,
    ops: VecDeque<MouseOp>,
    last_aim: Vec2,
}

pub struct TouchButton {
    pub rect: [f32; 4],
    pub label: &'static str,
    pub key: K,
}

/// Which on-screen buttons exist right now.
pub fn buttons(app: &App, size: (u32, u32)) -> Vec<TouchButton> {
    let (w, s) = (size.0 as f32, super::hud::ui_scale(app, size));
    let (bw, bh, gap) = (84.0 * s, 34.0 * s, 40.0 * s);
    let place = |x: f32, y: f32, label: &'static str, key: K| TouchButton { rect: [x, y, bw, bh], label, key };
    let left = 14.0 * s;
    let top = 86.0 * s;
    let across = bw + 6.0 * s;
    if app.build_sel.is_some() {
        // In a row above the build bar, which fills the lower part of the screen.
        return [("SELL", K::KeyX), ("UPGRADE", K::KeyU), ("REPAIR", K::KeyT), ("DONE", K::KeyB)]
            .iter()
            .enumerate()
            .map(|(i, &(label, key))| place(left + i as f32 * across, top, label, key))
            .collect();
    }
    let mut v: Vec<TouchButton> = [("BUILD", K::KeyB), ("ARMORY", K::Tab), ("WAVE", K::Enter), ("MENU", K::Escape)]
        .iter()
        .enumerate()
        .map(|(i, &(label, key))| place(left, top + i as f32 * gap, label, key))
        .collect();
    // A block of four between the minimap and the weapon panel.
    let right = w - 14.0 * s - bw;
    for (i, &(label, key)) in [("GRENADE", K::KeyG), ("RELOAD", K::KeyR), ("ROLL", K::Space), ("SWAP", K::KeyE)].iter().enumerate() {
        v.push(place(right - (i % 2) as f32 * across, 192.0 * s + (i / 2) as f32 * gap, label, key));
    }
    v
}

impl App {
    /// True when the sticks and buttons are in charge of the screen.
    pub fn touch_play(&self) -> bool {
        self.touch.enabled && !self.touch.prompt && self.screen == Screen::Playing && !self.armory_open && !self.help_open && !self.panel_open
    }

    /// Decides whether this looks like a device that wants touch controls. `force` is for testing.
    pub fn detect_device(&mut self, screen: (f32, f32), touch_capable: bool, force: bool) {
        let short = screen.0.min(screen.1);
        let looks_touch = force || short <= 500.0 || (touch_capable && short <= 1060.0);
        match self.settings.touch.as_str() {
            "off" if !force => self.touch.enabled = false,
            "on" => self.touch.enabled = true,
            _ => {
                self.touch.enabled = looks_touch;
                self.touch.prompt = looks_touch;
            }
        }
    }

    /// Ok: keep the controls and stop asking. Disable: turn them off, also for good.
    pub fn answer_touch_prompt(&mut self, keep: bool) {
        self.touch.prompt = false;
        self.touch.enabled = keep;
        self.settings.touch = if keep { "on" } else { "off" }.to_string();
        self.store_settings();
    }

    /// Turns this frame's touches into stick positions, key presses or mouse actions.
    pub fn begin_frame(&mut self, raw: &mut RawInput, size: (u32, u32)) {
        let events = std::mem::take(&mut raw.touches);
        self.touch.radius = 66.0 * super::hud::ui_scale(self, size);
        let playing = self.touch_play();
        if !playing {
            self.touch.left = None;
            self.touch.right = None;
        }
        let buttons = if playing { buttons(self, size) } else { Vec::new() };
        let building = self.build_sel.is_some();
        let (w, h) = (size.0 as f32, size.1 as f32);
        for e in events {
            let t = &mut self.touch;
            match e.phase {
                TouchPhase::Start => {
                    if playing {
                        let hit = buttons.iter().find(|b| e.x >= b.rect[0] && e.x < b.rect[0] + b.rect[2] && e.y >= b.rect[1] && e.y < b.rect[1] + b.rect[3]);
                        if let Some(b) = hit {
                            raw.pressed.push(b.key);
                            continue;
                        }
                        let stick = Stick { id: e.id, origin: (e.x, e.y), cur: (e.x, e.y) };
                        if building {
                            // The world is for placing things; only the corner still walks.
                            if e.x < w * 0.3 && e.y > h * 0.45 {
                                t.left = Some(stick);
                                continue;
                            }
                        } else if e.x < w * 0.5 {
                            t.left = Some(stick);
                            continue;
                        } else {
                            t.right = Some(stick);
                            continue;
                        }
                    }
                    if t.mouse.is_none() {
                        t.mouse = Some(e.id);
                        t.ops.push_back(MouseOp::Move(e.x, e.y));
                        t.ops.push_back(MouseOp::Press);
                    }
                }
                TouchPhase::Move => {
                    if let Some(s) = t.left.as_mut().filter(|s| s.id == e.id) {
                        s.cur = (e.x, e.y);
                    } else if let Some(s) = t.right.as_mut().filter(|s| s.id == e.id) {
                        s.cur = (e.x, e.y);
                    } else if t.mouse == Some(e.id) {
                        t.ops.push_back(MouseOp::Move(e.x, e.y));
                    }
                }
                TouchPhase::End => {
                    if t.left.is_some_and(|s| s.id == e.id) {
                        t.left = None;
                    } else if t.right.is_some_and(|s| s.id == e.id) {
                        t.right = None;
                    } else if t.mouse == Some(e.id) {
                        t.mouse = None;
                        t.ops.push_back(MouseOp::Release);
                    }
                }
            }
        }
        // A press waits a frame after its move, so the UI has seen where the finger is by the
        // time it is asked whether that spot was clicked.
        let mut moved = false;
        while let Some(op) = self.touch.ops.front().copied() {
            match op {
                MouseOp::Move(x, y) => {
                    raw.mouse = (x, y);
                    moved = true;
                }
                MouseOp::Press if !moved => {
                    raw.lmb = true;
                    raw.lmb_pressed = true;
                }
                MouseOp::Release if !moved => raw.lmb = false,
                _ => break,
            }
            self.touch.ops.pop_front();
            if !matches!(op, MouseOp::Move(..)) {
                break;
            }
        }
    }

    /// Replaces the mouse and keyboard parts of this frame's input with the sticks.
    pub fn apply_touch(&mut self, input: &mut super::player::PlayerInput) {
        let t = &mut self.touch;
        if let Some(s) = t.left {
            let v = s.value(t.radius);
            input.move_dir = v;
            input.sprint = v.length() > 0.92;
        }
        let at = self.world.me().pos;
        if let Some(s) = t.right {
            let v = s.value(t.radius);
            if v.length() > 0.15 {
                t.last_aim = v.normalize();
            }
            input.fire = v.length() > 0.5;
        } else {
            input.fire = false;
        }
        if t.last_aim == Vec2::ZERO {
            t.last_aim = Vec2::new(0.0, -1.0);
        }
        input.aim = at + vec3(t.last_aim.x, 0.0, t.last_aim.y) * 10.0 + vec3(0.0, 1.2, 0.0);
    }
}
