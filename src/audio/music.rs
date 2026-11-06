//! The soundtrack: an original score written as note data and synthesized live.
//!
//! Each song is a loop of sections. A section lists one chord per bar and 16-step
//! patterns for drums, bass and arpeggio, plus a melody. All instruments always play,
//! but sit on layer buses whose gain tracks the game's intensity, so the arrangement
//! builds and thins out in time with the horde without ever skipping a beat.
use super::dsp::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Track {
    Menu,
    Build,
    CombatA,
    CombatB,
    CombatC,
    Boss,
    GameOver,
}

pub struct Section {
    pub chords: &'static str,
    pub kick: &'static str,
    pub snare: &'static str,
    pub hat: &'static str,
    pub bass: &'static str,
    pub arp: &'static str,
    pub stab: &'static str,
    /// Space separated `Note:sixteenths` pairs, `-` is a rest. Loops over the section.
    pub lead: &'static str,
    pub choir: &'static str,
    pub repeats: u32,
}

pub struct Song {
    pub bpm: f32,
    pub swing: f32,
    pub pad_cut: f32,
    pub bass_drive: f32,
    pub lead_inst: Inst,
    pub sections: &'static [Section],
}

const O: &str = "";

static MENU: &[Section] = &[
    Section {
        chords: "Am F C G", kick: O, snare: O, hat: O, bass: "0...............",
        arp: O, stab: O, choir: O,
        lead: "E5:8 A5:4 G5:4 F5:8 E5:8 G5:6 E5:2 C5:8 D5:8 B4:8", repeats: 2,
    },
    Section {
        chords: "Dm Am F E", kick: O, snare: O, hat: O, bass: "0.......5.......",
        arp: "0...2...1...2...", stab: O, choir: O,
        lead: "F5:4 E5:4 D5:8 C5:4 E5:4 A4:8 C5:4 D5:4 F5:8 E5:12 G#4:4", repeats: 1,
    },
];

static BUILD: &[Section] = &[
    Section {
        chords: "Em C G D",
        kick: "x.......x.......", snare: "....x.......x...", hat: "..x...x...x...x.",
        bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.", stab: O, choir: O,
        lead: "-:16 -:16 -:16 -:16", repeats: 2,
    },
    Section {
        chords: "Em C G D",
        kick: "x.......x.......", snare: "....x.......x...", hat: "..x...x...x...x.",
        bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.", stab: O, choir: O,
        lead: "B4:3 E5:3 G5:2 F#5:4 E5:4 E5:3 C5:3 E5:2 G5:8 D5:3 G5:3 B5:2 A5:4 G5:4 F#5:6 D5:2 A4:8",
        repeats: 2,
    },
    Section {
        chords: "Am C Em D",
        kick: "x...x...x...x...", snare: "....x.......x..x", hat: "x.x.x.x.x.x.x.x.",
        bass: "0.0.3.0.5.0.3.0.", arp: "4.3.2.1.0.1.2.3.", stab: O, choir: O,
        lead: "A5:4 C6:4 B5:4 A5:4 G5:6 E5:2 G5:8 B5:4 G5:4 E5:4 D5:4 F#5:12 -:4",
        repeats: 1,
    },
];

static COMBAT_A: &[Section] = &[
    Section {
        chords: "Dm Dm Bb C",
        kick: "x...x...x...x...", snare: "....x.......x...", hat: "..x...x...x...x.",
        bass: "0.00.00.0.00.03.", arp: "0.1.2.0.1.2.0.1.", stab: O, choir: O,
        lead: "-:16 -:16 -:16 -:16", repeats: 2,
    },
    Section {
        chords: "Dm Bb F C",
        kick: "x...x...x...x...", snare: "....x.......x.x.", hat: "x.x.x.x.x.x.x.xx",
        bass: "0.00.00.0.00.03.", arp: "0.1.2.3.4.3.2.1.", stab: "x..x..x.........", choir: O,
        lead: "D5:3 F5:3 A5:2 G5:4 F5:4 F5:3 D5:3 F5:2 A#5:8 A5:3 F5:3 C5:2 F5:4 A5:4 G5:6 E5:2 C5:8",
        repeats: 2,
    },
    Section {
        chords: "Gm Dm Bb A",
        kick: "x...x...x...x.x.", snare: "....x.......x..x", hat: "x.xxx.xxx.xxx.xx",
        bass: "0.0.00.30.0.00.5", arp: "2.1.0.1.2.3.4.3.", stab: "x.....x.....x...", choir: O,
        lead: "G5:2 A5:2 A#5:4 A5:2 G5:2 F5:4 A5:6 D5:2 F5:8 A#5:4 A5:4 G5:4 F5:4 E5:6 C#5:2 A4:8",
        repeats: 2,
    },
    Section {
        chords: "Dm Dm Bb A",
        kick: "x.x.x.x.x.x.x.x.", snare: "....x.......xxxx", hat: "xxxxxxxxxxxxxxxx",
        bass: "01010101010101.5", arp: "0123432101234321", stab: "x..x..x.x..x..x.", choir: "x",
        lead: "A5:2 D6:2 C6:2 A5:2 F5:2 A5:2 D6:4 A#5:2 D6:2 C6:2 A#5:2 F5:2 A#5:2 D6:4 A#5:2 F5:2 D5:2 F5:2 A#5:4 C6:4 C#6:8 A5:4 E5:4",
        repeats: 1,
    },
];

