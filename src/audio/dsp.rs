//! DSP building blocks shared by the music synth and the sound-effect generator.
pub const SR: f32 = 44100.0;

#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
pub fn saw(phase: f32, dt: f32) -> f32 {
    2.0 * phase - 1.0 - poly_blep(phase, dt)
}

#[inline]
pub fn pulse(phase: f32, dt: f32, width: f32) -> f32 {
    let mut p2 = phase + (1.0 - width);
    if p2 >= 1.0 {
        p2 -= 1.0;
    }
    let a = if phase < width { 1.0 } else { -1.0 };
    a + poly_blep(phase, dt) - poly_blep(p2, dt)
}

#[inline]
pub fn sine(phase: f32) -> f32 {
    (phase * std::f32::consts::TAU).sin()
}

#[inline]
pub fn soft_clip(x: f32) -> f32 {
    // Pade approximation of tanh: fast and smooth.
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// Topology-preserving state variable filter.
#[derive(Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    #[inline]
    pub fn process(&mut self, x: f32, cutoff: f32, q: f32) -> SvfOut {
        let g = (std::f32::consts::PI * (cutoff.clamp(20.0, SR * 0.45)) / SR).tan();
        let k = 1.0 / q.max(0.3);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        if !self.ic1.is_finite() || !self.ic2.is_finite() {
            self.ic1 = 0.0;
            self.ic2 = 0.0;
        }
        SvfOut { lp: v2, bp: v1, hp: x - k * v1 - v2 }
    }
}

pub struct Delay {
    buf: Vec<f32>,
    idx: usize,
    len: usize,
}

impl Delay {
    pub fn new(max: usize) -> Self {
        Delay { buf: vec![0.0; max.max(1)], idx: 0, len: max.max(1) }
    }
    pub fn set_len(&mut self, len: usize) {
        self.len = len.clamp(1, self.buf.len());
        if self.idx >= self.len {
            self.idx = 0;
        }
    }
    #[inline]
    pub fn tap(&self) -> f32 {
        self.buf[self.idx]
    }
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.idx] = x;
        self.idx += 1;
        if self.idx >= self.len {
            self.idx = 0;
        }
    }
}

/// Schroeder-style stereo reverb (parallel damped combs into series all-passes).
pub struct Reverb {
    combs: Vec<(Delay, f32)>,
    allpass: Vec<Delay>,
}

impl Reverb {
    pub fn new() -> Self {
        let tunings = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        let combs = tunings
            .iter()
            .enumerate()
            .map(|(i, &t)| (Delay::new(t + if i % 2 == 1 { 23 } else { 0 }), 0.0))
            .collect();
        let allpass = [556, 441, 579, 341].iter().map(|&t| Delay::new(t)).collect();
        Reverb { combs, allpass }
    }
    #[inline]
    pub fn process(&mut self, x: f32, room: f32, damp: f32) -> (f32, f32) {
        let (mut l, mut r) = (0.0, 0.0);
        for (i, (d, store)) in self.combs.iter_mut().enumerate() {
            let out = d.tap();
            *store = out * (1.0 - damp) + *store * damp;
            d.push(x + *store * room);
            if i % 2 == 0 {
                l += out;
            } else {
                r += out;
            }
        }
        for (i, ap) in self.allpass.iter_mut().enumerate() {
            let s = if i % 2 == 0 { &mut l } else { &mut r };
            let b = ap.tap();
            ap.push(*s + b * 0.5);
            *s = b - *s;
        }
        (l * 0.18, r * 0.18)
    }
}

/// White-noise source used throughout the synth (allocation-free, audio-thread safe).
#[derive(Clone, Copy)]
pub struct Noise(pub u32);
impl Noise {
    #[inline]
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as i32 as f32) / 2147483648.0
    }
}

pub fn midi_hz(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}
