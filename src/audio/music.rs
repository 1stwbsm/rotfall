//! The soundtrack engine: an original score written as note data and synthesized live.
//!
//! ROTFALL's sound is orchestral techno with a few rules of its own:
//!
//! - **The Engine.** The night music runs on a machine: slow, rigid and exact. A pile-driver kick on
//!   every beat, and a slam the size of a building dropping across it at half the speed.
//! - **The Wall.** See `mass.rs`: an orchestra as one solid block of sound, behind everything.
//! - **Low.** The tunes are played two octaves down on a heavy distorted riff voice, over bare fifths
//!   and a reactor's drone. Stabs carry a flat ninth grinding away at the top: the Rot Chord.
//! - **The Sinking.** Key changes only go down. When a theme returns, the world has sunk a semitone.
//! - **ROT, FALL.** A falling tritone on an enormous horn is the album's call sign.
//! - **Steady.** Nothing wobbles. No vibrato and no slides: pitches stand where they were put.
//!
//! A song is a list of sections. All instruments always play, but sit on layer buses whose gain
//! follows the game's intensity, so the arrangement builds with the horde without skipping a beat.
use super::dsp::*;
use super::mass::{Mass, MassCue, ONCE, QUIET};
pub use super::score::song;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Track {
    Menu,
    Build,
    CombatA,
    CombatB,
    CombatC,
    Boss,
    GameOver,
    Rot,
    OneOfMany,
    Smitherines,
    RestWell,
}

impl Track {
    /// In album order.
    pub const ALL: [Track; 11] = [
        Track::Menu, Track::Build, Track::CombatA, Track::CombatB, Track::CombatC, Track::Boss,
        Track::GameOver, Track::Rot, Track::OneOfMany, Track::Smitherines, Track::RestWell,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Track::Menu => "Descent",
            Track::Build => "Daytime Escape",
            Track::CombatA => "Holdout",
            Track::CombatB => "No Sleep",
            Track::CombatC => "Rust Choir",
            Track::Boss => "Colossus",
            Track::GameOver => "Overrun",
            Track::Rot => "Rot",
            Track::OneOfMany => "One of Many",
            Track::Smitherines => "Smitherines",
            Track::RestWell => "Rest Well, for It Is Over",
        }
    }

    /// Tracks that land like a blow: they cut straight in rather than fading.
    pub fn is_death(self) -> bool {
        matches!(self, Track::GameOver | Track::Rot | Track::OneOfMany | Track::Smitherines | Track::RestWell)
    }

    /// How long one full pass through the track takes.
    pub fn loop_seconds(self) -> f32 {
        let s = song(self);
        let mut total = 0.0;
        for sec in s.sections {
            let steps = parse_section(sec).len * sec.repeats;
            for i in 0..steps {
                let tempo = sec.tempo.0 + (sec.tempo.1 - sec.tempo.0) * i as f32 / steps as f32;
                total += 60.0 / (s.bpm * tempo) / 4.0;
            }
        }
        total
    }
}

/// One layer of a section beyond the standard lanes.
#[derive(Clone, Copy)]
pub struct Part {
    pub inst: Inst,
    pub vel: f32,
    pub pan: f32,
    pub score: Score,
}

#[derive(Clone, Copy)]
pub enum Score {
    /// Space separated `Note:sixteenths`, `-` for a rest. `C3+G3:16` is a chord. The Geiger counter
    /// can also ramp its rate, as `C2>C4:128`.
    Line(&'static str),
    /// A fixed note on a pattern: `x` a hit, `X` an accent, `o` a ghost.
    Hits(&'static str, &'static str),
}

pub const fn line(inst: Inst, vel: f32, pan: f32, notes: &'static str) -> Part {
    Part { inst, vel, pan, score: Score::Line(notes) }
}

pub const fn hit(inst: Inst, note: &'static str, vel: f32, pan: f32, pattern: &'static str) -> Part {
    Part { inst, vel, pan, score: Score::Hits(note, pattern) }
}

pub struct Section {
    /// One chord per bar.
    pub chords: &'static str,
    // Patterns, one character per sixteenth, sixteen to the bar.
    pub kick: &'static str,
    pub snare: &'static str,
    pub hat: &'static str,
    pub ride: &'static str,
    pub bass: &'static str,
    pub arp: &'static str,
    /// A driving low sequence: digits are chord degrees, as in `bass`.
    pub low: &'static str,
    pub tom: &'static str,
    pub sub: &'static str,
    pub stab: &'static str,
    /// Space separated `Note:sixteenths` pairs, `-` is a rest. Each loops over the section.
    pub lead: &'static str,
    /// A second line above or around the melody.
    pub counter: &'static str,
    /// Long synth brass lines.
    pub brass: &'static str,
    /// The sustained chord: 2 is a full spread, 1 bare fifths, 0 nothing.
    pub pad: u8,
    pub repeats: u32,
    /// A noise riser in the last bar, leading into the next section.
    pub swell: bool,
    /// A drum fill in the last bar.
    pub fill: bool,
    /// How hard the arrangement pushes, 0..1.
    pub energy: f32,
    /// Everything stops dead for the last beat, then comes back on the downbeat.
    pub gasp: bool,
    /// Pairs below run from the start of the section to its end, across repeats.
    pub tempo: (f32, f32),
    /// The master filter: 1 is wide open, 0 is hearing it from inside a sealed bunker.
    pub dark: (f32, f32),
    /// The music physically falling apart: crushed, and dropping out. 0..1.
    pub rot: (f32, f32),
    /// Playback speed of the whole mix. Running down to 0 is the power failing; it is put right
    /// when the next section begins.
    pub speed: (f32, f32),
    /// How much the kick feeds the low rumble beneath it, 0..1.
    pub rumble: (f32, f32),
    /// Reverb size, 0 for the song's own. Near 1 never quite dies away.
    pub hall: f32,
    /// Semitones to move everything by. In this soundtrack, a key change goes down: when a theme
    /// comes back, the whole world has sunk a little further.
    pub shift: i32,
    /// How far the kick pushes everything else out of its way, 0..1.
    pub pump: f32,
    /// Layers shared with other sections, then this section's own.
    pub kit: &'static [Part],
    pub parts: &'static [Part],
    pub mass: MassCue,
}

pub struct Song {
    pub bpm: f32,
    pub swing: f32,
    pub pad_cut: f32,
    pub bass_drive: f32,
    pub lead_inst: Inst,
    pub arp_inst: Inst,
    /// Semitones the tune is moved from where it is written. Stabs and arpeggios go half as far.
    pub low: f32,
    /// Where the song returns to after its last section, so that intros only play once.
    pub loop_to: usize,
    /// The usual size of its reverb.
    pub hall: f32,
    pub sections: &'static [Section],
}

const O: &str = "";

/// A section with nothing in it, to build on.
pub const E: Section = Section {
    chords: "", kick: O, snare: O, hat: O, ride: O, bass: O, arp: O, low: O, tom: O, sub: O,
    stab: O, lead: O, counter: O, brass: O, pad: 2, repeats: 1, swell: false, fill: false, energy: 0.8,
    gasp: false, tempo: (1.0, 1.0), dark: (1.0, 1.0), rot: (0.0, 0.0), speed: (1.0, 1.0),
    rumble: (0.0, 0.0), hall: 0.0, shift: 0, pump: 0.5, kit: &[], parts: &[], mass: QUIET,
};

/// Instruments, all unashamedly synthesizers. Each maps to a mix layer via `bus_of()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inst {
    /// Slow detuned saws under everything.
    Pad,
    /// A string machine: bright saws through the ensemble.
    SynthStrings,
    /// A short resonant pulse, for sequenced patterns.
    Arp,
    /// Saws with a filter that snaps open: synth brass.
    Brass,
    /// The night's melody voice: low, distorted and dark, somewhere between a synth and a riff.
    Riff,
    /// A wide saw chord, slammed shut by its filter.
    Stab,
    Pluck,
    Bell,
    /// A tuned electronic drum.
    SynTom,
    /// A long sub hit.
    Sub,
    Bass,
    /// The pile driver: a thump with iron in it.
    Kick,
    /// A slap on sheet steel.
    Snare,
    /// Something the size of a building, landing. The night's backbeat.
    Slam,
    /// A dry metal tick.
    Hat,
    /// A steam vent.
    OpenHat,
    /// A struck pipe.
    Ride,
    Crash,
    /// Filtered noise sweeping upward, into the next section.
    Riser,
    /// An oil drum.
    Tom,
    /// The reactor itself: a drone that never really stops.
    Hum,
    /// The call sign. Enormous.
    Horn,
    /// Something huge being struck, deep inside the reactor.
    Hull,
    /// A building coming down, a long way off.
    Collapse,
    /// A short metallic clang.
    Anvil,
    /// A Geiger counter. The note sets the click rate, at a tenth of its frequency.
    Geiger,
    Wind,
    Heart,
    /// The ringing in your ears after an explosion: one thin, perfectly steady tone.
    Ring,
    /// A clock.
    Tick,
}