static COMBAT_B: &[Section] = &[
    Section {
        chords: "Fm Db Eb Cm",
        kick: "x.....x...x.....", snare: "....x.......x...", hat: "x.x.x.x.x.x.x.x.",
        bass: "0.0..0..0.0..3..", arp: "0120120120120120", stab: O, choir: O,
        lead: "-:16 -:16 -:16 -:16", repeats: 2,
    },
    Section {
        chords: "Fm Db Eb Cm",
        kick: "x.....x...x.....", snare: "....x.......x...", hat: "xxx.xxx.xxx.xxx.",
        bass: "0.0..0..0.0..3..", arp: "0120120120120120", stab: O, choir: O,
        lead: "C6:6 G#5:2 F5:8 C#6:6 A#5:2 F5:8 D#6:4 C#6:4 C6:4 G#5:4 G5:6 D#5:2 C5:8",
        repeats: 2,
    },
    Section {
        chords: "Db Ab Eb Fm",
        kick: "x.x...x...x.x...", snare: "....x.......x.xx", hat: "xxxxxxxxxxxxxxxx",
        bass: "00.0.3.00.0.5.0.", arp: "0123210123210123", stab: "x.....x.....x...", choir: O,
        lead: "F6:2 D#6:2 C#6:4 C6:2 C#6:2 G#5:4 D#6:2 C6:2 G#5:4 G5:2 G#5:2 D#5:4 A#5:2 C6:2 C#6:4 D#6:2 F6:2 G#6:4 F6:12 -:4",
        repeats: 2,
    },
];

static COMBAT_C: &[Section] = &[
    Section {
        chords: "Cm Cm Ab G",
        kick: "x..x..x.x.......", snare: "....x.......x...", hat: "x...x...x...x...",
        bass: "0..0..0.0...0.3.", arp: O, stab: "x..x..x.........", choir: "x",
        lead: "-:16 -:16 -:16 -:16", repeats: 2,
    },
    Section {
        chords: "Cm Ab Fm G",
        kick: "x..x..x.x..x..x.", snare: "....x.......x..x", hat: "x.x.x.x.x.x.x.x.",
        bass: "0..0..0.0.03.05.", arp: "0.2.1.2.0.2.1.2.", stab: "x..x..x.........", choir: "x",
        lead: "C5:8 D#5:4 F5:4 G5:12 D#5:4 G#5:8 G5:4 F5:4 D5:12 B4:4",
        repeats: 2,
    },
    Section {
        chords: "Ab Eb Fm G",
        kick: "x.x.x..xx.x.x..x", snare: "....x.......x.xx", hat: "xxx.xxx.xxx.xxx.",
        bass: "00.0.0.300.0.0.5", arp: "0123012301230123", stab: "x.x...x.x.x...x.", choir: "x",
        lead: "C6:4 A#5:4 G#5:4 G5:4 G5:4 F5:4 D#5:8 F5:4 G5:4 G#5:4 A#5:4 B5:8 G5:8",
        repeats: 2,
    },
];

