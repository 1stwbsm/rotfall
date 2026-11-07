//! All sound effects are synthesized at startup; no audio assets ship with the game.
use super::dsp::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum Sfx {
    Pistol, Smg, Rifle, Shotgun, Sniper, Lmg, Minigun, Launcher, Rocket, Flame, Tesla, Beam, Saw,
    Rail, Magnum, Cryo, Sentry, SentryHeavy,
    ExplosionBig, ExplosionSmall, Fleshy1, Fleshy2, Fleshy3, Ricochet, Casing,
    ReloadStart, ReloadEnd, Empty, Build, Sell, WallHit, WallBreak, Upgrade,
    Groan1, Groan2, Groan3, Groan4, Attack, Death1, Death2, BruteRoar, Spit, Scream, Pop, Boss,
    Click, Horn, Clear, Pickup, Hurt, PlayerDeath, Dash, Buy, Denied,
    // Menu sounds never cross the network, so adding them does not affect the protocol.
    MenuHover, MenuSelect, MenuBack, MenuToggle, MenuTick, MenuConfirm, MenuError,
}
pub const SFX_COUNT: usize = Sfx::MenuError as usize + 1;

const ALL: [Sfx; SFX_COUNT] = {
    use Sfx::*;
    [
        Pistol, Smg, Rifle, Shotgun, Sniper, Lmg, Minigun, Launcher, Rocket, Flame, Tesla, Beam, Saw,
        Rail, Magnum, Cryo, Sentry, SentryHeavy,
        ExplosionBig, ExplosionSmall, Fleshy1, Fleshy2, Fleshy3, Ricochet, Casing,
        ReloadStart, ReloadEnd, Empty, Build, Sell, WallHit, WallBreak, Upgrade,
        Groan1, Groan2, Groan3, Groan4, Attack, Death1, Death2, BruteRoar, Spit, Scream, Pop, Boss,
        Click, Horn, Clear, Pickup, Hurt, PlayerDeath, Dash, Buy, Denied,
        MenuHover, MenuSelect, MenuBack, MenuToggle, MenuTick, MenuConfirm, MenuError,
    ]
};

impl Sfx {
    pub fn from_u8(v: u8) -> Option<Sfx> {
        ALL.get(v as usize).copied()
    }
}

struct G {
    n: Noise,
    f: Svf,
    f2: Svf,
}

impl G {
    fn new(seed: u32) -> Self {
        G { n: Noise(seed * 2 + 1), f: Svf::default(), f2: Svf::default() }
    }
}

fn len(s: f32) -> usize {
    (s * SR) as usize
}

/// A gunshot: sharp click, body thump, filtered noise tail.
fn gunshot(seed: u32, dur: f32, thump: f32, body: f32, tail_cut: f32, gain: f32) -> Vec<f32> {
    let mut g = G::new(seed);
    let mut ph = 0.0f32;
    (0..len(dur))
        .map(|i| {
            let t = i as f32 / SR;
            let e = g.n.next();
            let crack = e * (-t * 300.0).exp();
            let tail = g.f.process(e, tail_cut * (0.25 + (-t * 18.0).exp()), 0.8).lp * (-t * (5.0 / dur)).exp();
            ph += (thump * (1.0 + 2.0 * (-t * 60.0).exp())) / SR;
            let low = (ph * std::f32::consts::TAU).sin() * (-t * 28.0).exp();
            soft_clip((crack * 0.7 + tail * 1.1 + low * body) * gain)
        })
        .collect()
}

fn explosion(seed: u32, dur: f32, cut: f32) -> Vec<f32> {
    let mut g = G::new(seed);
    let (mut ph, mut rumble) = (0.0f32, 0.0f32);
    (0..len(dur))
        .map(|i| {
            let t = i as f32 / SR;
            let e = g.n.next();
            rumble += (e - rumble) * 0.02;
            let body = g.f.process(e, cut * (-t * 3.5).exp() + 60.0, 0.9).lp;
            ph += (32.0 + 90.0 * (-t * 9.0).exp()) / SR;
            let sub = (ph * std::f32::consts::TAU).sin() * (-t * 3.2).exp();
            let crack = e * (-t * 70.0).exp();
            soft_clip((body * 1.8 * (-t * (4.0 / dur)).exp() + sub * 1.3 + crack * 0.8 + rumble * 2.0 * (-t * 2.2).exp()) * 1.2)
        })
        .collect()
}