const BUS_PAD: usize = 0;
const BUS_BASS: usize = 1;
const BUS_ARP: usize = 2;
const BUS_LEAD: usize = 3;
const BUS_KICK: usize = 4;
const BUS_PERC: usize = 5;
const BUS_STAB: usize = 6;
/// The world around the music. It doesn't care how the fight is going.
const BUS_ATMOS: usize = 7;
const BUS_MASS: usize = 8;
pub const BUSES: usize = 9;
pub const BUS_NAMES: [&str; BUSES] = ["pads", "bass", "arp", "lead", "kick", "perc", "stab", "atmos", "wall"];

fn bus_of(i: Inst) -> usize {
    match i {
        Inst::Pad | Inst::Hum => BUS_PAD,
        Inst::Bass => BUS_BASS,
        Inst::Pluck | Inst::Arp => BUS_ARP,
        Inst::Riff | Inst::SynthStrings | Inst::Brass | Inst::Bell => BUS_LEAD,
        Inst::Stab | Inst::Horn => BUS_STAB,
        Inst::Kick | Inst::Sub => BUS_KICK,
        Inst::Hull | Inst::Collapse | Inst::Geiger | Inst::Wind | Inst::Heart | Inst::Ring => BUS_ATMOS,
        Inst::SynTom | Inst::Snare | Inst::Slam | Inst::Hat | Inst::OpenHat | Inst::Ride | Inst::Crash | Inst::Riser
        | Inst::Tom | Inst::Anvil | Inst::Tick => BUS_PERC,
    }
}

/// These go through the ensemble effect, which is what string machines are known for.
fn is_section(i: Inst) -> bool {
    matches!(i, Inst::Pad | Inst::SynthStrings | Inst::Brass)
}

#[derive(Clone)]
struct Voice {
    inst: Inst,
    freq: f32,
    vel: f32,
    t: f32,
    gate: f32,
    phase: [f32; 6],
    filt: Svf,
    filt2: Svf,
    noise: Noise,
    pan: f32,
    drive: f32,
    cut: f32,
    /// Multiplies the pitch on every sample. Only a Geiger counter's ramp ever moves it.
    ramp: f32,
    /// A number of its own, 0..1, for things that shouldn't line up from voice to voice.
    own: f32,
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
    #[allow(clippy::too_many_arguments)]
    fn new(inst: Inst, freq: f32, to: Option<f32>, vel: f32, gate: f32, pan: f32, seed: u32, drive: f32, cut: f32) -> Self {
        let mut n = Noise(seed | 1);
        let mut phase = [0.0; 6];
        for p in &mut phase {
            *p = n.next().abs();
        }
        // Drums and bells start every oscillator together, for a clean attack.
        if matches!(inst, Inst::SynTom | Inst::Sub | Inst::Kick | Inst::Bell | Inst::Tom | Inst::Hull | Inst::Anvil | Inst::Geiger | Inst::Heart | Inst::Slam) {
            phase = [0.0; 6];
        }
        let own = n.next().abs();
        let ramp = to.map_or(1.0, |t| fexp((t / freq).ln() / (gate.max(0.01) * SR)));
        Voice { inst, freq, vel, t: 0.0, gate, phase, filt: Svf::default(), filt2: Svf::default(), noise: n, pan, drive, cut, ramp, own }
    }