static BOSS: &[Section] = &[
    Section {
        chords: "Em F Em C",
        kick: "x.x.x.x.x.x.x.x.", snare: "....x.......x...", hat: "xxxxxxxxxxxxxxxx",
        bass: "0.00.00.0.00.01.", arp: "0120120120120120", stab: "x..x..x.........", choir: "x",
        lead: "E5:4 F5:4 G5:4 F5:4 E5:12 B4:4 C5:4 D5:4 E5:4 F5:4 G5:8 E5:8",
        repeats: 2,
    },
    Section {
        chords: "Am F Em Em",
        kick: "x.xxx.x.x.xxx.x.", snare: "....x..x....x.xx", hat: "xxxxxxxxxxxxxxxx",
        bass: "00.00.0.00.00.15", arp: "0123432101234321", stab: "x.x.x.x.x.x.xxxx", choir: "x",
        lead: "A5:2 B5:2 C6:4 B5:2 A5:2 F5:4 A5:2 C6:2 F6:4 E6:8 G5:2 B5:2 E6:4 D#6:4 E6:4 B5:2 G5:2 E5:8 -:4",
        repeats: 2,
    },
    Section {
        chords: "C F Dm E",
        kick: "x.x.x.x.x.x.x.x.", snare: "x...x...x...xxxx", hat: "xxxxxxxxxxxxxxxx",
        bass: "01010101010101.5", arp: "0123012301230123", stab: "x..x..x.x..x..x.", choir: "x",
        lead: "G5:4 C6:4 E6:8 F6:4 E6:4 C6:8 D6:4 C6:4 A5:8 G#5:4 B5:4 E6:8",
        repeats: 1,
    },
];

static GAMEOVER: &[Section] = &[Section {
    chords: "Dm Bb Gm A", kick: O, snare: O, hat: O, bass: "0...............",
    arp: O, stab: O, choir: "x",
    lead: "A4:16 F5:12 D5:4 G5:8 F5:8 E5:12 C#5:4", repeats: 2,
}];

pub fn song(t: Track) -> Song {
    let mk = |bpm, swing, pad_cut, bass_drive, lead_inst, sections| Song {
        bpm, swing, pad_cut, bass_drive, lead_inst, sections,
    };
    match t {
        Track::Menu => mk(66.0, 0.0, 900.0, 1.0, Inst::Bell, MENU),
        Track::Build => mk(96.0, 0.12, 1500.0, 1.2, Inst::Pluck, BUILD),
        Track::CombatA => mk(128.0, 0.0, 2200.0, 2.5, Inst::Lead, COMBAT_A),
        Track::CombatB => mk(140.0, 0.0, 2600.0, 2.0, Inst::Lead, COMBAT_B),
        Track::CombatC => mk(116.0, 0.06, 1700.0, 3.5, Inst::Lead, COMBAT_C),
        Track::Boss => mk(158.0, 0.0, 3200.0, 4.5, Inst::Lead, BOSS),
        Track::GameOver => mk(58.0, 0.0, 700.0, 1.0, Inst::Bell, GAMEOVER),
    }
}

/// Instruments. Each maps to a mix layer via `bus()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inst {
    Pad,
    Bass,
    Arp,
    Lead,
    Pluck,
    Bell,
    Stab,
    Choir,
    Kick,
    Snare,
    Hat,
    OpenHat,
    Crash,
}

const BUS_PAD: usize = 0;
const BUS_BASS: usize = 1;
const BUS_ARP: usize = 2;
const BUS_LEAD: usize = 3;
const BUS_KICK: usize = 4;
const BUS_PERC: usize = 5;
const BUS_STAB: usize = 6;
const BUSES: usize = 7;

fn bus_of(i: Inst) -> usize {
    match i {
        Inst::Pad | Inst::Choir => BUS_PAD,
        Inst::Bass => BUS_BASS,
        Inst::Arp => BUS_ARP,
        Inst::Lead | Inst::Pluck | Inst::Bell => BUS_LEAD,
        Inst::Stab => BUS_STAB,
        Inst::Kick => BUS_KICK,
        Inst::Snare | Inst::Hat | Inst::OpenHat | Inst::Crash => BUS_PERC,
    }
}

#[derive(Clone)]
struct Voice {
    inst: Inst,
    freq: f32,
    vel: f32,
    t: f32,
    gate: f32,
    phase: [f32; 5],
    filt: Svf,
    filt2: Svf,
    noise: Noise,
    pan: f32,
    drive: f32,
    cut: f32,
}