/// A zombie vocalization: formant filtered pulse with a wobbling pitch.
fn voice(seed: u32, dur: f32, f0: f32, f1: f32, formants: (f32, f32), rough: f32, attack: f32) -> Vec<f32> {
    let mut g = G::new(seed);
    let mut ph = 0.0f32;
    let wob = 3.0 + (seed % 5) as f32;
    (0..len(dur))
        .map(|i| {
            let t = i as f32 / SR;
            let k = t / dur;
            let e = g.n.next();
            let fr = (f0 + (f1 - f0) * k) * (1.0 + 0.08 * (t * wob * 6.28).sin() + e * rough * 0.2);
            ph += fr / SR;
            if ph >= 1.0 {
                ph -= 1.0;
            }
            let src = saw(ph, fr / SR) + e * rough * 0.6;
            let a = g.f.process(src, formants.0 * (1.0 + 0.1 * (t * 2.1 * 6.28).sin()), 5.0).bp;
            let b = g.f2.process(src, formants.1, 6.0).bp;
            let env = (t / attack).min(1.0) * ((dur - t) / (dur * 0.35)).clamp(0.0, 1.0);
            soft_clip((a + b * 0.6) * env * 1.5)
        })
        .collect()
}

fn thud(seed: u32, dur: f32, freq: f32, noise: f32) -> Vec<f32> {
    let mut g = G::new(seed);
    let mut ph = 0.0f32;
    (0..len(dur))
        .map(|i| {
            let t = i as f32 / SR;
            ph += (freq * (1.0 + (-t * 40.0).exp())) / SR;
            let e = g.f.process(g.n.next(), 1600.0, 0.8).lp;
            ((ph * 6.283).sin() * (-t * (6.0 / dur)).exp() + e * noise * (-t * (10.0 / dur)).exp()) * 0.9
        })
        .collect()
}

fn tones(notes: &[(f32, f32, f32)], wave: u8) -> Vec<f32> {
    let total = notes.iter().map(|n| n.0 + n.1).fold(0.0, f32::max) + 0.25;
    let mut out = vec![0.0; len(total)];
    for &(start, dur, hz) in notes {
        let s = len(start);
        let mut ph = 0.0f32;
        for i in 0..len(dur + 0.2) {
            let t = i as f32 / SR;
            ph = (ph + hz / SR).fract();
            let v = match wave {
                0 => sine(ph),
                1 => pulse(ph, hz / SR, 0.5) * 0.5,
                _ => saw(ph, hz / SR) * 0.5,
            };
            let env = (t / 0.005).min(1.0) * (-t * (3.5 / dur)).exp();
            if s + i < out.len() {
                out[s + i] += v * env * 0.45;
            }
        }
    }
    out
}

fn metal(seed: u32, dur: f32, freqs: &[f32]) -> Vec<f32> {
    let mut g = G::new(seed);
    (0..len(dur))
        .map(|i| {
            let t = i as f32 / SR;
            let mut s = g.n.next() * (-t * 220.0).exp() * 0.6;
            for (k, f) in freqs.iter().enumerate() {
                s += (t * f * 6.283).sin() * (-t * (5.0 + 3.0 * k as f32) * (0.25 / dur)).exp() * 0.25;
            }
            s
        })
        .collect()
}

