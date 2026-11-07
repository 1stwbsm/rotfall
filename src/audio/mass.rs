//! The Wall: the orchestra behind ROTFALL's night music, as one enormous block of sound.
//!
//! Seventy-two raw oscillators in three registers, stacked up the whole range, holding the harmony
//! rock steady. Nothing slides, shakes or wanders: the only things that move are how loud it is, how
//! far it reaches, how bright it burns, and how much of it has gone sour. They change together, on
//! the bar, because a mass this size is frightening exactly as long as it moves as one.
//!
//! `grit` turns a share of the voices over to the two notes that hurt most against any chord, the
//! semitone above the root and the tritone. A `tutti` slams the whole wall onto the beat, and `chop`
//! hands it to the machine, which lets it through only on the steps it says.
use super::dsp::*;

const VOICES: usize = 72;
const BLOCK: usize = 64;
const FAMILIES: usize = 3;

const LOW: std::ops::Range<usize> = 0..18;
const MID: std::ops::Range<usize> = 18..48;
const HIGH: std::ops::Range<usize> = 48..72;

/// What the wall is doing across a section. Pairs run from the section's start to its end.
#[derive(Clone, Copy)]
pub struct MassCue {
    /// How loud it is, 0..1. Louder is also fuller, and more distorted.
    pub level: (f32, f32),
    /// The lowest and highest notes it reaches, as MIDI numbers.
    pub lo: (f32, f32),
    pub hi: (f32, f32),
    /// How much of it has gone sour, 0..1.
    pub grit: (f32, f32),
    /// How bright it burns, 0..1.
    pub bite: (f32, f32),
    /// Steps on which the whole thing slams down.
    pub tutti: &'static str,
    /// On `.` steps the entire wall is cut off, and on anything else it is let through.
    pub chop: &'static str,
}

/// A `tutti` that happens once, at the very start of its section, and never again.
pub const ONCE: &str = "once";

/// Nothing at all.
pub const QUIET: MassCue = MassCue {
    level: (0.0, 0.0),
    lo: (36.0, 36.0),
    hi: (72.0, 72.0),
    grit: (0.0, 0.0),
    bite: (0.4, 0.4),
    tutti: "",
    chop: "",
};

#[derive(Clone, Copy, Default)]
struct V {
    phase: f32,
    dt: f32,
    amp: f32,
    step: f32,
    pl: f32,
    pr: f32,
    /// A tiny fixed detuning, in semitones, which is what thickens it. It never changes.
    detune: f32,
    /// How soon this voice goes sour as `grit` rises.
    sours_at: f32,
}

pub struct Mass {
    v: Vec<V>,
    buf: [[f32; BLOCK]; FAMILIES * 2],
    out_l: [f32; BLOCK],
    out_r: [f32; BLOCK],
    at: usize,
    filt: [[Svf; 2]; FAMILIES],
    high_cut: [Svf; 2],
    /// The sub-bass belongs to the kick. This is a wall, not a floor.
    floor: [Svf; 2],
    dc: [DcBlock; 2],
    want: Controls,
    now: Controls,
    /// The tutti's punch, fading away.
    hit: f32,
    /// How hard the game is pushing: the closer the horde, the sourer and brighter it gets.
    pub heat: f32,
    asleep: bool,
    /// Whether the machine is letting anything through, and how open that gate currently is.
    pub open: bool,
    gate: f32,
}

#[derive(Clone, Copy, Default)]
struct Controls {
    level: f32,
    lo: f32,
    hi: f32,
    grit: f32,
    bite: f32,
}

#[inline]
fn lerp(p: (f32, f32), k: f32) -> f32 {
    p.0 + (p.1 - p.0) * k
}