#[inline]
fn adsr(t: f32, gate: f32, a: f32, d: f32, s: f32, r: f32) -> f32 {
    let held = if t < a {
        t / a
    } else if t < a + d {
        1.0 - (1.0 - s) * (t - a) / d
    } else {
        s
    };
    if t < gate {
        held
    } else {
        let at_gate = if gate < a { gate / a } else if gate < a + d { 1.0 - (1.0 - s) * (gate - a) / d } else { s };
        at_gate * (1.0 - (t - gate) / r).max(0.0)
    }
}

impl Voice {
    fn new(inst: Inst, freq: f32, vel: f32, gate: f32, pan: f32, seed: u32, drive: f32, cut: f32) -> Self {
        let mut n = Noise(seed | 1);
        let phase = [n.next().abs(), n.next().abs(), n.next().abs(), n.next().abs(), n.next().abs()];
        Voice { inst, freq, vel, t: 0.0, gate, phase, filt: Svf::default(), filt2: Svf::default(), noise: n, pan, drive, cut }
    }

    fn duration(&self) -> f32 {
        match self.inst {
            Inst::Pad => self.gate + 1.6,
            Inst::Choir => self.gate + 1.4,
            Inst::Bass => self.gate + 0.06,
            Inst::Arp => 0.42,
            Inst::Lead => self.gate + 0.14,
            Inst::Pluck => 0.9,
            Inst::Bell => 2.6,
            Inst::Stab => 0.5,
            Inst::Kick => 0.32,
            Inst::Snare => 0.26,
            Inst::Hat => 0.07,
            Inst::OpenHat => 0.3,
            Inst::Crash => 2.2,
        }
    }

    #[inline]
    fn adv(&mut self, i: usize, f: f32) -> f32 {
        let dt = f / SR;
        self.phase[i] += dt;
        if self.phase[i] >= 1.0 {
            self.phase[i] -= 1.0;
        }
        dt
    }