fn generate(s: Sfx) -> Vec<f32> {
    use Sfx::*;
    let id = s as u32 + 3;
    match s {
        Pistol => gunshot(id, 0.22, 150.0, 0.55, 3000.0, 0.8),
        Smg => gunshot(id, 0.13, 170.0, 0.35, 3800.0, 0.62),
        Rifle => gunshot(id, 0.26, 120.0, 0.7, 2700.0, 0.82),
        Shotgun => gunshot(id, 0.42, 80.0, 1.2, 1900.0, 1.1),
        Sniper => gunshot(id, 0.75, 70.0, 1.3, 2300.0, 1.2),
        Lmg => gunshot(id, 0.25, 100.0, 0.9, 2300.0, 0.85),
        Minigun => gunshot(id, 0.09, 140.0, 0.6, 3300.0, 0.6),
        Magnum => gunshot(id, 0.5, 90.0, 1.2, 2100.0, 1.15),
        Sentry => gunshot(id, 0.14, 180.0, 0.35, 4200.0, 0.42),
        SentryHeavy => gunshot(id, 0.3, 90.0, 0.95, 2400.0, 0.7),
        Launcher => {
            let mut v = thud(id, 0.26, 95.0, 0.9);
            v.iter_mut().for_each(|x| *x *= 1.1);
            v
        }
        Rocket => {
            let mut g = G::new(id);
            (0..len(0.75))
                .map(|i| {
                    let t = i as f32 / SR;
                    let e = g.n.next();
                    let b = g.f.process(e, 500.0 + 3000.0 * (t * 3.0).min(1.0), 1.2).bp;
                    (b * 1.5 + e * (-t * 40.0).exp()) * (-t * 3.2).exp()
                })
                .collect()
        }
        Flame => {
            let mut g = G::new(id);
            (0..len(0.32))
                .map(|i| {
                    let t = i as f32 / SR;
                    let e = g.n.next();
                    let b = g.f.process(e, 900.0 + 400.0 * (t * 40.0).sin(), 0.8).lp;
                    b * 1.1 * (t / 0.03).min(1.0) * ((0.32 - t) / 0.1).clamp(0.0, 1.0)
                })
                .collect()
        }
        Tesla => {
            let mut g = G::new(id);
            let mut ph = 0.0f32;
            (0..len(0.3))
                .map(|i| {
                    let t = i as f32 / SR;
                    let e = g.n.next();
                    ph = (ph + (2200.0 + 2500.0 * e.abs()) / SR).fract();
                    let v = if ph < 0.5 { 1.0 } else { -1.0 };
                    let gate = if (t * 170.0).fract() < 0.55 { 1.0 } else { 0.2 };
                    g.f.process(v + e, 5200.0, 1.5).bp * gate * (-t * 8.5).exp() * 1.0
                })
                .collect()
        }
        Beam => {
            let mut g = G::new(id);
            let (mut a, mut b) = (0.0f32, 0.0f32);
            (0..len(0.2))
                .map(|i| {
                    let t = i as f32 / SR;
                    a = (a + 610.0 / SR).fract();
                    b = (b + 1227.0 / SR).fract();
                    let v = saw(a, 610.0 / SR) + saw(b, 1227.0 / SR) * 0.6 + g.n.next() * 0.12;
                    g.f.process(v, 2600.0, 3.0).bp * 0.6 * (t / 0.02).min(1.0) * ((0.2 - t) / 0.06).clamp(0.0, 1.0)
                })
                .collect()
        }
        Saw => metal(id, 0.3, &[1800.0, 2710.0, 3970.0]),
        Rail => {
            let mut v = gunshot(id, 0.6, 65.0, 1.3, 5000.0, 1.0);
            let z = tones(&[(0.0, 0.32, 1900.0), (0.03, 0.26, 3100.0)], 2);
            for (a, b) in v.iter_mut().zip(z) {
                *a += b * 0.9;
            }
            v
        }
        Cryo => {
            let mut g = G::new(id);
            (0..len(0.25))
                .map(|i| {
                    let t = i as f32 / SR;
                    let e = g.n.next();
                    g.f.process(e, 5200.0 - 2800.0 * t * 4.0, 2.2).bp * (t / 0.02).min(1.0) * ((0.25 - t) / 0.1).clamp(0.0, 1.0)
                })
                .collect()
        }
        ExplosionBig => explosion(id, 1.9, 2200.0),
        ExplosionSmall => explosion(id, 0.9, 3000.0),
        Fleshy1 => thud(id, 0.11, 110.0, 1.0),
        Fleshy2 => thud(id, 0.14, 90.0, 1.1),
        Fleshy3 => thud(id, 0.09, 150.0, 1.2),
        Ricochet => {
            let mut ph = 0.0f32;
            (0..len(0.3))
                .map(|i| {
                    let t = i as f32 / SR;
                    ph = (ph + (3600.0 - 5000.0 * t) / SR).fract();
                    sine(ph) * (-t * 10.0).exp() * 0.3
                })
                .collect()
        }
        Casing => metal(id, 0.12, &[5300.0, 7200.0]),
        ReloadStart => metal(id, 0.14, &[900.0, 1700.0]),
        ReloadEnd => metal(id, 0.18, &[720.0, 1350.0, 2400.0]),
        Empty => metal(id, 0.06, &[2100.0]),
        Build => {
            let mut a = metal(id, 0.22, &[210.0, 430.0, 980.0]);
            a.iter_mut().for_each(|x| *x *= 1.2);
            a
        }
        Sell => tones(&[(0.0, 0.08, 700.0), (0.07, 0.1, 420.0)], 1),
        WallHit => thud(id, 0.16, 170.0, 1.3),
        WallBreak => explosion(id, 0.55, 1500.0),
        Upgrade => tones(&[(0.0, 0.1, 520.0), (0.09, 0.1, 660.0), (0.18, 0.2, 880.0)], 1),
        Groan1 => voice(id, 1.05, 92.0, 70.0, (480.0, 900.0), 0.25, 0.2),
        Groan2 => voice(id, 0.85, 120.0, 82.0, (640.0, 1100.0), 0.3, 0.14),
        Groan3 => voice(id, 1.3, 78.0, 96.0, (380.0, 800.0), 0.35, 0.3),
        Groan4 => voice(id, 0.7, 140.0, 95.0, (720.0, 1300.0), 0.4, 0.1),
        Attack => voice(id, 0.34, 200.0, 120.0, (850.0, 1500.0), 0.55, 0.02),
        Death1 => voice(id, 0.55, 150.0, 60.0, (560.0, 1000.0), 0.45, 0.02),
        Death2 => voice(id, 0.7, 115.0, 48.0, (430.0, 900.0), 0.5, 0.02),
        BruteRoar => voice(id, 1.5, 62.0, 42.0, (320.0, 700.0), 0.6, 0.08),
        Spit => voice(id, 0.3, 260.0, 500.0, (1200.0, 2200.0), 0.7, 0.01),
        Scream => voice(id, 0.9, 520.0, 780.0, (1300.0, 2600.0), 0.3, 0.05),
        Pop => {
            let mut v = explosion(id, 0.5, 900.0);
            v.iter_mut().for_each(|x| *x *= 0.9);
            v
        }
        Boss => voice(id, 2.4, 44.0, 33.0, (260.0, 560.0), 0.7, 0.2),
        Click => tones(&[(0.0, 0.03, 1250.0)], 1),
        Horn => {
            let mut g = G::new(id);
            let (mut a, mut b) = (0.0f32, 0.0f32);
            (0..len(2.3))
                .map(|i| {
                    let t = i as f32 / SR;
                    let f = 98.0 + 10.0 * (t * 2.0).min(1.0);
                    a = (a + f / SR).fract();
                    b = (b + f * 1.498 / SR).fract();
                    let s = saw(a, f / SR) + saw(b, f * 1.5 / SR) * 0.7;
                    let v = g.f.process(s, 300.0 + 900.0 * (t * 1.5).min(1.0), 1.5).lp;
                    soft_clip(v * 1.5) * (t / 0.2).min(1.0) * ((2.3 - t) / 0.6).clamp(0.0, 1.0) * 0.8
                })
                .collect()
        }
        Clear => tones(&[(0.0, 0.18, 392.0), (0.15, 0.18, 523.0), (0.3, 0.18, 659.0), (0.45, 0.6, 784.0)], 2),
        Pickup => tones(&[(0.0, 0.04, 1400.0), (0.03, 0.05, 1900.0)], 0),
        Hurt => voice(id, 0.2, 240.0, 150.0, (800.0, 1500.0), 0.3, 0.01),
        PlayerDeath => voice(id, 1.1, 210.0, 70.0, (700.0, 1200.0), 0.3, 0.02),
        Dash => {
            let mut g = G::new(id);
            (0..len(0.22))
                .map(|i| {
                    let t = i as f32 / SR;
                    let e = g.n.next();
                    g.f.process(e, 600.0 + 3000.0 * (t * 14.0).sin().abs(), 1.3).bp * ((0.22 - t) / 0.2).clamp(0.0, 1.0)
                })
                .collect()
        }
        Buy => tones(&[(0.0, 0.06, 980.0), (0.05, 0.14, 1470.0)], 1),
        Denied => tones(&[(0.0, 0.1, 180.0), (0.1, 0.16, 140.0)], 1),
        // The menu speaks the soundtrack's language: its pluck, its stab and its A minor.
        MenuHover => {
            let mut v = vec![0.0; len(0.06)];
            clack(&mut v, 0.0, 3600.0, 3.0, 0.09, 170.0, id);
            add_pluck(&mut v, 0.0, midi_hz(88.0), 0.04);
            v
        }
        MenuSelect => {
            let mut v = vec![0.0; len(0.7)];
            clack(&mut v, 0.0, 2400.0, 1.2, 0.28, 120.0, id);
            knock(&mut v, 0.0, 150.0, 34.0, 0.32);
            add_pluck(&mut v, 0.0, midi_hz(69.0), 0.34);
            add_pluck(&mut v, 0.035, midi_hz(76.0), 0.3);
            v
        }
        MenuBack => {
            let mut v = vec![0.0; len(0.7)];
            clack(&mut v, 0.0, 1700.0, 1.2, 0.22, 130.0, id);
            knock(&mut v, 0.0, 115.0, 30.0, 0.26);
            add_pluck(&mut v, 0.0, midi_hz(76.0), 0.26);
            add_pluck(&mut v, 0.05, midi_hz(69.0), 0.28);
            v
        }
        MenuToggle => {
            let mut v = vec![0.0; len(0.14)];
            clack(&mut v, 0.0, 2100.0, 1.5, 0.3, 240.0, id);
            clack(&mut v, 0.028, 3300.0, 1.5, 0.22, 300.0, id + 1);
            knock(&mut v, 0.0, 190.0, 60.0, 0.2);
            v
        }
        MenuTick => {
            let mut v = vec![0.0; len(0.04)];
            clack(&mut v, 0.0, 2800.0, 2.0, 0.2, 260.0, id);
            v
        }
        MenuError => {
            let mut v = vec![0.0; len(0.3)];
            clack(&mut v, 0.0, 620.0, 1.0, 0.25, 90.0, id);
            knock(&mut v, 0.0, 74.0, 16.0, 0.5);
            knock(&mut v, 0.0, 78.0, 18.0, 0.32);
            v
        }
        MenuConfirm => synth_hit(id),
    }
}