    fn duration(&self) -> f32 {
        match self.inst {
            Inst::Pad => self.gate + 1.6,
            Inst::SynthStrings => self.gate + 0.4,
            Inst::Brass => self.gate + 0.2,
            Inst::Bass => self.gate + 0.06,
            Inst::Arp => 0.42,
            Inst::Riff => self.gate + 0.06,
            Inst::Pluck => 0.9,
            Inst::Bell => 2.6,
            Inst::Stab => 0.6,
            Inst::SynTom => 0.9,
            Inst::Sub => 1.8,
            Inst::Kick => 0.4,
            Inst::Tom => 0.5,
            Inst::Snare => 0.3,
            Inst::Slam => 1.4,
            Inst::Hat => 0.05,
            Inst::OpenHat => 0.4,
            Inst::Ride => 0.7,
            Inst::Crash => 2.4,
            Inst::Riser => self.gate + 0.05,
            Inst::Hum => self.gate + 3.0,
            Inst::Horn => self.gate + 0.7,
            Inst::Hull => 7.0,
            Inst::Collapse => 5.0,
            Inst::Anvil => 0.6,
            Inst::Geiger => self.gate,
            Inst::Wind => self.gate + 3.0,
            Inst::Heart => 0.72,
            Inst::Ring => self.gate + 3.0,
            Inst::Tick => 0.05,
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

    /// Three slightly detuned saws.
    #[inline]
    fn saws(&mut self, f: f32, spread: f32) -> f32 {
        let d0 = self.adv(0, f * (1.0 - spread));
        let d1 = self.adv(1, f);
        let d2 = self.adv(2, f * (1.0 + spread * 1.1));
        (saw(self.phase[0], d0) + saw(self.phase[1], d1) + saw(self.phase[2], d2)) * 0.33
    }

    /// Six clashing square waves: a sheet of metal.
    #[inline]
    fn metal(&mut self, base: f32) -> f32 {
        const RATIOS: [f32; 6] = [1.0, 1.304, 1.466, 1.787, 1.932, 2.536];
        let mut m = 0.0;
        for (i, r) in RATIOS.iter().enumerate() {
            self.adv(i, base * r);
            m += if self.phase[i] < 0.5 { 1.0 } else { -1.0 };
        }
        m / 6.0
    }

    #[inline]
    fn sample(&mut self, lfo: f32) -> f32 {
        let t = self.t;
        self.t += 1.0 / SR;
        let f = self.freq;
        self.freq *= self.ramp;
        let out = match self.inst {
            Inst::Pad => {
                let s = self.saws(f, 0.0008);
                // Louder passages open the filter as well as raising the level.
                let cut = self.cut * (0.8 + 0.25 * lfo) * (0.6 + self.vel * 2.5);
                self.filt.process(s, cut, 0.9).lp * adsr(t, self.gate, 0.5, 0.4, 0.85, 1.5)
            }
            Inst::SynthStrings => {
                let s = self.saws(f, 0.0008);
                self.filt.process(s, (f * 5.0).clamp(1000.0, 5000.0), 0.8).lp * adsr(t, self.gate, 0.04, 0.1, 0.9, 0.35)
            }
            Inst::Arp => {
                let dt = self.adv(0, f);
                let s = pulse(self.phase[0], dt, 0.28);
                let cut = 350.0 + 2200.0 * fexp(-t * 16.0);
                self.filt.process(s, cut, 1.8).lp * fexp(-t * 8.0)
            }
            Inst::Brass => {
                let s = self.saws(f, 0.0008);
                let dt = self.adv(3, f * 0.5);
                let oct = pulse(self.phase[3], dt, 0.5) * 0.3;
                // Opens hard on the attack, then settles.
                let cut = f * (1.6 + 4.0 * (t / 0.05).min(1.0) * fexp(-t * 5.0));
                soft_clip(self.filt.process(s + oct, cut.clamp(200.0, 5500.0), 1.2).lp * 1.5) * adsr(t, self.gate, 0.012, 0.1, 0.85, 0.18)
            }
            Inst::Riff => {
                // Two saws and a square an octave down, pushed hard into distortion and kept dark.
                let d0 = self.adv(0, f);
                let d1 = self.adv(1, f * 1.0008);
                let d2 = self.adv(2, f * 0.5);
                let s = saw(self.phase[0], d0) + saw(self.phase[1], d1) * 0.6 + pulse(self.phase[2], d2, 0.5) * 0.7;
                let cut = 1250.0 + 2200.0 * fexp(-t * 9.0);
                soft_clip(self.filt.process(s * 0.5, cut, 1.1).lp * 3.2) * adsr(t, self.gate, 0.004, 0.06, 0.85, 0.05) * 0.66
            }
            Inst::Stab => {
                let s = self.saws(f, 0.006);
                let cut = 450.0 + 3500.0 * fexp(-t * 11.0);
                soft_clip(self.filt.process(s * 1.5, cut, 1.1).lp * 1.6) * fexp(-t * 6.0)
            }
            Inst::Pluck => {
                let dt0 = self.adv(0, f);
                let dt1 = self.adv(1, f * 2.003);
                let s = saw(self.phase[0], dt0) + 0.4 * pulse(self.phase[1], dt1, 0.4);
                let cut = 700.0 + 3000.0 * fexp(-t * 10.0);
                self.filt.process(s * 0.6, cut, 1.2).lp * fexp(-t * 4.2)
            }
            Inst::Bell => {
                self.adv(0, f);
                self.adv(1, f * 2.76);
                self.adv(2, f * 5.4);
                let s = sine(self.phase[0]) + 0.45 * sine(self.phase[1]) * fexp(-t * 3.5) + 0.2 * sine(self.phase[2]) * fexp(-t * 7.0);
                s * fexp(-t * 1.9) * (t / 0.004).min(1.0) * 0.8
            }
            Inst::SynTom => {
                self.adv(0, f * (1.0 + 0.25 * fexp(-t * 30.0)));
                let skin = self.filt.process(self.noise.next(), 500.0, 0.8).lp * fexp(-t * 50.0) * 0.5;
                soft_clip(sine(self.phase[0]) * 1.6 + skin) * fexp(-t * 5.0)
            }
            Inst::Sub => {
                self.adv(0, 45.0 + 60.0 * fexp(-t * 20.0));
                soft_clip(sine(self.phase[0]) * 1.8) * fexp(-t * 2.4)
            }
            Inst::Bass => {
                let dt0 = self.adv(0, f);
                let dt1 = self.adv(1, f * 0.5);
                let s = saw(self.phase[0], dt0) * 0.7 + pulse(self.phase[1], dt1, 0.5) * 0.6;
                let cut = 240.0 + 1400.0 * fexp(-t * 14.0);
                soft_clip(self.filt.process(s, cut, 1.3).lp * self.drive) * adsr(t, self.gate, 0.003, 0.08, 0.8, 0.05)
            }
            Inst::Kick => {
                // A pile driver: the thump, then the iron it is hitting, then the crack of contact.
                let fr = 44.0 + 150.0 * fexp(-t * 34.0) + 24.0 * fexp(-t * 8.0);
                self.adv(0, fr);
                self.adv(1, 317.0);
                self.adv(2, 1143.0);
                let body = soft_clip(sine(self.phase[0]) * 2.2) * fexp(-t * 8.0);
                let iron = (sine(self.phase[1]) + sine(self.phase[2]) * 0.6) * fexp(-t * 42.0) * 0.14;
                let crack = self.filt.process(self.noise.next(), 2600.0, 0.8).bp * fexp(-t * 380.0) * 0.8;
                body + iron + crack
            }
            Inst::Tom => {
                // An oil drum, which has a second, unrelated note in it.
                self.adv(0, f * (1.0 + 0.3 * fexp(-t * 30.0)));
                self.adv(1, f * 1.62);
                soft_clip(sine(self.phase[0]) * 1.5) * fexp(-t * 8.0) + sine(self.phase[1]) * fexp(-t * 26.0) * 0.22
            }
            Inst::Snare => {
                // A slap on sheet steel: wires, the plate's bark, and what rings on afterwards.
                self.adv(0, 190.0 * (1.0 + 0.5 * fexp(-t * 60.0)));
                self.adv(1, 731.0);
                self.adv(2, 1683.0);
                let n = self.noise.next();
                let wires = self.filt.process(n, 1800.0, 0.8).hp * fexp(-t * 15.0) * 0.75;
                let plate = self.filt2.process(n, 1250.0, 3.0).bp * fexp(-t * 22.0) * 1.4;
                let ring = (sine(self.phase[1]) + sine(self.phase[2]) * 0.7) * fexp(-t * 30.0) * 0.2;
                (wires + plate + ring + sine(self.phase[0]) * fexp(-t * 22.0) * 0.5) * 1.05
            }
            Inst::Slam => {
                // The thump of it, the crack of it, and sheet metal shuddering long afterwards.
                self.adv(0, 58.0 * (1.0 + 1.1 * fexp(-t * 30.0)));
                let body = soft_clip(sine(self.phase[0]) * 2.0) * fexp(-t * 6.5);
                let n = self.noise.next();
                let crack = self.filt.process(n, 1300.0, 0.7).bp * fexp(-t * 13.0) * 1.4;
                let m = self.metal(240.0);
                let sheet = self.filt2.process(m * 0.5 + n * 0.5, 2600.0, 0.7).hp * fexp(-t * 4.0) * 0.32;
                body + crack + sheet
            }
            Inst::Hat => self.filt.process(self.noise.next(), 7000.0, 0.9).hp * fexp(-t * 130.0) * 0.55,
            Inst::OpenHat => {
                let hiss = self.filt.process(self.noise.next(), 5500.0, 0.9).hp;
                hiss * (t / 0.02).min(1.0) * fexp(-t * 8.0) * 0.5
            }
            Inst::Ride => {
                self.adv(0, 720.0);
                self.adv(1, 720.0 * 2.76);
                self.adv(2, 720.0 * 5.4);
                (sine(self.phase[0]) + sine(self.phase[1]) * 0.8 + sine(self.phase[2]) * 0.5) * fexp(-t * 7.0) * 0.3
            }
            Inst::Crash => {
                let m = self.metal(330.0);
                let n = self.noise.next();
                self.filt.process(m * 0.3 + n * 0.8, 4000.0, 0.7).hp * fexp(-t * 1.9) * 0.5
            }
            Inst::Riser => {
                let n = self.noise.next();
                let k = (t / self.gate.max(0.1)).min(1.0);
                let sweep = self.filt.process(n, 300.0 * fexp(k * 3.0), 1.5).bp;
                sweep * k * ((self.gate + 0.05 - t) / 0.05).clamp(0.0, 1.0) * 0.8
            }
            Inst::Hum => {
                let s = self.saws(f, 0.0008);
                self.adv(3, f * 0.5);
                let sub = sine(self.phase[3]);
                let body = self.filt.process(s, f * 8.0, 1.0).lp;
                soft_clip((body + sub * 0.25) * 1.4) * adsr(t, self.gate, 2.5, 0.5, 0.9, 3.0)
            }
            Inst::Horn => {
                let s = self.saws(f, 0.0008);
                let dt = self.adv(3, f * 0.5);
                let low = pulse(self.phase[3], dt, 0.5) * 0.5;
                let cut = f * 1.5 + 4200.0 * (t / 0.035).min(1.0) * fexp(-t * 2.6);
                soft_clip(self.filt.process(s + low, cut, 1.2).lp * 3.0) * adsr(t, self.gate, 0.012, 0.3, 0.75, 0.65)
            }
            Inst::Hull => {
                const PARTIALS: [(f32, f32, f32); 6] = [(1.0, 1.0, 0.75), (1.58, 0.62, 1.0), (2.24, 0.5, 1.4), (2.92, 0.35, 2.1), (3.76, 0.22, 3.0), (5.1, 0.12, 5.0)];
                let mut s = 0.0;
                for (i, (r, a, d)) in PARTIALS.iter().enumerate() {
                    self.adv(i, f * r);
                    s += sine(self.phase[i]) * a * fexp(-t * d);
                }
                let strike = self.filt.process(self.noise.next(), 900.0, 0.8).lp * fexp(-t * 60.0) * 0.6;
                (s + strike) * 0.38
            }
            Inst::Collapse => {
                let cut = 90.0 + 1300.0 * fexp(-t * 1.5);
                let x = self.filt.process(self.noise.next(), cut, 0.9).lp;
                let tumble = 1.0 + 0.6 * sine(t * (5.0 + self.own * 6.0));
                self.adv(0, 38.0 + 70.0 * fexp(-t * 14.0));
                let low = sine(self.phase[0]) * fexp(-t * 2.2);
                soft_clip((x * 2.4 * tumble * fexp(-t * 0.95) + low) * 1.3)
            }
            Inst::Anvil => {
                const PARTIALS: [(f32, f32, f32); 4] = [(1.0, 1.0, 9.0), (2.76, 0.7, 14.0), (5.4, 0.45, 22.0), (8.93, 0.25, 35.0)];
                let mut s = 0.0;
                for (i, (r, a, d)) in PARTIALS.iter().enumerate() {
                    self.adv(i, f * r);
                    s += sine(self.phase[i]) * a * fexp(-t * d);
                }
                let strike = self.filt.process(self.noise.next(), 2600.0, 0.8).bp * fexp(-t * 300.0) * 0.8;
                (s * 0.5 + strike) * (t / 0.0006).min(1.0)
            }
            Inst::Geiger => {
                if (self.noise.next() * 0.5 + 0.5) < (f / 10.0) / SR {
                    self.phase[0] = 1.0;
                }
                self.phase[0] *= 0.93;
                let click = self.phase[0];
                let x = self.filt.process(self.noise.next() * click, 3200.0, 1.2).bp;
                x * 2.2 * (t / 0.1).min(1.0) * ((self.gate - t) / 0.1).clamp(0.0, 1.0)
            }
            Inst::Wind => {
                let own = self.own;
                let w = sine(t * 0.083 + own) * 0.5 + sine(t * 0.21 + own * 2.0) * 0.35;
                let x = self.filt.process(self.noise.next(), (f * (2.0 + 1.4 * w)).clamp(120.0, 5200.0), 1.4).bp;
                x * (0.65 + 0.35 * sine(t * 0.13 + own)) * adsr(t, self.gate, 3.0, 1.0, 0.9, 3.0) * 1.4
            }
            Inst::Heart => {
                let beat = |p: f32, x: f32| sine(p) * fexp(-x * 11.0) * (x / 0.004).min(1.0);
                self.adv(0, 48.0 * (1.0 + 0.6 * fexp(-t * 30.0)));
                let lub = beat(self.phase[0], t);
                let dub = if t > 0.26 {
                    let x = t - 0.26;
                    self.adv(1, 44.0 * (1.0 + 0.6 * fexp(-x * 30.0)));
                    beat(self.phase[1], x) * 0.7
                } else {
                    0.0
                };
                soft_clip((lub + dub) * 1.6)
            }
            Inst::Ring => {
                self.adv(0, f);
                sine(self.phase[0]) * adsr(t, self.gate, 0.02, 0.5, 0.8, 3.0)
            }
            Inst::Tick => {
                self.adv(0, f);
                let tone = sine(self.phase[0]) * fexp(-t * 190.0) * 0.7;
                tone + self.filt.process(self.noise.next(), f * 1.7, 2.0).bp * fexp(-t * 260.0) * 1.6
            }
        };
        out * self.vel
    }
}

/// One note on its own, for checking how an instrument behaves.
#[cfg(test)]
pub(crate) fn render_note(inst: Inst, note: f32, seconds: f32) -> Vec<f32> {
    let mut v = Voice::new(inst, midi_hz(note), None, 1.0, seconds, 0.0, 12345, 2.0, 1500.0);
    (0..(seconds * SR) as usize).map(|_| v.sample(0.0)).collect()
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

/// Parse a chord symbol ("Dm", "Bb", "F#m", "Cdim", "Eb5") into semitones from C and the chord
/// tones: root, third, fifth, seventh. A "5" chord has no third, and is only stronger for it.
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
    let tones = match &s[i.min(s.len())..] {
        "dim" => [0.0, 3.0, 6.0, 9.0],
        "5" => [0.0, 12.0, 7.0, 12.0],
        q if q.starts_with('m') => [0.0, 3.0, 7.0, 10.0],
        _ => [0.0, 4.0, 7.0, 12.0],
    };
    (root, tones)
}

/// What a digit in a bass, sequence or low pattern means, in semitones above the chord's root.
fn degree(c: u8, tones: &[f32; 4]) -> f32 {
    match c {
        b'1' => 12.0,
        b'3' => tones[1],
        b'5' => 7.0,
        // The two notes the dread is built on: a semitone's grind and a tritone's menace.
        b'2' => 1.0,
        b'6' => 6.0,
        b'7' => tones[3],
        _ => 0.0,
    }
}

struct LeadEvent {
    step: u32,
    len: u32,
    note: f32,
    to: Option<f32>,
}

/// One melodic line: its notes, and how long before it repeats.
struct Line {
    events: Vec<LeadEvent>,
    len: u32,
}

/// Reads a `C3+G3>F3:16` token's notes, as (note, slide target) pairs.
pub fn parse_notes(notes: &str) -> Vec<(f32, Option<f32>)> {
    notes
        .split('+')
        .filter_map(|n| {
            let mut it = n.split('>');
            let from = parse_note(it.next()?)?;
            Some((from, it.next().and_then(parse_note)))
        })
        .collect()
}

fn parse_line(text: &str, shift: f32) -> Line {
    let mut events = Vec::new();
    let mut step = 0;
    for tok in text.split_whitespace() {
        let mut it = tok.split(':');
        let (n, l) = (it.next().unwrap_or("-"), it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(4));
        for (note, to) in parse_notes(n) {
            events.push(LeadEvent { step, len: l, note: note + shift, to: to.map(|t| t + shift) });
        }
        step += l;
    }
    Line { events, len: step }
}

enum PartRun {
    Line(Line),
    Hits(f32, &'static [u8]),
}

struct ParsedPart {
    inst: Inst,
    vel: f32,
    pan: f32,
    run: PartRun,
}

pub(crate) struct ParsedSection {
    chords: Vec<(f32, [f32; 4])>,
    lead: Line,
    counter: Line,
    brass: Line,
    parts: Vec<ParsedPart>,
    /// How many steps it runs for: as many bars as it has chords, or as its melody needs.
    pub(crate) len: u32,
}

/// Sixteen sixteenths to the bar, without exception.
const BAR: u32 = 16;

pub(crate) fn parse_section(s: &Section) -> ParsedSection {
    let shift = s.shift as f32;
    let chords: Vec<_> = s.chords.split_whitespace().map(parse_chord).map(|(root, tones)| (root + shift, tones)).collect();
    let (lead, counter, brass) = (parse_line(s.lead, shift), parse_line(s.counter, shift), parse_line(s.brass, shift));
    let bars = chords.len().max(1) as u32;
    let len = (bars * BAR).max(lead.len.max(1).div_ceil(BAR) * BAR);
    let parts = s
        .kit
        .iter()
        .chain(s.parts)
        .map(|p| ParsedPart {
            inst: p.inst,
            vel: p.vel,
            pan: p.pan,
            run: match p.score {
                Score::Line(text) => PartRun::Line(parse_line(text, shift)),
                Score::Hits(note, pattern) => PartRun::Hits(parse_note(note).unwrap_or(60.0) + shift, pattern.as_bytes()),
            },
        })
        .collect();
    ParsedSection { chords, lead, counter, brass, parts, len }
}

/// Two copies, one per side: the width of a string machine, held almost still so that nothing wobbles.
struct Ensemble {
    buf: Vec<f32>,
    at: usize,
    phase: f32,
}

impl Ensemble {
    fn new() -> Self {
        Ensemble { buf: vec![0.0; 2048], at: 0, phase: 0.0 }
    }

    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let pos = self.at as f32 + self.buf.len() as f32 - delay;
        let i = pos as usize;
        let fr = pos - i as f32;
        let n = self.buf.len();
        self.buf[i % n] * (1.0 - fr) + self.buf[(i + 1) % n] * fr
    }

    #[inline]
    fn process(&mut self, x: f32) -> (f32, f32) {
        self.buf[self.at] = x;
        self.at = (self.at + 1) % self.buf.len();
        self.phase += 0.11 / SR;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        // Far too small and slow to hear as a change in pitch.
        let drift = sine(self.phase) * 6.0;
        let l = self.read(620.0 + drift) + self.read(1010.0 - drift);
        let r = self.read(760.0 - drift) + self.read(1190.0 + drift);
        (x * 0.45 + l * 0.42, x * 0.45 + r * 0.42)
    }
}

/// What the current section asks of the master chain.
#[derive(Clone, Copy)]
pub struct Auto {
    pub dark: f32,
    pub rot: f32,
    pub speed: f32,
    pub rumble: f32,
    pub hall: f32,
    pub gasp: bool,
}

/// One frame of a song before the master chain.
#[derive(Clone, Copy, Default)]
pub struct Mix {
    l: f32,
    r: f32,
    send: f32,
    echo: f32,
    kick: f32,
    /// What is ringing inside your own head, which no amount of muffling can reach.
    ring: f32,
}

/// One note in a dump of the score, for checking what was written.
pub struct ScoreEvent {
    pub time: f32,
    pub inst: Inst,
    pub note: f32,
    pub to: f32,
    pub vel: f32,
    pub gate: f32,
    pub pan: f32,
}

pub struct Player {
    song: Song,
    parsed: Vec<ParsedSection>,
    voices: Vec<Voice>,
    step_samples: f32,
    until_step: f32,
    pub(crate) step: u32,
    section: usize,
    section_step: u32,
    repeat: u32,
    seed: u32,
    lfo: f32,
    duck: f32,
    pump: f32,
    ensemble: Ensemble,
    mass: Mass,
    auto: Auto,
    /// Whether to write the score down rather than play it.
    log: Option<Vec<ScoreEvent>>,
    clock: f32,
}

const MAX_VOICES: usize = 90;

impl Player {
    pub fn new(track: Track, seed: u32) -> Self {
        Self::new_at(track, seed, 0)
    }

    /// Joins a song at the start of one of its sections.
    pub fn new_at(track: Track, seed: u32, section: usize) -> Self {
        let song = song(track);
        let parsed = song.sections.iter().map(parse_section).collect();
        let step_samples = 60.0 / song.bpm / 4.0 * SR;
        let hall = song.hall;
        let section = if section < song.sections.len() { section } else { 0 };
        Player {
            song,
            parsed,
            voices: Vec::with_capacity(MAX_VOICES),
            step_samples,
            until_step: 0.0,
            step: 0,
            section,
            section_step: 0,
            repeat: 0,
            seed: seed | 1,
            lfo: 0.0,
            duck: 0.0,
            pump: 0.5,
            ensemble: Ensemble::new(),
            mass: Mass::new(seed.wrapping_mul(2_654_435_761) | 1),
            auto: Auto { dark: 1.0, rot: 0.0, speed: 1.0, rumble: 0.0, hall, gasp: false },
            log: None,
            clock: 0.0,
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

    /// What a pattern's character asks for: how hard to play, if at all.
    fn accent(c: u8) -> Option<f32> {
        match c {
            b'x' => Some(1.0),
            b'X' => Some(1.3),
            b'o' => Some(0.45),
            _ => None,
        }
    }

    fn spawn_full(&mut self, inst: Inst, note: f32, to: Option<f32>, vel: f32, gate_steps: f32, pan: f32) {
        let gate = gate_steps * self.step_samples / SR;
        if let Some(log) = &mut self.log {
            log.push(ScoreEvent { time: self.clock, inst, note, to: to.unwrap_or(note), vel, gate, pan });
            return;
        }
        if self.voices.len() >= MAX_VOICES {
            // Steal something short and quiet before anything people will notice going missing.
            match self.voices.iter().position(|v| matches!(v.inst, Inst::Hat | Inst::Arp | Inst::OpenHat | Inst::Ride | Inst::Tick)) {
                Some(i) => {
                    self.voices.swap_remove(i);
                }
                None => return,
            }
        }
        let seed = self.rnd();
        let (drive, cut) = (self.song.bass_drive, self.song.pad_cut);
        self.voices.push(Voice::new(inst, midi_hz(note), to.map(midi_hz), vel, gate, pan, seed, drive, cut));
    }

    fn spawn(&mut self, inst: Inst, note: f32, vel: f32, gate_steps: f32, pan: f32) {
        self.spawn_full(inst, note, None, vel, gate_steps, pan);
    }

    /// A pattern lane at this step: it loops at its own length.
    fn lane(p: &str, section_step: u32) -> u8 {
        if p.is_empty() {
            b'.'
        } else {
            p.as_bytes()[section_step as usize % p.len()]
        }
    }

    fn tick(&mut self) {
        let parsed = std::mem::take(&mut self.parsed);
        self.tick_with(&parsed);
        self.parsed = parsed;
    }

    fn tick_with(&mut self, parsed: &[ParsedSection]) {
        let section_count = self.song.sections.len();
        let sec: &'static Section = &self.song.sections[self.section];
        let p = &parsed[self.section];
        let (bar_index, bar_step) = ((self.section_step / BAR) as usize, self.section_step % BAR);
        let bars = p.chords.len().max(1);
        let (root, tones) = p.chords[bar_index % bars];
        let on = |pattern: &str| Self::lane(pattern, bar_step);
        let section_len = p.len;
        let last_bar = self.section_step + BAR >= section_len;
        let last_repeat = self.repeat + 1 >= sec.repeats;
        let e = sec.energy;
        // Louder passages are played harder, not just mixed higher.
        let orch = 0.55 + 0.45 * e;
        let night = self.song.low < 0.0;

        // Where things stand, for anything that moves across the section.
        let total = section_len * sec.repeats;
        let k = (self.repeat * section_len + self.section_step) as f32 / total as f32;
        let along = |pair: (f32, f32)| pair.0 + (pair.1 - pair.0) * k;
        self.step_samples = 60.0 / (self.song.bpm * along(sec.tempo)) / 4.0 * SR;
        let gasping = sec.gasp && last_repeat && self.section_step + 4 >= section_len;
        self.pump = sec.pump;
        self.auto = Auto {
            dark: along(sec.dark),
            rot: along(sec.rot),
            speed: along(sec.speed),
            rumble: along(sec.rumble),
            hall: if sec.hall > 0.0 { sec.hall } else { self.song.hall },
            gasp: gasping,
        };
        self.mass.cue(&sec.mass, k);

        if !gasping {
            if bar_step == 0 {
                self.mass.chord(root, tones);
            }
            let slam = if sec.mass.tutti == ONCE {
                self.section_step == 0 && self.repeat == 0
            } else {
                Self::lane(sec.mass.tutti, self.section_step) == b'x'
            };
            if slam {
                self.mass.tutti();
            }
            self.mass.open = sec.mass.chop.is_empty() || Self::lane(sec.mass.chop, self.section_step) != b'.';
            // The sustained chord, one per bar, spread across the stereo field.
            if bar_step == 0 {
                let base = 48.0 + root;
                let oct = if base > 53.0 { -12.0 } else { 0.0 };
                let middle = base + oct + 12.0;
                let whole = BAR as f32;
                if sec.pad == 2 {
                    for (i, t) in tones.iter().enumerate().take(3) {
                        self.spawn(Inst::Pad, middle + t, 0.2 * orch, whole, [-0.5, 0.0, 0.5][i]);
                    }
                    self.spawn(Inst::Pad, base + oct, 0.18 * orch, whole, 0.25);
                    if e >= 0.8 {
                        // Everything in: the pad opens up an octave above.
                        self.spawn(Inst::Pad, middle + 12.0 + tones[0], 0.11 * orch, whole, -0.6);
                        self.spawn(Inst::Pad, middle + 12.0 + tones[2], 0.1 * orch, whole, -0.4);
                    }
                } else if sec.pad == 1 {
                    self.spawn(Inst::Pad, base + oct, 0.2 * orch, whole, 0.0);
                    self.spawn(Inst::Pad, middle + 7.0, 0.16 * orch, whole, -0.4);
                    self.spawn(Inst::Pad, middle + 12.0, 0.14 * orch, whole, 0.4);
                }
            }
            // Drums. The last bar of a section with a fill hands over to the drums.
            let fill_now = sec.fill && last_bar && bar_step >= 8;
            if fill_now {
                let k = (bar_step - 8) as f32 / 8.0;
                if bar_step % 2 == 0 {
                    let i = ((bar_step - 8) / 2) as usize;
                    self.spawn(Inst::Tom, [45.0, 43.0, 40.0, 38.0][i], 0.65 + k * 0.3, 1.0, [0.3, 0.1, -0.1, -0.3][i]);
                }
                if !night {
                    self.spawn(Inst::Snare, 40.0, 0.35 + k * 0.5, 1.0, 0.05);
                }
            }
            if let Some(a) = Self::accent(on(sec.kick)) {
                self.spawn(Inst::Kick, 40.0, a * (0.4 + 0.45 * e), 1.0, 0.0);
                self.duck = self.duck.max(a.min(1.0));
            }
            if let Some(a) = Self::accent(on(sec.sub)) {
                self.spawn(Inst::Sub, 30.0, 0.85 * a, 1.0, 0.0);
                self.duck = self.duck.max(a.min(1.0));
            }
            if !fill_now {
                if let Some(a) = Self::accent(on(sec.snare)) {
                    self.spawn(Inst::Snare, 40.0, 0.8 * a, 1.0, 0.05);
                }
            }
            if let Some(a) = Self::accent(on(sec.hat)) {
                self.spawn(Inst::Hat, 40.0, 0.28 * a, 1.0, 0.0);
            }
            if let Some(a) = Self::accent(on(sec.ride)) {
                self.spawn(Inst::Ride, 40.0, 0.3 * a, 1.0, 0.3);
            }
            if on(sec.tom) == b'x' {
                // The electronic toms follow the chord, low.
                let mut note = 31.0 + root;
                if note < 36.0 {
                    note += 12.0;
                }
                let strong = if bar_step == 0 { 1.0 } else { 0.72 };
                self.spawn(Inst::SynTom, note, 0.62 * orch * strong, 1.0, 0.2);
            }
            // A new section is announced with a crash, once there is a kit playing, in the daytime.
            if !night && self.section_step == 0 && !sec.kick.is_empty() {
                self.spawn(Inst::Crash, 40.0, 0.5, 1.0, 0.25);
            }
            // Rising into the next section.
            if sec.swell && last_bar && bar_step == 0 {
                self.spawn(Inst::Riser, 40.0, 0.5, BAR as f32, -0.2);
            }
            // Bass: digits are chord degrees.
            let c = on(sec.bass);
            if c != b'.' {
                let len = if sec.kick.is_empty() { 14.0 } else { 0.9 };
                self.spawn(Inst::Bass, 36.0 + root + degree(c, &tones), 0.44, len, 0.0);
            }
            // The driving low sequence, doubled an octave down when the arrangement digs in.
            let c = on(sec.low);
            if c != b'.' {
                let mut note = 48.0 + root;
                if note > 55.0 {
                    note -= 12.0;
                }
                let note = note + degree(c, &tones);
                let accent = if bar_step % 4 == 0 { 1.0 } else { 0.78 };
                self.spawn(Inst::Arp, note, 0.42 * orch * accent, 1.0, 0.35);
                if e >= 0.7 {
                    self.spawn(Inst::Arp, note - 12.0, 0.34 * orch * accent, 1.0, 0.1);
                }
            }
            // The arpeggio climbs chord tones across octaves.
            let c = on(sec.arp);
            if c.is_ascii_digit() {
                let d = (c - b'0') as usize;
                let t = tones[d % 3] + 12.0 * (d / 3) as f32;
                let pan = if bar_step % 2 == 0 { -0.45 } else { 0.45 };
                let (inst, vel) = if self.song.arp_inst == Inst::Pluck { (Inst::Pluck, 0.38) } else { (Inst::Arp, 0.3 * orch) };
                self.spawn(inst, 60.0 + self.song.low * 0.5 + root + t, vel, 1.0, pan);
            }
            // The Rot Chord: a stab with a semitone grinding away at the top of it.
            if on(sec.stab) == b'x' {
                let base = 60.0 + self.song.low * 0.5 + root;
                for (i, t) in tones.iter().enumerate().take(3) {
                    self.spawn(Inst::Stab, base + t, 0.2, 1.0, [-0.35, 0.0, 0.35][i]);
                }
                self.spawn(Inst::Stab, base - 12.0, 0.18, 1.0, 0.0);
                self.spawn(Inst::Stab, base + 13.0, 0.13, 1.0, 0.5);
            }
            // The melody.
            let lead_inst = self.song.lead_inst;
            let lead_vel = match lead_inst {
                Inst::Bell => 0.72,
                Inst::Pluck => 0.4,
                _ => 0.46,
            };
            let low = self.song.low;
            for (line, inst, vel, pan, shift) in [(&p.lead, lead_inst, lead_vel, 0.0, low), (&p.counter, Inst::SynthStrings, 0.2, -0.35, 0.0), (&p.brass, Inst::Brass, 0.3, 0.3, low * 0.5)] {
                if line.len == 0 {
                    continue;
                }
                let pos = self.section_step % line.len;
                for ev in line.events.iter().filter(|ev| ev.step == pos) {
                    let (note, len, vel) = (ev.note + shift, ev.len as f32, vel * orch);
                    self.spawn_full(inst, note, None, vel, len - 0.2, pan);
                    // At the peaks the riff is doubled an octave up, by brass.
                    if inst == Inst::Riff && e >= 0.85 {
                        self.spawn_full(Inst::Brass, note + 12.0, None, vel * 0.5, len - 0.2, -0.15);
                    }
                }
            }
            // Everything else the section has written down.
            for part in &p.parts {
                match &part.run {
                    PartRun::Line(l) => {
                        if l.len > 0 {
                            // Long lines carry on across the section's repeats.
                            let pos = (self.repeat * section_len + self.section_step) % l.len;
                            for ev in l.events.iter().filter(|ev| ev.step == pos) {
                                self.spawn_full(part.inst, ev.note, ev.to, part.vel, ev.len as f32 - 0.1, part.pan);
                            }
                        }
                    }
                    PartRun::Hits(note, pattern) => {
                        let c = pattern[(self.repeat * section_len + self.section_step) as usize % pattern.len()];
                        if let Some(a) = Self::accent(c) {
                            if matches!(part.inst, Inst::Kick | Inst::Sub) {
                                self.duck = self.duck.max(a.min(1.0));
                            }
                            self.spawn_full(part.inst, *note, None, part.vel * a, 1.0, part.pan);
                        }
                    }
                }
            }
        }

        // Advance the song position.
        self.step += 1;
        self.section_step += 1;
        if self.section_step >= section_len {
            self.section_step = 0;
            self.repeat += 1;
            if self.repeat >= sec.repeats {
                self.repeat = 0;
                self.section += 1;
                if self.section >= section_count {
                    self.section = self.song.loop_to.min(section_count - 1);
                }
            }
        }
    }

    /// Mix one stereo frame.
    #[inline]
    pub fn frame(&mut self, gains: &[f32; BUSES]) -> Mix {
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
        let side = 1.0 - self.pump * self.duck * self.duck;
        let mut mix = Mix::default();
        let mut section = 0.0;
        let mut i = 0;
        while i < self.voices.len() {
            let v = &mut self.voices[i];
            let s = v.sample(lfo);
            let inst = v.inst;
            let bus = bus_of(inst);
            let mut g = gains[bus];
            if bus != BUS_KICK && bus != BUS_PERC && bus != BUS_ATMOS {
                g *= side;
            }
            let x = s * g;
            let (pl, pr) = (0.5 - v.pan * 0.5, 0.5 + v.pan * 0.5);
            let finished = v.t > v.duration();
            if inst == Inst::Ring {
                mix.ring += x;
            } else {
                if is_section(inst) {
                    section += x;
                }
                let dry = if is_section(inst) { 0.55 } else { 1.0 };
                mix.l += x * pl * 1.4 * dry;
                mix.r += x * pr * 1.4 * dry;
                // Long tails on the huge things; the kick and bass stay dry and close.
                mix.send += x * match inst {
                    Inst::Hull => 0.8,
                    Inst::Pad => 0.75,
                    Inst::Slam | Inst::Horn | Inst::Bell | Inst::Pluck | Inst::Collapse => 0.6,
                    Inst::Stab | Inst::Anvil | Inst::Wind => 0.5,
                    Inst::SynthStrings | Inst::Brass => 0.45,
                    Inst::Riser => 0.4,
                    Inst::Arp | Inst::Tick | Inst::Riff => 0.25,
                    Inst::SynTom | Inst::Tom | Inst::Crash | Inst::Snare | Inst::Hum | Inst::Geiger => 0.22,
                    Inst::OpenHat | Inst::Ride => 0.15,
                    Inst::Hat | Inst::Heart => 0.05,
                    _ => 0.0,
                };
                mix.echo += x * match inst {
                    Inst::Bell | Inst::Pluck => 0.5,
                    Inst::Anvil => 0.4,
                    Inst::Arp => 0.3,
                    Inst::Tick => 0.28,
                    _ => 0.0,
                };
                if matches!(inst, Inst::Kick | Inst::Sub) {
                    mix.kick += x;
                }
            }
            if finished {
                self.voices.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let (el, er) = self.ensemble.process(section);
        mix.l += el * 0.8;
        mix.r += er * 0.8;
        let (ml, mr) = self.mass.next();
        let g = gains[BUS_MASS] * side;
        mix.l += ml * g;
        mix.r += mr * g;
        mix.send += (ml + mr) * g * 0.3;
        mix
    }

    pub fn bpm(&self) -> f32 {
        self.song.bpm
    }

    pub(crate) fn voice_count(&self) -> usize {
        self.voices.len()
    }
}

/// Writes down everything one full pass of a song plays, instead of playing it.
pub fn dump_score(track: Track) -> (Vec<ScoreEvent>, Vec<(f32, usize, &'static str)>) {
    let mut p = Player::new(track, 77);
    p.log = Some(Vec::new());
    let mut marks = Vec::new();
    let seconds = track.loop_seconds();
    while p.clock < seconds {
        if p.section_step == 0 && p.repeat == 0 {
            marks.push((p.clock, p.section, p.song.sections[p.section].chords));
        }
        p.tick();
        p.clock += p.step_samples / SR;
    }
    (p.log.take().unwrap_or_default(), marks)
}

/// Maps game intensity to layer gains.
pub fn layer_gains(track: Track, intensity: f32) -> [f32; BUSES] {
    let ss = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let i = intensity;
    match track {
        Track::Menu | Track::GameOver | Track::RestWell => [0.95, 0.6, 0.6, 0.8, 0.75, 0.7, 0.7, 1.0, 1.0],
        Track::Build => [0.85, 0.55, 0.7, 0.75, 0.6, 0.5, 0.5, 0.8, 0.8],
        Track::Rot => [0.9, 0.7, 0.5, 0.85, 0.68, 0.75, 0.7, 1.0, 1.0],
        Track::OneOfMany => [1.0, 0.8, 0.5, 0.85, 0.75, 0.7, 0.8, 1.0, 1.0],
        Track::Smitherines => [0.65, 0.85, 0.6, 0.85, 0.82, 0.75, 0.8, 1.0, 1.0],
        _ => [
            0.75,
            0.6 + 0.35 * ss(0.0, 0.25, i),
            0.6 + 0.3 * ss(0.1, 0.45, i),
            0.45 + 0.55 * ss(0.2, 0.6, i),
            0.65 + 0.3 * ss(0.0, 0.2, i),
            0.55 + 0.4 * ss(0.1, 0.45, i),
            0.45 + 0.55 * ss(0.3, 0.7, i),
            0.9,
            // The closer the horde gets, the bigger the wall.
            0.5 + 0.5 * ss(0.1, 0.8, i),
        ],
    }
}

/// How much the fight is winding up the Wall, on top of the score's own instructions.
fn heat_of(track: Track, intensity: f32) -> f32 {
    match track {
        Track::CombatA | Track::CombatB | Track::CombatC | Track::Boss => intensity,
        _ => 0.0,
    }
}

/// The long low bed that dark techno is built on: the kick, distorted and left to ring in a small
/// space, with only the very bottom of it kept.
struct Rumble {
    a: Delay,
    b: Delay,
    low: Svf,
    high: Svf,
}

impl Rumble {
    fn new() -> Self {
        Rumble { a: Delay::new(2087), b: Delay::new(3137), low: Svf::default(), high: Svf::default() }
    }

    fn clear(&mut self) {
        self.a.clear();
        self.b.clear();
    }

    #[inline]
    fn process(&mut self, kick: f32) -> f32 {
        let x = soft_clip(kick * 2.5);
        let (a, b) = (self.a.tap(), self.b.tap());
        self.a.push(x * 0.5 + b * 0.86);
        self.b.push(x * 0.5 + a * 0.84);
        let low = self.low.process(a + b, 135.0, 0.9).lp;
        soft_clip(self.high.process(low, 40.0, 0.7).hp * 1.5) * 0.2
    }
}

/// The music physically rotting: resolution falling away, and pieces going missing.
struct Decay {
    hold: (f32, f32),
    count: u32,
    gain: f32,
    target: f32,
    timer: u32,
    rng: u32,
}

impl Decay {
    fn new() -> Self {
        Decay { hold: (0.0, 0.0), count: 0, gain: 1.0, target: 1.0, timer: 0, rng: 0x51ed_270b }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, rot: f32) -> (f32, f32) {
        if rot < 0.001 {
            self.gain = 1.0;
            return (l, r);
        }
        self.count += 1;
        if self.count >= 1 + (rot * rot * 20.0) as u32 {
            self.count = 0;
            self.hold = (l, r);
        }
        let levels = fexp((11.0 - 7.5 * rot) * std::f32::consts::LN_2);
        let wet = (rot * 1.6).min(1.0);
        let cl = (self.hold.0 * levels).round() / levels;
        let cr = (self.hold.1 * levels).round() / levels;
        if self.timer == 0 {
            if self.rand() < rot * 0.7 {
                self.target = self.rand() * 0.25;
                self.timer = ((0.03 + self.rand() * 0.25 * rot) * SR) as u32;
            } else {
                self.target = 1.0;
                self.timer = ((0.1 + self.rand() * (0.9 - rot * 0.7)) * SR) as u32;
            }
        } else {
            self.timer -= 1;
        }
        self.gain += (self.target - self.gain) * 0.004;
        ((l + (cl - l) * wet) * self.gain, (r + (cr - r) * wet) * self.gain)
    }
}

const TAPE_FRAMES: usize = 44100 * 4;

/// A loop of tape running behind the mix, so that when the power fails it can run down to nothing.
struct Tape {
    l: Vec<f32>,
    r: Vec<f32>,
    at: usize,
    lag: f32,
    /// Sliding back to the present when the slowdown is over.
    catching: f32,
}

impl Tape {
    fn new() -> Self {
        Tape { l: vec![0.0; TAPE_FRAMES], r: vec![0.0; TAPE_FRAMES], at: 0, lag: 0.0, catching: 0.0 }
    }

    fn clear(&mut self) {
        self.lag = 0.0;
        self.catching = 0.0;
    }

    #[inline]
    fn read(&self, lag: f32) -> (f32, f32) {
        let pos = self.at as f32 + TAPE_FRAMES as f32 - lag;
        let i = pos as usize;
        let f = pos - i as f32;
        let (a, b) = (i % TAPE_FRAMES, (i + 1) % TAPE_FRAMES);
        (self.l[a] + (self.l[b] - self.l[a]) * f, self.r[a] + (self.r[b] - self.r[a]) * f)
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, speed: f32) -> (f32, f32) {
        self.l[self.at] = l;
        self.r[self.at] = r;
        let out = if speed >= 0.9995 {
            if self.lag > 0.0 {
                // Back to the present, with a quick fade across the jump.
                self.catching += 1.0 / 441.0;
                let k = self.catching.min(1.0);
                let old = self.read(self.lag);
                if self.catching >= 1.0 {
                    self.lag = 0.0;
                    self.catching = 0.0;
                }
                (old.0 + (l - old.0) * k, old.1 + (r - old.1) * k)
            } else {
                (l, r)
            }
        } else {
            self.catching = 0.0;
            self.lag = (self.lag + 1.0 - speed).clamp(0.0, TAPE_FRAMES as f32 - 8.0);
            self.read(self.lag)
        };
        self.at = (self.at + 1) % TAPE_FRAMES;
        out
    }
}

/// Plays a track, crossfades on change, and runs the master chain.
pub struct Music {
    current: (Player, Track),
    outgoing: Option<(Player, Track)>,
    fade: f32,
    /// A cut in progress, and how much of the old sound is left.
    cutting_to: Option<Track>,
    cut_gain: f32,
    pub intensity: f32,
    target_intensity: f32,
    delay_l: Delay,
    delay_r: Delay,
    reverb: Reverb,
    rumble: Rumble,
    decay: Decay,
    tape: Tape,
    dark_l: Svf,
    dark_r: Svf,
    dark_coef: SvfCoef,
    dc: [DcBlock; 2],
    /// The master chain's settings, easing towards what the score asks for.
    dark: f32,
    rot: f32,
    speed: f32,
    rumble_amount: f32,
    hall: f32,
    silence: f32,
    gains: [f32; BUSES],
    old_gains: [f32; BUSES],
    control: u32,
    /// Where to pick each song back up, so the fight hears more than its opening every wave.
    resume: [usize; 11],
    seed: u32,
    pub volume: f32,
    /// Keeps only one layer, for checking the mix.
    pub solo: Option<usize>,
    /// How far the music is stepping back for the game's own sounds, and how far it is being asked to.
    room: f32,
    pub room_wanted: f32,
}

const FADE_SECONDS: f32 = 2.4;
/// A cut is immediate to the ear. This is only long enough to stop the speakers clicking.
const CUT_SECONDS: f32 = 0.006;
const CONTROL_FRAMES: u32 = 32;

impl Music {
    pub fn new(track: Track) -> Self {
        let mut m = Music {
            current: (Player::new(track, 77), track),
            outgoing: None,
            cutting_to: None,
            cut_gain: 1.0,
            fade: 1.0,
            intensity: 0.0,
            target_intensity: 0.0,
            delay_l: Delay::new(44100),
            delay_r: Delay::new(44100),
            reverb: Reverb::new(),
            rumble: Rumble::new(),
            decay: Decay::new(),
            tape: Tape::new(),
            dark_l: Svf::default(),
            dark_r: Svf::default(),
            dark_coef: SvfCoef::new(20000.0, 0.7),
            dc: [DcBlock::default(); 2],
            dark: 1.0,
            rot: 0.0,
            speed: 1.0,
            rumble_amount: 0.0,
            hall: 0.84,
            silence: 1.0,
            gains: layer_gains(track, 0.0),
            old_gains: [0.0; BUSES],
            control: 0,
            resume: [0; 11],
            seed: 991,
            volume: 0.8,
            solo: None,
            room: 1.0,
            room_wanted: 1.0,
        };
        m.update_delay();
        m
    }

    fn update_delay(&mut self) {
        let beat = 60.0 / self.current.0.bpm() * SR;
        self.delay_l.set_len((beat * 0.75) as usize);
        self.delay_r.set_len((beat * 0.5) as usize);
    }

    fn index_of(track: Track) -> usize {
        Track::ALL.iter().position(|t| *t == track).unwrap_or(0)
    }

    /// Remembers how far this song got, unless it is one that should always start at the top.
    fn remember(&mut self) {
        let (player, track) = &self.current;
        if !track.is_death() && *track != Track::Menu {
            let next = player.section + 1;
            let song_len = player.song.sections.len();
            self.resume[Self::index_of(*track)] = if next < song_len { next } else { player.song.loop_to };
        }
    }

    /// Changes track the way that track wants: death tracks cut in, the rest fade.
    pub fn set_track(&mut self, track: Track) {
        if track.is_death() || self.cutting_to.is_some() {
            return self.cut_to(track);
        }
        if self.current.1 == track {
            return;
        }
        // Leaving the title screen means a new run, which gets the whole soundtrack from the top.
        if self.current.1 == Track::Menu {
            self.resume = [0; 11];
        }
        self.remember();
        self.seed = self.seed.wrapping_mul(31).wrapping_add(7);
        let from = self.resume[Self::index_of(track)];
        let new = (Player::new_at(track, self.seed, from), track);
        self.outgoing = Some(std::mem::replace(&mut self.current, new));
        self.old_gains = self.gains;
        self.fade = 0.0;
        self.update_delay();
    }

    /// Drops whatever is playing, echoes and all, and starts this track from the top.
    pub fn cut_to(&mut self, track: Track) {
        if self.cutting_to == Some(track) || (self.cutting_to.is_none() && self.current.1 == track) {
            return;
        }
        self.remember();
        self.cutting_to = Some(track);
    }

    fn finish_cut(&mut self, track: Track) {
        self.seed = self.seed.wrapping_mul(31).wrapping_add(7);
        self.current = (Player::new(track, self.seed), track);
        self.outgoing = None;
        self.fade = 1.0;
        self.cutting_to = None;
        self.cut_gain = 1.0;
        self.intensity = self.target_intensity;
        self.delay_l.clear();
        self.delay_r.clear();
        self.reverb.clear();
        self.rumble.clear();
        self.tape.clear();
        self.dark = 1.0;
        self.rot = 0.0;
        self.speed = 1.0;
        self.rumble_amount = 0.0;
        self.silence = 1.0;
        self.control = 0;
        self.update_delay();
    }

    pub fn set_intensity(&mut self, v: f32) {
        self.target_intensity = v.clamp(0.0, 1.0);
    }

    #[cfg(test)]
    pub fn track(&self) -> Track {
        self.current.1
    }

    #[cfg(test)]
    pub fn is_crossfading(&self) -> bool {
        self.outgoing.is_some()
    }

    #[cfg(test)]
    pub fn position(&self) -> u32 {
        self.current.0.step
    }

    pub fn voices_playing(&self) -> usize {
        self.current.0.voice_count()
    }

    #[cfg(test)]
    pub fn section(&self) -> usize {
        self.current.0.section
    }

    /// Things that only need adjusting a little over a thousand times a second.
    fn control(&mut self) {
        self.intensity += (self.target_intensity - self.intensity) * (CONTROL_FRAMES as f32 / (SR * 1.2));
        let track = self.current.1;
        self.gains = layer_gains(track, self.intensity);
        if let Some(keep) = self.solo {
            for (i, g) in self.gains.iter_mut().enumerate() {
                if i != keep {
                    *g = 0.0;
                }
            }
        }
        self.current.0.mass.heat = heat_of(track, self.intensity);
        let a = self.current.0.auto;
        let ease = 0.03;
        self.dark += (a.dark - self.dark) * ease;
        self.rot += (a.rot - self.rot) * ease;
        // A tape stop has to follow the score closely, or it wouldn't finish in time.
        self.speed += (a.speed - self.speed) * 0.2;
        self.rumble_amount += (a.rumble - self.rumble_amount) * ease;
        self.hall += (a.hall - self.hall) * 0.01;
        // Out of the way quickly, and back slowly enough that nobody notices it having moved.
        let rate = if self.room_wanted < self.room { 0.1 } else { 0.002 };
        self.room += (self.room_wanted - self.room) * rate;
        if self.dark < 0.995 {
            // From about 120 Hz with the filter shut to the top of hearing with it open.
            self.dark_coef = SvfCoef::new(120.0 * fexp(self.dark * 5.2), 0.75);
        }
    }

    /// Adds into an interleaved stereo buffer.
    pub fn render_add(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            if let Some(track) = self.cutting_to {
                self.cut_gain -= 1.0 / (CUT_SECONDS * SR);
                if self.cut_gain <= 0.0 {
                    self.finish_cut(track);
                }
            }
            if self.control == 0 {
                self.control();
            }
            self.control = (self.control + 1) % CONTROL_FRAMES;
            let mut m = self.current.0.frame(&self.gains);
            if self.fade < 1.0 {
                self.fade = (self.fade + 1.0 / (FADE_SECONDS * SR)).min(1.0);
                let k_in = sine(self.fade * 0.25);
                let k_out = sine(0.25 + self.fade * 0.25);
                m.l *= k_in;
                m.r *= k_in;
                m.send *= k_in;
                m.echo *= k_in;
                m.kick *= k_in;
                m.ring *= k_in;
                if let Some((p, _)) = &mut self.outgoing {
                    let o = p.frame(&self.old_gains);
                    m.l += o.l * k_out;
                    m.r += o.r * k_out;
                    m.send += o.send * k_out;
                    m.echo += o.echo * k_out;
                    m.kick += o.kick * k_out;
                    m.ring += o.ring * k_out;
                }
            } else if self.outgoing.is_some() {
                self.outgoing = None;
            }
            let (dl, dr) = (self.delay_l.tap(), self.delay_r.tap());
            self.delay_l.push(m.echo * 0.5 + dr * 0.38);
            self.delay_r.push(m.echo * 0.5 + dl * 0.38);
            let (rl, rr) = self.reverb.process(m.send * 0.4, self.hall, 0.3);
            let mut rum = 0.0;
            if self.rumble_amount > 0.002 {
                rum = self.rumble.process(m.kick) * self.rumble_amount;
                let duck = self.current.0.duck;
                rum *= 1.0 - self.current.0.pump * duck * duck;
            }
            let (mut l, mut r) = (m.l + dl * 0.45 + rl + rum, m.r + dr * 0.45 + rr + rum);
            (l, r) = self.decay.process(l, r, self.rot);
            if self.dark < 0.995 {
                l = self.dark_l.tick(l, &self.dark_coef).lp;
                r = self.dark_r.tick(r, &self.dark_coef).lp;
            }
            let gasp = self.current.0.auto.gasp;
            (l, r) = self.tape.process(l, r, self.speed);
            self.silence += ((if gasp { 0.0 } else { 1.0 }) - self.silence) * 0.02;
            let gain = self.volume * self.cut_gain * self.silence * self.room;
            frame[0] += (self.dc[0].tick(l) + m.ring) * gain;
            frame[1] += (self.dc[1].tick(r) + m.ring) * gain;
        }
    }
}