    #[inline]
    fn sample(&mut self, lfo: f32) -> f32 {
        let t = self.t;
        self.t += 1.0 / SR;
        let f = self.freq;
        let out = match self.inst {
            Inst::Pad => {
                const DET: [f32; 3] = [0.9945, 1.0, 1.0058];
                let mut s = 0.0;
                for i in 0..3 {
                    let dt = self.adv(i, f * DET[i]);
                    s += saw(self.phase[i], dt);
                }
                let env = adsr(t, self.gate, 0.5, 0.4, 0.85, 1.5);
                self.filt.process(s * 0.33, self.cut * (0.8 + 0.25 * lfo), 0.9).lp * env
            }
            Inst::Choir => {
                let vib = 1.0 + 0.004 * (t * 5.2 * std::f32::consts::TAU).sin();
                let dt0 = self.adv(0, f * vib);
                let dt1 = self.adv(1, f * vib * 1.004);
                let src = saw(self.phase[0], dt0) + saw(self.phase[1], dt1);
                let a = self.filt.process(src, 620.0, 7.0).bp;
                let b = self.filt2.process(src, 1150.0, 8.0).bp;
                (a + b * 0.6) * adsr(t, self.gate, 0.7, 0.5, 0.8, 1.3) * 0.33
            }
            Inst::Bass => {
                let dt0 = self.adv(0, f);
                let dt1 = self.adv(1, f * 0.5);
                let s = saw(self.phase[0], dt0) * 0.7 + pulse(self.phase[1], dt1, 0.5) * 0.6;
                let cut = 180.0 + 1700.0 * (-t * 14.0).exp();
                let v = self.filt.process(s, cut, 1.6).lp;
                soft_clip(v * self.drive) * adsr(t, self.gate, 0.003, 0.08, 0.8, 0.05)
            }
            Inst::Arp => {
                let dt = self.adv(0, f);
                let s = pulse(self.phase[0], dt, 0.28);
                let cut = 500.0 + 3500.0 * (-t * 16.0).exp();
                self.filt.process(s, cut, 2.2).lp * (-t * 8.0).exp()
            }
            Inst::Lead => {
                let vib = 1.0 + 0.006 * ((t * 5.6 * std::f32::consts::TAU).sin()) * (t * 3.0).min(1.0);
                let dt0 = self.adv(0, f * vib);
                let dt1 = self.adv(1, f * vib * 1.007);
                let dt2 = self.adv(2, f * vib * 0.5);
                let s = saw(self.phase[0], dt0) + saw(self.phase[1], dt1) * 0.8 + pulse(self.phase[2], dt2, 0.5) * 0.3;
                let cut = 1200.0 + 3000.0 * (-t * 5.0).exp();
                let v = self.filt.process(s * 0.5, cut, 1.5).lp;
                soft_clip(v * 1.5) * adsr(t, self.gate, 0.008, 0.1, 0.75, 0.12)
            }
            Inst::Pluck => {
                let dt0 = self.adv(0, f);
                let dt1 = self.adv(1, f * 2.003);
                let s = saw(self.phase[0], dt0) + 0.4 * pulse(self.phase[1], dt1, 0.4);
                let cut = 700.0 + 3000.0 * (-t * 10.0).exp();
                self.filt.process(s * 0.6, cut, 1.2).lp * (-t * 4.2).exp()
            }
            Inst::Bell => {
                self.adv(0, f);
                self.adv(1, f * 2.76);
                self.adv(2, f * 5.4);
                let s = sine(self.phase[0]) + 0.45 * sine(self.phase[1]) * (-t * 3.5).exp()
                    + 0.2 * sine(self.phase[2]) * (-t * 7.0).exp();
                s * (-t * 1.9).exp() * (t / 0.004).min(1.0) * 0.8
            }
            Inst::Stab => {
                const DET: [f32; 3] = [0.992, 1.0, 1.009];
                let mut s = 0.0;
                for i in 0..3 {
                    let dt = self.adv(i, f * DET[i]);
                    s += saw(self.phase[i], dt);
                }
                let cut = 400.0 + 4200.0 * (-t * 11.0).exp();
                soft_clip(self.filt.process(s * 0.5, cut, 1.3).lp * 1.4) * (-t * 7.0).exp()
            }
            Inst::Kick => {
                let fr = 46.0 + 170.0 * (-t * 42.0).exp();
                self.adv(0, fr);
                let click = self.noise.next() * (-t * 450.0).exp() * 0.3;
                soft_clip(sine(self.phase[0]) * 1.8) * (-t * 11.0).exp() + click
            }
            Inst::Snare => {
                self.adv(0, 190.0 * (1.0 + 0.5 * (-t * 60.0).exp()));
                let n = self.noise.next();
                let hp = self.filt.process(n, 1800.0, 0.8).hp;
                (hp * (-t * 15.0).exp() * 0.85 + sine(self.phase[0]) * (-t * 22.0).exp() * 0.5) * 1.1
            }
            Inst::Hat => {
                let n = self.noise.next();
                self.filt.process(n, 8000.0, 1.0).hp * (-t * 65.0).exp() * 0.75
            }
            Inst::OpenHat => {
                let n = self.noise.next();
                self.filt.process(n, 7000.0, 1.0).hp * (-t * 11.0).exp() * 0.5
            }
            Inst::Crash => {
                let n = self.noise.next();
                self.filt.process(n, 4500.0, 0.7).hp * (-t * 2.2).exp() * 0.5
            }
        };
        out * self.vel
    }
}