/// The switch itself: a very short burst of band-limited noise.
fn clack(out: &mut [f32], start: f32, hz: f32, q: f32, gain: f32, decay: f32, seed: u32) {
    let (mut n, mut f) = (Noise(seed * 2 + 1), Svf::default());
    for (i, o) in out.iter_mut().skip(len(start)).enumerate() {
        let t = i as f32 / SR;
        *o += f.process(n.next(), hz, q).bp * (-t * decay).exp() * gain * 2.5;
    }
}

/// The weight behind a switch, felt more than heard.
fn knock(out: &mut [f32], start: f32, hz: f32, decay: f32, gain: f32) {
    let mut ph = 0.0f32;
    for (i, o) in out.iter_mut().skip(len(start)).enumerate() {
        let t = i as f32 / SR;
        ph = (ph + hz * (1.0 + 0.6 * (-t * 70.0).exp()) / SR).fract();
        *o += sine(ph) * (-t * decay).exp() * gain;
    }
}

/// The soundtrack's synth pluck: a saw and a pulse with the filter falling shut.
fn add_pluck(out: &mut [f32], start: f32, hz: f32, gain: f32) {
    let (mut p0, mut p1, mut f) = (0.0f32, 0.0f32, Svf::default());
    for (i, o) in out.iter_mut().skip(len(start)).enumerate() {
        let t = i as f32 / SR;
        let (d0, d1) = (hz / SR, hz * 2.003 / SR);
        p0 = (p0 + d0).fract();
        p1 = (p1 + d1).fract();
        let s = saw(p0, d0) + 0.4 * pulse(p1, d1, 0.4);
        let cut = 700.0 + 3000.0 * (-t * 10.0).exp();
        *o += f.process(s * 0.6, cut, 1.2).lp * (-t * 6.0).exp() * (t / 0.002).min(1.0) * gain;
    }
}

