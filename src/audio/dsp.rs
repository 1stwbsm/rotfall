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
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
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

/// A plate-style reverb: a feedback delay network whose eight lines mix into each other, so the
/// tail thickens smoothly rather than ringing like a row of separate echoes.
pub struct Reverb {
    lines: Vec<Delay>,
    damp: [f32; 8],
    diffuse: Vec<Delay>,
}

impl Reverb {
    pub fn new() -> Self {
        let lines = [1913, 2251, 2647, 3083, 3491, 3967, 4421, 4943].iter().map(|&t| Delay::new(t)).collect();
        let diffuse = [331, 557, 809].iter().map(|&t| Delay::new(t)).collect();
        Reverb { lines, damp: [0.0; 8], diffuse }
    }

    /// Empties the reverb at once.
    pub fn clear(&mut self) {
        self.damp = [0.0; 8];
        self.lines.iter_mut().chain(&mut self.diffuse).for_each(Delay::clear);
    }

    #[inline]
    pub fn process(&mut self, x: f32, room: f32, damp: f32) -> (f32, f32) {
        // All-passes smear the input, so that no one attack excites a single echo.
        let mut input = x;
        for ap in &mut self.diffuse {
            let b = ap.tap();
            ap.push(input + b * 0.55);
            input = b - input * 0.55;
        }

        let mut out = [0.0f32; 8];
        let mut sum = 0.0;
        for (o, line) in out.iter_mut().zip(&self.lines) {
            *o = line.tap();
            sum += *o;
        }
        // A Householder matrix: every line feeds every other, with no energy gained or lost.
        let mix = sum * 0.25;
        for i in 0..8 {
            let v = (out[i] - mix) * room;
            self.damp[i] = v * (1.0 - damp) + self.damp[i] * damp;
            self.lines[i].push(self.damp[i] + input * 0.35);
        }
        let l = out[0] + out[2] + out[4] + out[6];
        let r = out[1] + out[3] + out[5] + out[7];
        (l * 0.2, r * 0.2)
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