/// Parse e.g. "F#5" into a MIDI number.
pub fn parse_note(s: &str) -> Option<f32> {
    let b = s.as_bytes();
    let base = match b.first()? {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let (acc, rest) = match b.get(1) {
        Some(b'#') => (1, &s[2..]),
        Some(b'b') => (-1, &s[2..]),
        _ => (0, &s[1..]),
    };
    let oct: i32 = rest.parse().ok()?;
    Some((12 * (oct + 1) + base + acc) as f32)
}

/// Parse a chord symbol ("Dm", "Bb", "F#m") into semitones from C and the chord tones.
pub fn parse_chord(s: &str) -> (f32, [f32; 4]) {
    let b = s.as_bytes();
    let base = match b.first() {
        Some(b'C') => 0,
        Some(b'D') => 2,
        Some(b'E') => 4,
        Some(b'F') => 5,
        Some(b'G') => 7,
        Some(b'A') => 9,
        Some(b'B') => 11,
        _ => 0,
    };
    let mut i = 1;
    let mut root = base as f32;
    match b.get(1) {
        Some(b'#') => {
            root += 1.0;
            i = 2;
        }
        Some(b'b') => {
            root -= 1.0;
            i = 2;
        }
        _ => {}
    }
    let minor = b.get(i) == Some(&b'm');
    let third = if minor { 3.0 } else { 4.0 };
    (root, [0.0, third, 7.0, if minor { 10.0 } else { 12.0 }])
}

struct LeadEvent {
    step: u32,
    len: u32,
    note: f32,
}

struct ParsedSection {
    chords: Vec<(f32, [f32; 4])>,
    lead: Vec<LeadEvent>,
    lead_len: u32,
}

fn parse_section(s: &Section) -> ParsedSection {
    let chords: Vec<_> = s.chords.split_whitespace().map(parse_chord).collect();
    let mut lead = Vec::new();
    let mut step = 0;
    for tok in s.lead.split_whitespace() {
        let mut it = tok.split(':');
        let (n, l) = (it.next().unwrap_or("-"), it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(4));
        if let Some(note) = parse_note(n) {
            lead.push(LeadEvent { step, len: l, note });
        }
        step += l;
    }
    ParsedSection { chords, lead, lead_len: step.max(16) }
}

pub struct Player {
    song: Song,
    parsed: Vec<ParsedSection>,
    voices: Vec<Voice>,
    step_samples: f32,
    until_step: f32,
    step: u32,
    section: usize,
    section_step: u32,
    repeat: u32,
    seed: u32,
    lfo: f32,
    duck: f32,
}

impl Player {
    pub fn new(track: Track, seed: u32) -> Self {
        let song = song(track);
        let parsed = song.sections.iter().map(parse_section).collect();
        let step_samples = 60.0 / song.bpm / 4.0 * SR;
        Player {
            song,
            parsed,
            voices: Vec::with_capacity(64),
            step_samples,
            until_step: 0.0,
            step: 0,
            section: 0,
            section_step: 0,
            repeat: 0,
            seed: seed | 1,
            lfo: 0.0,
            duck: 0.0,
        }
    }

    fn rnd(&mut self) -> u32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        x
    }

    fn spawn(&mut self, inst: Inst, note: f32, vel: f32, gate_steps: f32, pan: f32) {
        if self.voices.len() >= 64 {
            // Steal the oldest percussive voice.
            if let Some(i) = self.voices.iter().position(|v| matches!(v.inst, Inst::Hat | Inst::Arp | Inst::OpenHat)) {
                self.voices.swap_remove(i);
            } else {
                return;
            }
        }
        let gate = gate_steps * self.step_samples / SR;
        let seed = self.rnd();
        let (drive, cut) = (self.song.bass_drive, self.song.pad_cut);
        self.voices.push(Voice::new(inst, midi_hz(note), vel, gate, pan, seed, drive, cut));
    }

    fn tick(&mut self) {
        let section_count = self.song.sections.len();
        let sec: &'static Section = &self.song.sections[self.section];
        let (bar_index, bar_step) = ((self.section_step / 16) as usize, (self.section_step % 16) as usize);
        let bars = self.parsed[self.section].chords.len().max(1);
        let (root, tones) = self.parsed[self.section].chords[bar_index % bars];
        let on = |p: &str| p.as_bytes().get(bar_step).copied().unwrap_or(b'.');

        // Pad and choir, one chord per bar.
        if bar_step == 0 {
            let base = 48.0 + root;
            let oct = if base > 53.0 { -12.0 } else { 0.0 };
            for (i, t) in tones.iter().enumerate().take(3) {
                let pan = [-0.4, 0.4, 0.0][i];
                self.spawn(Inst::Pad, base + oct + t + 12.0, 0.17, 16.0, pan);
            }
            if !sec.choir.is_empty() {
                self.spawn(Inst::Choir, base + oct + tones[0], 0.4, 16.0, -0.2);
                self.spawn(Inst::Choir, base + oct + tones[2], 0.34, 16.0, 0.2);
            }
        }
        // Drums.
        if on(sec.kick) == b'x' {
            self.spawn(Inst::Kick, 40.0, 1.0, 1.0, 0.0);
            self.duck = 1.0;
        }
        if on(sec.snare) == b'x' {
            self.spawn(Inst::Snare, 40.0, 0.8, 1.0, 0.05);
        }
        if on(sec.hat) == b'x' {
            let accent = if bar_step % 4 == 2 { 0.5 } else { 0.28 };
            let open = bar_step == 14 && self.section_step % 64 >= 48;
            self.spawn(if open { Inst::OpenHat } else { Inst::Hat }, 40.0, accent, 1.0, -0.15);
        }
        // First beat of a new section gets a crash.
        if self.section_step == 0 && !sec.kick.is_empty() {
            self.spawn(Inst::Crash, 40.0, 0.5, 1.0, 0.1);
        }
        // Bass: digits are chord degrees (0 root, 3 third, 5 fifth, 1 octave up).
        let c = on(sec.bass);
        if c != b'.' {
            let bass_root = 24.0 + (root + 12.0) % 12.0 + 12.0;
            let off = match c {
                b'0' => 0.0,
                b'1' => 12.0,
                b'3' => tones[1],
                b'5' => 7.0,
                _ => 0.0,
            };
            let len = if sec.kick.is_empty() { 14.0 } else { 0.9 };
            self.spawn(Inst::Bass, bass_root + off, 0.62, len, 0.0);
        }
        // Arpeggio climbs chord tones across octaves.
        let c = on(sec.arp);
        if c.is_ascii_digit() {
            let d = (c - b'0') as usize;
            let t = tones[d % 3] + 12.0 * (d / 3) as f32;
            let pan = if bar_step % 2 == 0 { -0.45 } else { 0.45 };
            self.spawn(Inst::Arp, 60.0 + root.rem_euclid(12.0) + t, 0.3, 1.0, pan);
        }
        // Stabs.
        if on(sec.stab) == b'x' {
            let base = 60.0 + root.rem_euclid(12.0);
            for (i, t) in tones.iter().enumerate().take(3) {
                self.spawn(Inst::Stab, base + t, 0.22, 1.0, [-0.3, 0.0, 0.3][i]);
            }
        }
        // Melody.
        let inst = self.song.lead_inst;
        let lead = &self.parsed[self.section];
        let lead_len = lead.lead_len;
        let pos = self.section_step % lead_len;
        let mut hits: [(f32, f32); 2] = [(0.0, 0.0); 2];
        let mut n = 0;
        for ev in &lead.lead {
            if ev.step == pos && n < 2 {
                hits[n] = (ev.note, ev.len as f32);
                n += 1;
            }
        }
        for &(note, len) in hits.iter().take(n) {
            let vel = if inst == Inst::Bell { 0.3 } else { 0.27 };
            self.spawn(inst, note, vel, len - 0.2, 0.08);
        }

        // Advance song position.
        self.step += 1;
        self.section_step += 1;
        let section_len = 16 * bars as u32;
        let lead_bars = lead_len.div_ceil(16);
        let total = section_len.max(lead_bars * 16);
        if self.section_step >= total {
            self.section_step = 0;
            self.repeat += 1;
            if self.repeat >= sec.repeats {
                self.repeat = 0;
                self.section = (self.section + 1) % section_count;
            }
        }
    }

    /// Mix one stereo frame. `intensity` in 0..1 controls which layers are audible.
    #[inline]
    pub fn frame(&mut self, gains: &[f32; BUSES]) -> (f32, f32, f32) {
        if self.until_step <= 0.0 {
            self.tick();
            let swung = if self.step % 2 == 1 { 1.0 + self.song.swing } else { 1.0 - self.song.swing };
            self.until_step += self.step_samples * swung;
        }
        self.until_step -= 1.0;
        self.lfo += 0.21 / SR;
        if self.lfo >= 1.0 {
            self.lfo -= 1.0;
        }
        let lfo = sine(self.lfo);
        self.duck *= 0.99975;
        let side = 1.0 - 0.55 * self.duck * self.duck;
        let (mut l, mut r, mut send) = (0.0, 0.0, 0.0);
        let mut i = 0;
        while i < self.voices.len() {
            let v = &mut self.voices[i];
            let s = v.sample(lfo);
            let bus = bus_of(v.inst);
            let mut g = gains[bus];
            if bus != BUS_KICK && bus != BUS_PERC {
                g *= side;
            }
            let x = s * g;
            let (pl, pr) = (0.5 - v.pan * 0.5, 0.5 + v.pan * 0.5);
            l += x * pl * 1.4;
            r += x * pr * 1.4;
            send += x * match bus {
                BUS_PAD => 0.7,
                BUS_LEAD => 0.5,
                BUS_ARP => 0.45,
                BUS_STAB => 0.4,
                BUS_PERC => 0.1,
                _ => 0.0,
            };
            if v.t > v.duration() {
                self.voices.swap_remove(i);
            } else {
                i += 1;
            }
        }
        (l, r, send)
    }

    pub fn bpm(&self) -> f32 {
        self.song.bpm
    }
}