/// Starting a run: one huge saw stab on A minor and a sub drop, through the score's reverb.
fn synth_hit(seed: u32) -> Vec<f32> {
    let mut n = Noise(seed * 2 + 1);
    let (mut body, mut hall) = (Svf::default(), Reverb::new());
    const NOTES: [f32; 6] = [45.0, 57.0, 60.0, 64.0, 69.0, 72.0];
    let mut phases = [[0.0f32; 3]; NOTES.len()];
    for p in phases.iter_mut().flatten() {
        *p = n.next().abs();
    }
    let hz = NOTES.map(midi_hz);
    let mut sub = 0.0f32;
    let mut v: Vec<f32> = (0..len(1.9))
        .map(|i| {
            let t = i as f32 / SR;
            let mut s = 0.0;
            for (ps, &f) in phases.iter_mut().zip(&hz) {
                for (k, p) in ps.iter_mut().enumerate() {
                    let d = f * [0.992, 1.0, 1.009][k] / SR;
                    *p = (*p + d).fract();
                    s += saw(*p, d);
                }
            }
            let cut = 400.0 + 5200.0 * (-t * 11.0).exp();
            let stab = soft_clip(body.process(s * 0.09, cut, 1.3).lp * 1.7) * (t / 0.002).min(1.0) * (-t * 6.0).exp() * 0.5;
            sub = (sub + (45.0 + 70.0 * (-t * 20.0).exp()) / SR).fract();
            let dry = stab + soft_clip(sine(sub) * 1.5) * (-t * 4.0).exp() * 0.42;
            let (l, r) = hall.process(dry * 0.5, 0.82, 0.3);
            dry + (l + r) * 0.45
        })
        .collect();
    // No clicks when it runs out of samples.
    let fade = len(0.3);
    for (i, x) in v.iter_mut().rev().take(fade).enumerate() {
        *x *= i as f32 / fade as f32;
    }
    v
}

pub fn build_bank() -> Vec<Vec<f32>> {
    // Browsers get no threads here.
    #[cfg(target_arch = "wasm32")]
    return ALL.iter().map(|&s| generate(s)).collect();
    #[cfg(not(target_arch = "wasm32"))]
    build_bank_threaded()
}

#[cfg(not(target_arch = "wasm32"))]
fn build_bank_threaded() -> Vec<Vec<f32>> {
    let handles: Vec<_> = ALL
        .chunks(8)
        .map(|c| {
            let c = c.to_vec();
            std::thread::spawn(move || c.into_iter().map(generate).collect::<Vec<_>>())
        })
        .collect();
    let mut out = Vec::with_capacity(SFX_COUNT);
    for h in handles {
        out.extend(h.join().expect("sfx generation thread panicked"));
    }
    out
}