impl Mass {
    pub fn new(seed: u32) -> Self {
        let mut rng = seed | 1;
        let mut rand = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            (rng >> 8) as f32 / 16_777_216.0
        };
        let mut v = vec![V::default(); VOICES];
        for (i, voice) in v.iter_mut().enumerate() {
            // Everyone stands to one side or the other: the middle belongs to the kick, bass and riff.
            let side = if rand() < 0.5 { -1.0 } else { 1.0 };
            let pan = if LOW.contains(&i) { side * (0.1 + rand() * 0.3) } else { side * (0.35 + rand() * 0.65) };
            voice.phase = rand();
            voice.pl = 0.5 - pan * 0.5;
            voice.pr = 0.5 + pan * 0.5;
            voice.detune = (rand() - 0.5) * 0.07;
            voice.sours_at = rand();
            voice.dt = 0.01;
        }
        let start = Controls { lo: 36.0, hi: 72.0, bite: 0.4, ..Default::default() };
        Mass {
            v,
            buf: [[0.0; BLOCK]; FAMILIES * 2],
            out_l: [0.0; BLOCK],
            out_r: [0.0; BLOCK],
            at: BLOCK,
            filt: [[Svf::default(); 2]; FAMILIES],
            high_cut: [Svf::default(); 2],
            floor: [Svf::default(); 2],
            dc: [DcBlock::default(); 2],
            want: start,
            now: start,
            hit: 0.0,
            heat: 0.0,
            asleep: true,
            open: true,
            gate: 1.0,
        }
    }

    /// Follows a section's cue, `k` of the way through it.
    pub fn cue(&mut self, c: &MassCue, k: f32) {
        self.want = Controls { level: lerp(c.level, k), lo: lerp(c.lo, k), hi: lerp(c.hi, k), grit: lerp(c.grit, k), bite: lerp(c.bite, k) };
    }

    /// A new chord, on the bar: every voice moves at once, straight to its place in the stack.
    pub fn chord(&mut self, root: f32, tones: [f32; 4]) {
        let (lo, hi) = (self.want.lo, self.want.hi);
        let grit = (self.want.grit + self.heat * 0.1).min(1.0);
        let span = (hi - lo).max(1.0);
        let place = |range: &std::ops::Range<usize>, i: usize, from: f32, to: f32| {
            let k = (i - range.start) as f32 + 0.5;
            lo + span * (from + (to - from) * k / range.len() as f32)
        };
        for i in 0..VOICES {
            let (wish, low) = if LOW.contains(&i) {
                (place(&LOW, i, 0.0, 0.35), true)
            } else if MID.contains(&i) {
                (place(&MID, i, 0.25, 0.72), false)
            } else {
                (place(&HIGH, i, 0.6, 1.0), false)
            };
            let v = &mut self.v[i];
            // The nearest note of the chord to this voice's place in the stack. Down in the chest
            // that means only the root and fifth, because a third that low is just mud.
            let full = [tones[0], tones[1], tones[2]];
            let set: &[f32] = if v.sours_at < grit {
                &[1.0, 6.0]
            } else if low {
                &[0.0, 7.0]
            } else {
                &full
            };
            let mut best = (f32::MAX, wish);
            for t in set {
                let p = root + t;
                let q = p + 12.0 * ((wish - p) / 12.0).round();
                if (q - wish).abs() < best.0 {
                    best = ((q - wish).abs(), q);
                }
            }
            v.dt = 0.009_977_f32 * fexp((best.1 + v.detune - 69.0) * 0.057_762_27);
        }
    }

    /// Everyone, at once.
    pub fn tutti(&mut self) {
        self.hit = 1.0;
    }

    fn render_block(&mut self) {
        // The conductor's hand moves steadily, whatever the score asks for.
        let (w, n) = (self.want, &mut self.now);
        let ease = 0.012;
        n.level += (w.level - n.level) * ease;
        n.bite += (w.bite - n.bite) * ease;
        let c = self.now;
        self.hit *= 0.993;
        if c.level < 0.002 && self.hit < 0.002 {
            if !self.asleep {
                self.asleep = true;
                self.out_l = [0.0; BLOCK];
                self.out_r = [0.0; BLOCK];
                for v in &mut self.v {
                    v.amp = 0.0;
                }
            }
            return;
        }
        self.asleep = false;

        let level = c.level;
        let punch = 1.0 + self.hit * 1.6;
        // Louder is fuller: the chest speaks first, the top of the range only when pushed hard.
        let weight = [level.sqrt() * 0.6, level.powf(0.8) * 1.6, level * level * 1.25];
        const VOICE_GAIN: f32 = 0.066;

        self.buf = [[0.0; BLOCK]; FAMILIES * 2];
        for i in 0..VOICES {
            let fam = if LOW.contains(&i) { 0 } else if MID.contains(&i) { 1 } else { 2 };
            let v = &mut self.v[i];
            let amp_to = VOICE_GAIN * weight[fam] * punch;
            v.step = (amp_to - v.amp) / BLOCK as f32;
            let (mut ph, dt, mut a, st, pl, pr) = (v.phase, v.dt, v.amp, v.step, v.pl, v.pr);
            let (left, right) = self.buf.split_at_mut(fam * 2 + 1);
            let (l, r) = (&mut left[fam * 2], &mut right[0]);
            for k in 0..BLOCK {
                ph += dt;
                if ph >= 1.0 {
                    ph -= 1.0;
                }
                let s = saw(ph, dt) * a;
                a += st;
                l[k] += s * pl;
                r[k] += s * pr;
            }
            v.phase = ph;
            v.amp = a;
        }

        let bite = (c.bite + self.hit * 0.25 + self.heat * 0.08).min(1.0);
        let b2 = bite * bite;
        let coefs = [
            SvfCoef::new(150.0 + bite * 800.0, 0.8),
            SvfCoef::new(420.0 + b2 * 3600.0, 0.75),
            SvfCoef::new(1300.0 + b2 * 4500.0, 0.75),
        ];
        let high = SvfCoef::new(900.0, 0.7);
        let floor = SvfCoef::new(100.0, 0.7);
        let drive = 1.0 + level * 1.7 + self.hit * 0.9;
        let makeup = 1.0 / (1.0 + level * 0.65);
        let gate_to: f32 = if self.open { 1.0 } else { 0.04 };
        let gate_step = (gate_to - self.gate) * 0.5 / BLOCK as f32;
        for k in 0..BLOCK {
            let (mut l, mut r) = (0.0, 0.0);
            for fam in 0..FAMILIES {
                let (x, y) = (self.buf[fam * 2][k], self.buf[fam * 2 + 1][k]);
                let mut a = self.filt[fam][0].tick(x, &coefs[fam]).lp;
                let mut b = self.filt[fam][1].tick(y, &coefs[fam]).lp;
                if fam == 2 {
                    a = self.high_cut[0].tick(a, &high).hp;
                    b = self.high_cut[1].tick(b, &high).hp;
                }
                l += a;
                r += b;
            }
            self.gate += gate_step;
            let l = self.floor[0].tick(soft_clip(l * drive), &floor).hp;
            let r = self.floor[1].tick(soft_clip(r * drive), &floor).hp;
            self.out_l[k] = self.dc[0].tick(l) * makeup * self.gate;
            self.out_r[k] = self.dc[1].tick(r) * makeup * self.gate;
        }
    }

    /// The next stereo frame.
    #[inline]
    pub fn next(&mut self) -> (f32, f32) {
        if self.at >= BLOCK {
            self.at = 0;
            self.render_block();
        }
        let f = (self.out_l[self.at], self.out_r[self.at]);
        self.at += 1;
        f
    }
}