/// Maps game intensity to layer gains.
pub fn layer_gains(track: Track, intensity: f32) -> [f32; BUSES] {
    let ss = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let i = intensity;
    match track {
        Track::Menu | Track::GameOver => [0.9, 0.5, 0.5, 0.7, 0.0, 0.0, 0.0],
        Track::Build => [0.8, 0.55, 0.65, 0.65, 0.55, 0.45, 0.0],
        _ => [
            0.7,
            0.55 + 0.4 * ss(0.0, 0.25, i),
            0.45 + 0.4 * ss(0.1, 0.45, i),
            0.2 + 0.7 * ss(0.3, 0.7, i),
            0.65 + 0.35 * ss(0.0, 0.2, i),
            0.3 + 0.5 * ss(0.15, 0.5, i),
            0.2 + 0.7 * ss(0.4, 0.8, i),
        ],
    }
}

/// Plays a track, crossfades on change, and applies the shared delay and reverb.
pub struct Music {
    current: (Player, Track),
    outgoing: Option<(Player, Track)>,
    fade: f32,
    pub intensity: f32,
    target_intensity: f32,
    delay_l: Delay,
    delay_r: Delay,
    reverb: Reverb,
    seed: u32,
    pub volume: f32,
}

const FADE_SECONDS: f32 = 2.4;

impl Music {
    pub fn new(track: Track) -> Self {
        let mut m = Music {
            current: (Player::new(track, 77), track),
            outgoing: None,
            fade: 1.0,
            intensity: 0.0,
            target_intensity: 0.0,
            delay_l: Delay::new(44100),
            delay_r: Delay::new(44100),
            reverb: Reverb::new(),
            seed: 991,
            volume: 0.8,
        };
        m.update_delay();
        m
    }

    fn update_delay(&mut self) {
        let beat = 60.0 / self.current.0.bpm() * SR;
        self.delay_l.set_len((beat * 0.75) as usize);
        self.delay_r.set_len((beat * 0.5) as usize);
    }

    pub fn set_track(&mut self, track: Track) {
        if self.current.1 == track {
            return;
        }
        self.seed = self.seed.wrapping_mul(31).wrapping_add(7);
        let new = (Player::new(track, self.seed), track);
        self.outgoing = Some(std::mem::replace(&mut self.current, new));
        self.fade = 0.0;
        self.update_delay();
    }

    pub fn set_intensity(&mut self, v: f32) {
        self.target_intensity = v.clamp(0.0, 1.0);
    }

    #[cfg(test)]
    pub fn track(&self) -> Track {
        self.current.1
    }

    /// Adds into an interleaved stereo buffer.
    pub fn render_add(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            self.intensity += (self.target_intensity - self.intensity) * (1.0 / (SR * 1.2));
            let gains = layer_gains(self.current.1, self.intensity);
            let (mut l, mut r, mut send) = self.current.0.frame(&gains);
            if self.fade < 1.0 {
                self.fade = (self.fade + 1.0 / (FADE_SECONDS * SR)).min(1.0);
                let k_in = (self.fade * std::f32::consts::FRAC_PI_2).sin();
                let k_out = (self.fade * std::f32::consts::FRAC_PI_2).cos();
                l *= k_in;
                r *= k_in;
                send *= k_in;
                if let Some((p, t)) = &mut self.outgoing {
                    let g = layer_gains(*t, self.intensity);
                    let (a, b, c) = p.frame(&g);
                    l += a * k_out;
                    r += b * k_out;
                    send += c * k_out;
                }
            } else if self.outgoing.is_some() {
                self.outgoing = None;
            }
            let (dl, dr) = (self.delay_l.tap(), self.delay_r.tap());
            self.delay_l.push(send * 0.35 + dr * 0.38);
            self.delay_r.push(send * 0.35 + dl * 0.38);
            let (rl, rr) = self.reverb.process(send * 0.4, 0.84, 0.3);
            frame[0] += (l + dl * 0.45 + rl) * self.volume;
            frame[1] += (r + dr * 0.45 + rr) * self.volume;
        }
    }
}
