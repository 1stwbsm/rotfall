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
        let steps: u32 = s.sections.iter().map(|sec| {
            let p = parse_section(sec);
            let bars = p.chords.len().max(1) as u32;
            (bars * 16).max(p.lead_len.div_ceil(16) * 16) * sec.repeats
        }).sum();
        steps as f32 * 60.0 / s.bpm / 4.0
    }
}

pub struct Section {
    /// One chord per bar.
    pub chords: &'static str,
    // 16-step patterns.
    pub kick: &'static str,
    pub snare: &'static str,
    pub hat: &'static str,
    pub ride: &'static str,
    pub bass: &'static str,
    pub acid: &'static str,
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
    pub choir: &'static str,
    pub repeats: u32,
    /// A noise riser and a rising line in the last bar, leading into the next section.
    pub swell: bool,
    /// A drum fill in the last bar.
    pub fill: bool,
    /// How hard the arrangement pushes, 0..1.
    pub energy: f32,
}

pub struct Song {
    pub bpm: f32,
    pub swing: f32,
    pub pad_cut: f32,
    pub bass_drive: f32,
    pub lead_inst: Inst,
    pub arp_inst: Inst,
    /// Where the song returns to after its last section, so that intros only play once.
    pub loop_to: usize,
    pub sections: &'static [Section],
}

const O: &str = "";

const E: Section = Section {
    chords: "", kick: O, snare: O, hat: O, ride: O, bass: O, acid: O, arp: O, low: O, tom: O, sub: O,
    stab: O, lead: O, counter: O, brass: O, choir: O, repeats: 1, swell: false, fill: false, energy: 0.8,
};

// DESCENT. A bell over slow pads, rising to the full arrangement.
static MENU: &[Section] = &[
    Section {
        chords: "Am Am F F", lead: "A4:16 C5:16 A4:16 F4:16", energy: 0.4, ..E
    },
    Section {
        chords: "Am F C G", bass: "0...............",
        counter: "A3:16 F3:16 G3:16 G3:16", energy: 0.55,
        lead: "E5:8 A5:4 G5:4 F5:8 E5:8 G5:6 E5:2 C5:8 D5:8 B4:8", repeats: 2, ..E
    },
    Section {
        chords: "Dm Am F E", bass: "0.......5.......",
        arp: "0...2...1...2...", counter: "D4:16 C4:16 C4:16 B3:16", energy: 0.65, swell: true,
        lead: "F5:4 E5:4 D5:8 C5:4 E5:4 A4:8 C5:4 D5:4 F5:8 E5:12 G#4:4", ..E
    },
    Section {
        chords: "Am F C G", kick: "x.......x.......", bass: "0.......0.......", choir: "x",
        tom: "x...............", low: "0...0...0...0...", brass: "E4:16 F4:16 G4:16 G4:16", energy: 1.0,
        lead: "A5:8 C6:4 B5:4 A5:8 F5:8 G5:6 E5:2 C5:8 D5:8 B4:8", ..E
    },
    Section {
        chords: "Dm Am E Am", bass: "0...............", counter: "D4:16 C4:16 B3:16 A3:16", energy: 0.5,
        lead: "F5:16 E5:16 E5:8 G#4:8 A4:16", ..E
    },
];

// DAYTIME ESCAPE. Plucks and string machine in the open air, gathering as the break runs out.
static BUILD: &[Section] = &[
    Section {
        chords: "Em C G D", arp: "0.1.2.3.4.3.2.1.", counter: "B4:16 G4:16 B4:16 A4:16", energy: 0.45, ..E
    },
    Section {
        chords: "Em C G D",
        kick: "x.......x.......", snare: "....x.......x...", hat: "..x...x...x...x.",
        bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.", low: "0...0...0...0...", energy: 0.6,
        lead: "-:16 -:16 -:16 -:16", repeats: 2, ..E
    },
    Section {
        chords: "Em C G D",
        kick: "x.......x.......", snare: "....x.......x...", hat: "..x...x...x...x.",
        bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.", low: "0...0...0...0...",
        counter: "G4:16 E4:16 D4:16 F#4:16", energy: 0.75,
        lead: "B4:3 E5:3 G5:2 F#5:4 E5:4 E5:3 C5:3 E5:2 G5:8 D5:3 G5:3 B5:2 A5:4 G5:4 F#5:6 D5:2 A4:8",
        repeats: 2, ..E
    },
    Section {
        chords: "C G Am Em",
        kick: "x.......x.......", hat: "..x...x...x...x.", bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.",
        brass: "G4:8 C5:8 B4:8 D5:8 C5:8 A4:8 G4:8 B4:8", energy: 0.8, swell: true, ..E
    },
    Section {
        chords: "Am C Em D",
        kick: "x...x...x...x...", snare: "....x.......x..x", hat: "x.x.x.x.x.x.x.x.",
        bass: "0.0.3.0.5.0.3.0.", arp: "4.3.2.1.0.1.2.3.", tom: "x.......x.......",
        counter: "E5:16 E5:16 G5:8 B5:8 F#5:16", energy: 0.9, fill: true,
        lead: "A5:4 C6:4 B5:4 A5:4 G5:6 E5:2 G5:8 B5:4 G5:4 E5:4 D5:4 F#5:12 -:4",
        ..E
    },
    Section {
        chords: "Em C G D",
        kick: "x...x...x...x...", snare: "....x.......x...", hat: "x.x.x.x.x.x.x.x.", ride: "x...x...x...x...",
        bass: "0.0.3.0.5.0.3.0.", arp: "0.1.2.3.4.3.2.1.", low: "0.0.0.0.0.0.0.0.", tom: "x...............",
        counter: "B5:16 C6:16 B5:16 A5:16", brass: "E4:16 E4:16 D4:16 D4:16", choir: "x", energy: 1.0,
        lead: "B4:3 E5:3 G5:2 F#5:4 E5:4 E5:3 C5:3 E5:2 G5:8 D5:3 G5:3 B5:2 A5:4 G5:4 F#5:6 D5:2 A4:8",
        ..E
    },
];

// HOLDOUT. Four-to-the-floor under a driving sequence and a synth brass call to arms.
static COMBAT_A: &[Section] = &[
    Section {
        chords: "Dm Dm Bb C", hat: "..x...x...x...x.", tom: "x...x...x...x...",
        low: "0.00.00.0.00.00.", arp: "0.1.2.0.1.2.0.1.", energy: 0.6, swell: true, ..E
    },
    Section {
        chords: "Dm Dm Bb C",
        kick: "x...x...x...x...", snare: "....x.......x...", hat: "..x...x...x...x.",
        bass: "0.00.00.0.00.03.", arp: "0.1.2.0.1.2.0.1.", low: "0.00.00.0.00.00.", tom: "x...............",
        lead: "-:16 -:16 -:16 -:16", energy: 0.7, repeats: 2, ..E
    },
    Section {
        chords: "Dm Bb F C",
        kick: "x...x...x...x...", snare: "....x.......x.x.", hat: "x.x.x.x.x.x.x.xx",
        bass: "0.00.00.0.00.03.", arp: "0.1.2.3.4.3.2.1.", stab: "x..x..x.........", low: "0.00.00.0.00.00.",
        counter: "A5:16 A#5:16 A5:16 G5:16", energy: 0.85,
        lead: "D5:3 F5:3 A5:2 G5:4 F5:4 F5:3 D5:3 F5:2 A#5:8 A5:3 F5:3 C5:2 F5:4 A5:4 G5:6 E5:2 C5:8",
        repeats: 2, ..E
    },
    Section {
        chords: "Gm Dm Bb A",
        kick: "x...x...x...x.x.", snare: "....x.......x..x", hat: "x.xxx.xxx.xxx.xx", ride: "x...x...x...x...",
        bass: "0.0.00.30.0.00.5", arp: "2.1.0.1.2.3.4.3.", stab: "x.....x.....x...", tom: "x.......x.......",
        brass: "G4:16 F4:16 F4:16 E4:16", energy: 0.9, fill: true,
        lead: "G5:2 A5:2 A#5:4 A5:2 G5:2 F5:4 A5:6 D5:2 F5:8 A#5:4 A5:4 G5:4 F5:4 E5:6 C#5:2 A4:8",
        repeats: 2, ..E
    },
    Section {
        chords: "Dm Bb F C", choir: "x", low: "0.0.0.0.0.0.0.0.", hat: "....x.......x...",
        brass: "D4:16 F4:16 F4:8 A4:8 G4:16", energy: 0.75, swell: true, ..E
    },
    Section {
        chords: "Dm Dm Bb A",
        kick: "x.x.x.x.x.x.x.x.", snare: "....x.......xxxx", hat: "xxxxxxxxxxxxxxxx", ride: "x...x...x...x...",
        bass: "01010101010101.5", arp: "0123432101234321", stab: "x..x..x.x..x..x.", choir: "x",
        acid: "0.0.1.0.0.3.0.5.", sub: "x...............", tom: "x...x...x...x...",
        counter: "A5:16 F5:16 F5:16 E5:16", brass: "D4:16 D4:16 D4:16 C#4:16", energy: 1.0,
        lead: "A5:2 D6:2 C6:2 A5:2 F5:2 A5:2 D6:4 A#5:2 D6:2 C6:2 A#5:2 F5:2 A#5:2 D6:4 A#5:2 F5:2 D5:2 F5:2 A#5:4 C6:4 C#6:8 A5:4 E5:4",
        repeats: 2, ..E
    },
    Section {
        chords: "Gm Bb A A", kick: "x...x...x...x...", hat: "x.x.x.x.x.x.x.x.", bass: "0.00.00.0.00.03.",
        stab: "x.....x.........", tom: "x.x.x.x.x.x.xxxx", low: "0.00.00.0.00.00.", energy: 0.95, fill: true,
        lead: "G5:8 D5:8 A#5:16 A5:8 E5:8 C#5:8 A4:8", ..E
    },
];

// NO SLEEP. Restless ostinatos at speed, with a choir breaking through.
static COMBAT_B: &[Section] = &[
    Section {
        chords: "Fm Db Eb Cm", arp: "0120120120120120", low: "0.0..0..0.0..3..", tom: "x...............",
        energy: 0.55, swell: true, ..E
    },
    Section {
        chords: "Fm Db Eb Cm",
        kick: "x.....x...x.....", snare: "....x.......x...", hat: "x.x.x.x.x.x.x.x.",
        bass: "0.0..0..0.0..3..", arp: "0120120120120120", low: "0.0.0.0.0.0.0.0.", tom: "x...............",
        lead: "-:16 -:16 -:16 -:16", energy: 0.7, repeats: 2, ..E
    },
    Section {
        chords: "Fm Db Eb Cm",
        kick: "x.....x...x.....", snare: "....x.......x...", hat: "xxx.xxx.xxx.xxx.",
        bass: "0.0..0..0.0..3..", arp: "0120120120120120", low: "0.0.0.0.0.0.0.0.",
        counter: "C5:16 C#5:16 D#5:16 D#5:16", energy: 0.85,
        lead: "C6:6 G#5:2 F5:8 C#6:6 A#5:2 F5:8 D#6:4 C#6:4 C6:4 G#5:4 G5:6 D#5:2 C5:8",
        repeats: 2, ..E
    },
    Section {
        chords: "Db Ab Eb Fm", choir: "x", arp: "0120120120120120", hat: "....x.......x...",
        brass: "F4:16 D#4:16 G4:16 F4:16", energy: 0.7, swell: true, ..E
    },
    Section {
        chords: "Db Ab Eb Fm",
        kick: "x.x...x...x.x...", snare: "....x.......x.xx", hat: "xxxxxxxxxxxxxxxx", ride: "x...x...x...x...",
        bass: "00.0.3.00.0.5.0.", arp: "0123210123210123", stab: "x.....x.....x...", acid: "0.0.3.0.1.0.5.0.",
        sub: "x...............", tom: "x.......x.......", counter: "G#5:16 G#5:16 G5:16 G#5:16",
        brass: "C#4:16 C4:16 D#4:16 C4:16", energy: 1.0, fill: true,
        lead: "F6:2 D#6:2 C#6:4 C6:2 C#6:2 G#5:4 D#6:2 C6:2 G#5:4 G5:2 G#5:2 D#5:4 A#5:2 C6:2 C#6:4 D#6:2 F6:2 G#6:4 F6:12 -:4",
        repeats: 2, ..E
    },
    Section {
        chords: "Fm Db Eb Cm",
        kick: "x.....x...x.....", snare: "....x.......x...", hat: "xxx.xxx.xxx.xxx.", choir: "x",
        bass: "0.0..0..0.0..3..", arp: "0120120120120120", stab: "x.....x.....x...", low: "0.0.0.0.0.0.0.0.",
        energy: 0.95, fill: true,
        lead: "C6:6 G#5:2 F5:8 C#6:6 A#5:2 F5:8 D#6:4 C#6:4 C6:4 G#5:4 G5:6 D#5:2 C5:8",
        ..E
    },
];

// RUST CHOIR. An industrial march, with voices rising out of the machinery.
static COMBAT_C: &[Section] = &[
    Section {
        chords: "Cm Cm Ab G", choir: "x", tom: "x.......x.......", energy: 0.55, ..E
    },
    Section {
        chords: "Cm Cm Ab G",
        kick: "x..x..x.x.......", snare: "....x.......x...", hat: "x...x...x...x...",
        bass: "0..0..0.0...0.3.", stab: "x..x..x.........", choir: "x", low: "0..0..0.0...0.3.",
        lead: "-:16 -:16 -:16 -:16", energy: 0.7, repeats: 2, ..E
    },
    Section {
        chords: "Cm Ab Fm G",
        kick: "x..x..x.x..x..x.", snare: "....x.......x..x", hat: "x.x.x.x.x.x.x.x.",
        bass: "0..0..0.0.03.05.", arp: "0.2.1.2.0.2.1.2.", stab: "x..x..x.........", choir: "x",
        low: "0..0..0.0.03.05.", brass: "G4:16 G#4:16 F4:16 G4:16", energy: 0.85,
        lead: "C5:8 D#5:4 F5:4 G5:12 D#5:4 G#5:8 G5:4 F5:4 D5:12 B4:4",
        repeats: 2, ..E
    },
    Section {
        chords: "Fm Cm Ab G", choir: "x", tom: "x...x...x...x...", counter: "C5:16 C5:16 C5:16 B4:16",
        energy: 0.7, swell: true, ..E
    },
    Section {
        chords: "Ab Eb Fm G",
        kick: "x.x.x..xx.x.x..x", snare: "....x.......x.xx", hat: "xxx.xxx.xxx.xxx.", ride: "x...x...x...x...",
        bass: "00.0.0.300.0.0.5", arp: "0123012301230123", stab: "x.x...x.x.x...x.", choir: "x",
        acid: "0.1.0.3.0.5.1.0.", sub: "x.......x.......", tom: "x.......x.......",
        counter: "D#5:16 D#5:8 G5:8 G#5:16 G5:16", energy: 1.0, fill: true,
        lead: "C6:4 A#5:4 G#5:4 G5:4 G5:4 F5:4 D#5:8 F5:4 G5:4 G#5:4 A#5:4 B5:8 G5:8",
        repeats: 2, ..E
    },
];

// COLOSSUS. Everything, all at once, as fast as it will go.
static BOSS: &[Section] = &[
    Section {
        chords: "Em F Em C", tom: "x.x.x.x.x.x.x.x.", choir: "x", sub: "x...............",
        low: "0.00.00.0.00.01.", energy: 0.7, swell: true, ..E
    },
    Section {
        chords: "Em F Em C",
        kick: "x.x.x.x.x.x.x.x.", snare: "....x.......x...", hat: "xxxxxxxxxxxxxxxx",
        bass: "0.00.00.0.00.01.", arp: "0120120120120120", stab: "x..x..x.........", choir: "x",
        low: "0.00.00.0.00.01.", brass: "E4:16 F4:16 E4:16 E4:16", tom: "x...x...x...x...", energy: 0.85,
        lead: "E5:4 F5:4 G5:4 F5:4 E5:12 B4:4 C5:4 D5:4 E5:4 F5:4 G5:8 E5:8",
        repeats: 2, ..E
    },
    Section {
        chords: "Am F Em Em",
        kick: "x.xxx.x.x.xxx.x.", snare: "....x..x....x.xx", hat: "xxxxxxxxxxxxxxxx", ride: "x...x...x...x...",
        bass: "00.00.0.00.00.15", arp: "0123432101234321", stab: "x.x.x.x.x.x.xxxx", choir: "x",
        counter: "A5:16 A5:16 B5:16 B5:16", tom: "x...x...x...x...", sub: "x...............", energy: 0.95,
        lead: "A5:2 B5:2 C6:4 B5:2 A5:2 F5:4 A5:2 C6:2 F6:4 E6:8 G5:2 B5:2 E6:4 D#6:4 E6:4 B5:2 G5:2 E5:8 -:4",
        repeats: 2, ..E
    },
    Section {
        chords: "Am F C E", choir: "x", hat: "....x.......x...", low: "0.0.0.0.0.0.0.0.",
        brass: "A4:16 A4:16 G4:16 G#4:16", energy: 0.75, swell: true, ..E
    },
    Section {
        chords: "C F Dm E",
        kick: "x.x.x.x.x.x.x.x.", snare: "x...x...x...xxxx", hat: "xxxxxxxxxxxxxxxx", ride: "x...x...x...x...",
        bass: "01010101010101.5", arp: "0123012301230123", stab: "x..x..x.x..x..x.", choir: "x",
        acid: "0.1.0.3.0.1.0.5.", sub: "x.......x.......", tom: "x.x.x.x.x.x.x.x.",
        counter: "E6:16 F6:16 F6:16 E6:16", brass: "G4:16 A4:16 A4:16 G#4:16", energy: 1.0, fill: true,
        lead: "G5:4 C6:4 E6:8 F6:4 E6:4 C6:8 D6:4 C6:4 A5:8 G#5:4 B5:4 E6:8",
        repeats: 2, ..E
    },
];

// OVERRUN. A last hymn for the reactor.
static GAMEOVER: &[Section] = &[
    Section {
        chords: "Dm Bb Gm A", bass: "0...............", choir: "x", counter: "D4:16 D4:16 D4:16 C#4:16", energy: 0.6,
        lead: "A4:16 F5:12 D5:4 G5:8 F5:8 E5:12 C#5:4", repeats: 2, ..E
    },
    Section {
        chords: "Gm Dm Bb A", bass: "0...............", choir: "x", tom: "x...............",
        brass: "G4:16 F4:16 F4:16 E4:16", energy: 0.8,
        lead: "A#4:16 F5:16 D5:16 E5:16", ..E
    },
];

// Death tracks. Each has about eight seconds to land before the player is back in the fight.
static ROT: &[Section] = &[
    Section {
        chords: "Dm Gm Bb A",
        kick: "x..x............", bass: "0...............", choir: "x", tom: "x..x............", energy: 0.65,
        lead: "A4:8 F5:8 D5:8 A4:8 G4:8 A#4:8 D5:16 F4:8 A#4:8 D5:8 F5:8 E5:8 C#5:8 A4:16",
        repeats: 2, ..E
    },
    Section {
        chords: "Bb Gm A Dm", bass: "0...............", choir: "x", brass: "D4:16 D4:16 C#4:16 D4:16",
        tom: "x...............", energy: 0.8, lead: "F4:16 G4:16 A4:16 A4:16", ..E
    },
];

static ONE_OF_MANY: &[Section] = &[
    Section {
        chords: "Cm Db Cm G",
        kick: "x.......x.x.....", snare: "............x...", bass: "0..0....0..0....",
        stab: "x...............", choir: "x", low: "0..0....0..0....", energy: 0.75,
        lead: "G4:8 C5:8 C#5:16 C5:8 G4:8 D#5:16 D5:8 B4:8 G4:16 G4:8 D#5:8 F5:16 D#5:8 C5:8 G4:8 C5:8 F4:8 D5:8 B4:8 G4:8",
        repeats: 2, ..E
    },
    Section {
        chords: "Cm Db Cm G",
        kick: "x...x...x...x...", snare: "....x.......x...", bass: "0..0..0.0..0..0.",
        stab: "x..x............", choir: "x", sub: "x...............", tom: "x...x...x...x...",
        low: "0.0.0.0.0.0.0.0.", brass: "G4:16 G#4:16 G4:16 G4:16", energy: 1.0, ..E
    },
];

static SMITHERINES: &[Section] = &[
    Section {
        chords: "Em Em C B",
        kick: "x.x.x.xxx.x.x.xx", snare: "....x.......x.xx", hat: "xxxxxxxxxxxxxxxx",
        bass: "0.00.00.0.00.01.", arp: "0120120120120120", stab: "x..x..x.x..x..x.",
        counter: "G5:16 E5:16 G5:16 F#5:16", tom: "x...x...x...x...", energy: 0.95,
        lead: "B5:4 E6:4 G6:8 F#6:4 E6:4 B5:8 G5:4 B5:4 E6:8 B5:8 G5:8 C6:4 E6:4 G6:8 E6:8 D#6:4 B5:4 F#5:8 B5:16 -:8",
        repeats: 2, ..E
    },
    Section {
        chords: "C B Em Em",
        kick: "x.x.x.x.x.x.x.x.", snare: "....x.......xxxx", hat: "xxxxxxxxxxxxxxxx", ride: "x...x...x...x...",
        bass: "0.00.00.0.00.01.", acid: "0.1.0.3.0.5.1.0.", sub: "x...............", tom: "x.x.x.x.x.x.x.x.",
        brass: "E4:16 D#4:16 E4:16 B4:16", energy: 1.0, fill: true, ..E
    },
];

// REST WELL, FOR IT IS OVER. Nothing left to fight: only pads and a slow bell.
static REST_WELL: &[Section] = &[
    Section {
        chords: "F C Dm Bb", bass: "0...............", counter: "F4:16 E4:16 F4:16 F4:16", energy: 0.45,
        lead: "A4:8 C5:8 G4:8 C5:8 F4:8 A4:8 F4:8 D5:8", ..E
    },
    Section {
        chords: "F C Bb F", bass: "0.......5.......",
        arp: "0...1...2...1...", choir: "x", counter: "A4:16 G4:16 F4:16 A4:16", energy: 0.55,
        lead: "C5:8 F5:8 E5:8 C5:8 D5:8 A#4:8 A4:16", ..E
    },
    Section {
        chords: "Bb F C F", bass: "0...............", choir: "x", energy: 0.4,
        lead: "D5:16 C5:16 C5:16 A4:16", ..E
    },
];

pub fn song(t: Track) -> Song {
    #[allow(clippy::too_many_arguments)]
    let mk = |bpm, swing, pad_cut, bass_drive, lead_inst, arp_inst, loop_to, sections| Song {
        bpm, swing, pad_cut, bass_drive, lead_inst, arp_inst, loop_to, sections,
    };
    match t {
        Track::Menu => mk(66.0, 0.0, 900.0, 1.0, Inst::Bell, Inst::Pluck, 1, MENU),
        Track::Build => mk(96.0, 0.12, 1500.0, 1.2, Inst::Pluck, Inst::Pluck, 1, BUILD),
        Track::CombatA => mk(128.0, 0.0, 2200.0, 2.5, Inst::Lead, Inst::Arp, 1, COMBAT_A),
        Track::CombatB => mk(140.0, 0.0, 2600.0, 2.0, Inst::Lead, Inst::Arp, 1, COMBAT_B),
        Track::CombatC => mk(116.0, 0.06, 1700.0, 3.5, Inst::Lead, Inst::Arp, 1, COMBAT_C),
        Track::Boss => mk(158.0, 0.0, 3200.0, 4.5, Inst::Lead, Inst::Arp, 1, BOSS),
        Track::GameOver => mk(58.0, 0.0, 700.0, 1.0, Inst::Bell, Inst::Pluck, 0, GAMEOVER),
        Track::Rot => mk(60.0, 0.0, 700.0, 2.0, Inst::Bell, Inst::Pluck, 0, ROT),
        Track::OneOfMany => mk(72.0, 0.0, 800.0, 3.0, Inst::Bell, Inst::Pluck, 0, ONE_OF_MANY),
        Track::Smitherines => mk(146.0, 0.0, 2600.0, 5.0, Inst::Lead, Inst::Arp, 0, SMITHERINES),
        Track::RestWell => mk(50.0, 0.0, 1100.0, 1.0, Inst::Bell, Inst::Pluck, 0, REST_WELL),
    }
}

/// Instruments, all unashamedly synthesizers. Each maps to a mix layer via `bus_of()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inst {
    /// Slow detuned saws under everything.
    Pad,
    /// A string machine: bright saws through the ensemble.
    SynthStrings,
    /// The same idea an octave or two down, for long low lines.
    LowSaw,
    /// A short resonant pulse, for sequenced patterns.
    Arp,
    /// Saws with a filter that snaps open: classic synth brass.
    Brass,
    Lead,
    /// A wide saw chord, slammed shut by its filter.
    Stab,
    /// Saws through two formants: a vocal patch, not a choir.
    Choir,
    Pluck,
    Bell,
    /// A tuned electronic drum, playing the orchestral timpani parts.
    SynTom,
    /// A long pitched sub hit.
    Sub,
    Bass,
    /// A resonant squelch for the highest-energy passages.
    Acid,
    Kick,
    Snare,
    Hat,
    OpenHat,
    Ride,
    Crash,
    /// Filtered noise sweeping upward, into the next section.
    Riser,
    Tom,
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
        Inst::Pad | Inst::Choir | Inst::LowSaw => BUS_PAD,
        Inst::Bass | Inst::Acid => BUS_BASS,
        Inst::Pluck | Inst::Arp => BUS_ARP,
        Inst::Lead | Inst::SynthStrings | Inst::Brass | Inst::Bell => BUS_LEAD,
        Inst::Stab => BUS_STAB,
        Inst::Kick | Inst::Sub => BUS_KICK,
        Inst::SynTom | Inst::Snare | Inst::Hat | Inst::OpenHat | Inst::Ride | Inst::Crash | Inst::Riser | Inst::Tom => BUS_PERC,
    }
}

/// These go through the ensemble effect, the chorus that string machines are known for.
fn is_section(i: Inst) -> bool {
    matches!(i, Inst::Pad | Inst::SynthStrings | Inst::LowSaw | Inst::Choir | Inst::Brass)
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

/// One fixed LFO for pitch, the same for every note, as on the hardware.
#[inline]
fn vibrato(t: f32, depth: f32) -> f32 {
    1.0 + depth * (t * 5.6 * std::f32::consts::TAU).sin() * (t * 3.0).min(1.0)
}

impl Voice {
    #[allow(clippy::too_many_arguments)]
    fn new(inst: Inst, freq: f32, vel: f32, gate: f32, pan: f32, seed: u32, drive: f32, cut: f32) -> Self {
        let mut n = Noise(seed | 1);
        let mut phase = [0.0; 6];
        for p in &mut phase {
            *p = n.next().abs();
        }
        // Drums and FM start every oscillator together, for a clean attack.
        if matches!(inst, Inst::SynTom | Inst::Sub | Inst::Kick | Inst::Bell | Inst::Tom) {
            phase = [0.0; 6];
        }
        Voice { inst, freq, vel, t: 0.0, gate, phase, filt: Svf::default(), filt2: Svf::default(), noise: n, pan, drive, cut }
    }

    fn duration(&self) -> f32 {
        match self.inst {
            Inst::Pad => self.gate + 1.6,
            Inst::SynthStrings => self.gate + 0.4,
            Inst::LowSaw => self.gate + 0.4,
            Inst::Brass => self.gate + 0.2,
            Inst::Choir => self.gate + 1.4,
            Inst::Bass | Inst::Acid => self.gate + 0.06,
            Inst::Arp => 0.42,
            Inst::Lead => self.gate + 0.14,
            Inst::Pluck => 0.9,
            Inst::Bell => 2.6,
            Inst::Stab => 0.5,
            Inst::SynTom => 0.9,
            Inst::Sub => 1.6,
            Inst::Kick => 0.34,
            Inst::Tom => 0.45,
            Inst::Snare => 0.28,
            Inst::Hat => 0.07,
            Inst::OpenHat => 0.32,
            Inst::Ride => 0.7,
            Inst::Crash => 2.2,
            Inst::Riser => self.gate + 0.05,
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

    /// Three detuned saws.
    #[inline]
    fn saws(&mut self, f: f32, spread: f32) -> f32 {
        let d0 = self.adv(0, f * (1.0 - spread));
        let d1 = self.adv(1, f);
        let d2 = self.adv(2, f * (1.0 + spread * 1.1));
        (saw(self.phase[0], d0) + saw(self.phase[1], d1) + saw(self.phase[2], d2)) * 0.33
    }

    /// Six clashing square waves, the classic drum machine cymbal.
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
        let out = match self.inst {
            Inst::Pad => {
                let s = self.saws(f, 0.0055);
                // Louder passages open the filter as well as raising the level.
                let cut = self.cut * (0.8 + 0.25 * lfo) * (0.6 + self.vel * 2.5);
                self.filt.process(s, cut, 0.9).lp * adsr(t, self.gate, 0.5, 0.4, 0.85, 1.5)
            }
            Inst::SynthStrings => {
                let fr = f * vibrato(t, 0.003);
                let s = self.saws(fr, 0.005);
                self.filt.process(s, (fr * 5.0).clamp(1200.0, 6000.0), 0.8).lp * adsr(t, self.gate, 0.04, 0.1, 0.9, 0.35)
            }
            Inst::LowSaw => {
                let s = self.saws(f, 0.003);
                self.filt.process(s, (f * 4.0).clamp(300.0, 1800.0), 1.0).lp * adsr(t, self.gate, 0.05, 0.2, 0.9, 0.35)
            }
            Inst::Arp => {
                let dt = self.adv(0, f);
                let s = pulse(self.phase[0], dt, 0.28);
                let cut = 500.0 + 3500.0 * (-t * 16.0).exp();
                self.filt.process(s, cut, 2.2).lp * (-t * 8.0).exp()
            }
            Inst::Brass => {
                let fr = f * vibrato(t, 0.002);
                let s = self.saws(fr, 0.003);
                let dt = self.adv(3, fr * 0.5);
                let oct = pulse(self.phase[3], dt, 0.5) * 0.3;
                // Opens hard on the attack, then settles: the sound of every 80s brass patch.
                let cut = fr * (1.6 + 5.0 * (t / 0.05).min(1.0) * (-t * 5.0).exp());
                soft_clip(self.filt.process(s + oct, cut.clamp(200.0, 7000.0), 1.4).lp * 1.5) * adsr(t, self.gate, 0.012, 0.1, 0.85, 0.18)
            }
            Inst::Lead => {
                let fr = f * vibrato(t, 0.006);
                let dt0 = self.adv(0, fr);
                let dt1 = self.adv(1, fr * 1.007);
                let dt2 = self.adv(2, fr * 0.5);
                let s = saw(self.phase[0], dt0) + saw(self.phase[1], dt1) * 0.8 + pulse(self.phase[2], dt2, 0.5) * 0.3;
                let cut = 1200.0 + 3000.0 * (-t * 5.0).exp();
                soft_clip(self.filt.process(s * 0.5, cut, 1.5).lp * 1.5) * adsr(t, self.gate, 0.008, 0.1, 0.75, 0.12)
            }
            Inst::Stab => {
                let s = self.saws(f, 0.009);
                let cut = 400.0 + 4200.0 * (-t * 11.0).exp();
                soft_clip(self.filt.process(s * 1.5, cut, 1.3).lp * 1.4) * (-t * 7.0).exp()
            }
            Inst::Choir => {
                let fr = f * vibrato(t, 0.004);
                let dt0 = self.adv(0, fr);
                let dt1 = self.adv(1, fr * 1.004);
                let src = saw(self.phase[0], dt0) + saw(self.phase[1], dt1);
                let a = self.filt.process(src, 620.0, 7.0).bp;
                let b = self.filt2.process(src, 1150.0, 8.0).bp;
                (a + b * 0.6) * adsr(t, self.gate, 0.7, 0.5, 0.8, 1.3) * 0.33
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
                let s = sine(self.phase[0]) + 0.45 * sine(self.phase[1]) * (-t * 3.5).exp() + 0.2 * sine(self.phase[2]) * (-t * 7.0).exp();
                s * (-t * 1.9).exp() * (t / 0.004).min(1.0) * 0.8
            }
            Inst::SynTom => {
                self.adv(0, f * (1.0 + 0.7 * (-t * 25.0).exp()));
                soft_clip(sine(self.phase[0]) * 1.6) * (-t * 5.5).exp()
            }
            Inst::Sub => {
                self.adv(0, 45.0 + 60.0 * (-t * 20.0).exp());
                soft_clip(sine(self.phase[0]) * 1.8) * (-t * 2.4).exp()
            }
            Inst::Bass => {
                let dt0 = self.adv(0, f);
                let dt1 = self.adv(1, f * 0.5);
                let s = saw(self.phase[0], dt0) * 0.7 + pulse(self.phase[1], dt1, 0.5) * 0.6;
                let cut = 180.0 + 1700.0 * (-t * 14.0).exp();
                soft_clip(self.filt.process(s, cut, 1.6).lp * self.drive) * adsr(t, self.gate, 0.003, 0.08, 0.8, 0.05)
            }
            Inst::Acid => {
                let dt = self.adv(0, f);
                let s = saw(self.phase[0], dt);
                let cut = 260.0 + 3400.0 * (-t * 11.0).exp();
                soft_clip(self.filt.process(s, cut, 7.0).lp * 1.6) * adsr(t, self.gate, 0.002, 0.08, 0.7, 0.04)
            }
            Inst::Kick => {
                let fr = 48.0 + 180.0 * (-t * 40.0).exp();
                self.adv(0, fr);
                let click = self.noise.next() * (-t * 450.0).exp() * 0.3;
                soft_clip(sine(self.phase[0]) * 2.0) * (-t * 10.0).exp() + click
            }
            Inst::Tom => {
                self.adv(0, f * (1.0 + 0.6 * (-t * 24.0).exp()));
                soft_clip(sine(self.phase[0]) * 1.4) * (-t * 9.0).exp()
            }
            Inst::Snare => {
                // The noise and the tone of a drum machine, with its clap on top.
                self.adv(0, 190.0 * (1.0 + 0.5 * (-t * 60.0).exp()));
                let n = self.noise.next();
                let wires = self.filt.process(n, 1800.0, 0.8).hp * (-t * 15.0).exp() * 0.8;
                let clap_env = if t < 0.03 { if (t * 110.0) as u32 % 2 == 0 { 1.0 } else { 0.25 } } else { (-(t - 0.03) * 26.0).exp() };
                let clap = self.filt2.process(n, 1500.0, 1.6).bp * clap_env;
                (wires + clap + sine(self.phase[0]) * (-t * 22.0).exp() * 0.5) * 1.05
            }
            Inst::Hat => {
                let m = self.metal(410.0);
                self.filt.process(m, 8500.0, 1.0).hp * (-t * 68.0).exp() * 0.8
            }
            Inst::OpenHat => {
                let m = self.metal(410.0);
                self.filt.process(m, 7500.0, 1.0).hp * (-t * 11.0).exp() * 0.55
            }
            Inst::Ride => {
                let m = self.metal(520.0);
                self.filt.process(m, 5200.0, 1.0).hp * (-t * 6.0).exp() * 0.4
            }
            Inst::Crash => {
                let n = self.noise.next();
                self.filt.process(n, 4500.0, 0.7).hp * (-t * 2.2).exp() * 0.5
            }
            Inst::Riser => {
                let n = self.noise.next();
                let k = (t / self.gate.max(0.1)).min(1.0);
                let sweep = self.filt.process(n, 300.0 * 30f32.powf(k), 2.0).bp;
                sweep * k * ((self.gate + 0.05 - t) / 0.05).clamp(0.0, 1.0) * 0.8
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

/// One melodic line: its notes, and how long before it repeats.
struct Line {
    events: Vec<LeadEvent>,
    len: u32,
}

fn parse_line(text: &str) -> Line {
    let mut events = Vec::new();
    let mut step = 0;
    for tok in text.split_whitespace() {
        let mut it = tok.split(':');
        let (n, l) = (it.next().unwrap_or("-"), it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(4));
        if let Some(note) = parse_note(n) {
            events.push(LeadEvent { step, len: l, note });
        }
        step += l;
    }
    Line { events, len: step }
}

struct ParsedSection {
    chords: Vec<(f32, [f32; 4])>,
    lead: Line,
    counter: Line,
    brass: Line,
    lead_len: u32,
}

fn parse_section(s: &Section) -> ParsedSection {
    let chords: Vec<_> = s.chords.split_whitespace().map(parse_chord).collect();
    let (lead, counter, brass) = (parse_line(s.lead), parse_line(s.counter), parse_line(s.brass));
    let lead_len = lead.len.max(16);
    ParsedSection { chords, lead, counter, brass, lead_len }
}

/// Two gently wandering copies, one per side: the chorus of a string machine.
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
        self.phase += 0.37 / SR;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let tri = |p: f32| (p * std::f32::consts::TAU).sin();
        let (a, b) = (tri(self.phase), tri(self.phase + 0.33));
        let (c, d) = (tri(self.phase + 0.5), tri(self.phase + 0.83));
        let l = self.read(620.0 + a * 170.0) + self.read(1010.0 + b * 210.0);
        let r = self.read(760.0 + c * 170.0) + self.read(1190.0 + d * 210.0);
        (x * 0.45 + l * 0.42, x * 0.45 + r * 0.42)
    }
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
    ensemble: Ensemble,
}

const MAX_VOICES: usize = 90;

impl Player {
    pub fn new(track: Track, seed: u32) -> Self {
        let song = song(track);
        let parsed = song.sections.iter().map(parse_section).collect();
        let step_samples = 60.0 / song.bpm / 4.0 * SR;
        Player {
            song,
            parsed,
            voices: Vec::with_capacity(MAX_VOICES),
            step_samples,
            until_step: 0.0,
            step: 0,
            section: 0,
            section_step: 0,
            repeat: 0,
            seed: seed | 1,
            lfo: 0.0,
            duck: 0.0,
            ensemble: Ensemble::new(),
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

    /// A small human unevenness in how hard each note is played.
    fn humanize(&mut self, vel: f32) -> f32 {
        vel * (0.93 + (self.rnd() % 1000) as f32 / 1000.0 * 0.14)
    }

    fn spawn(&mut self, inst: Inst, note: f32, vel: f32, gate_steps: f32, pan: f32) {
        if self.voices.len() >= MAX_VOICES {
            // Steal something short and quiet before anything people will notice going missing.
            if let Some(i) = self.voices.iter().position(|v| matches!(v.inst, Inst::Hat | Inst::Arp | Inst::OpenHat | Inst::Ride)) {
                self.voices.swap_remove(i);
            } else {
                return;
            }
        }
        let gate = gate_steps * self.step_samples / SR;
        let seed = self.rnd();
        let vel = self.humanize(vel);
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
        let lead_len = self.parsed[self.section].lead_len;
        let section_len = (16 * bars as u32).max(lead_len.div_ceil(16) * 16);
        let last_bar = self.section_step + 16 >= section_len;
        let e = sec.energy;
        // Louder passages are played harder, not just mixed higher.
        let orch = 0.55 + 0.45 * e;
        let degree = |c: u8, tones: &[f32; 4]| match c {
            b'1' => 12.0,
            b'3' => tones[1],
            b'5' => 7.0,
            _ => 0.0,
        };

        // Pads and vocals, one chord per bar, spread across the stereo field.
        if bar_step == 0 {
            let base = 48.0 + root;
            let oct = if base > 53.0 { -12.0 } else { 0.0 };
            let middle = base + oct + 12.0;
            for (i, t) in tones.iter().enumerate().take(3) {
                self.spawn(Inst::Pad, middle + t, 0.2 * orch, 16.0, [-0.5, 0.0, 0.5][i]);
            }
            self.spawn(Inst::Pad, base + oct, 0.18 * orch, 16.0, 0.25);
            if e >= 0.8 {
                // Everything in: the pad opens up an octave above.
                self.spawn(Inst::Pad, middle + 12.0 + tones[0], 0.11 * orch, 16.0, -0.6);
                self.spawn(Inst::Pad, middle + 12.0 + tones[2], 0.1 * orch, 16.0, -0.4);
            }
            if !sec.choir.is_empty() {
                self.spawn(Inst::Choir, base + oct + tones[0], 0.36 * orch, 16.0, -0.3);
                self.spawn(Inst::Choir, base + oct + tones[2], 0.3 * orch, 16.0, 0.3);
                self.spawn(Inst::Choir, base + oct + 12.0 + tones[1], 0.22 * orch, 16.0, 0.0);
            }
        }
        // Drums. The last bar of a section with a fill hands over to the toms and snare.
        let fill_now = sec.fill && last_bar && bar_step >= 8;
        if fill_now {
            let k = (bar_step - 8) as f32 / 8.0;
            if bar_step % 2 == 0 {
                let tom = [50.0, 47.0, 43.0, 40.0][(bar_step - 8) / 2];
                self.spawn(Inst::Tom, tom, 0.6 + k * 0.3, 1.0, [0.4, 0.15, -0.15, -0.4][(bar_step - 8) / 2]);
            }
            self.spawn(Inst::Snare, 40.0, 0.35 + k * 0.5, 1.0, 0.05);
        }
        if on(sec.kick) == b'x' {
            self.spawn(Inst::Kick, 40.0, 1.0, 1.0, 0.0);
            self.duck = 1.0;
        }
        if on(sec.sub) == b'x' {
            self.spawn(Inst::Sub, 30.0, 0.85, 1.0, 0.0);
            self.duck = 1.0;
        }
        if on(sec.snare) == b'x' && !fill_now {
            self.spawn(Inst::Snare, 40.0, 0.8, 1.0, 0.05);
        }
        if on(sec.hat) == b'x' {
            let accent = if bar_step % 4 == 2 { 0.5 } else { 0.28 };
            let open = bar_step == 14 && self.section_step % 64 >= 48;
            self.spawn(if open { Inst::OpenHat } else { Inst::Hat }, 40.0, accent, 1.0, -0.2);
        }
        if on(sec.ride) == b'x' {
            self.spawn(Inst::Ride, 40.0, 0.3, 1.0, 0.3);
        }
        let c = on(sec.tom);
        if c == b'x' {
            // The electronic toms follow the chord.
            let mut p = 36.0 + root.rem_euclid(12.0);
            if p < 41.0 {
                p += 12.0;
            }
            let strong = if bar_step == 0 { 1.0 } else { 0.72 };
            self.spawn(Inst::SynTom, p, 0.62 * orch * strong, 1.0, 0.2);
        }
        // A new section is announced with a crash, once there is a kit playing.
        if self.section_step == 0 && !sec.kick.is_empty() {
            self.spawn(Inst::Crash, 40.0, 0.5, 1.0, 0.25);
        }
        // Rising into the next section.
        if sec.swell && last_bar && bar_step == 0 {
            self.spawn(Inst::Riser, 40.0, 0.5, 16.0, -0.2);
            let top = 72.0 + root.rem_euclid(12.0) + tones[0];
            self.spawn(Inst::SynthStrings, top, 0.22, 16.0, -0.3);
        }
        // Bass: digits are chord degrees (0 root, 3 third, 5 fifth, 1 octave up).
        let c = on(sec.bass);
        if c != b'.' {
            let bass_root = 24.0 + (root + 12.0) % 12.0 + 12.0;
            let len = if sec.kick.is_empty() { 14.0 } else { 0.9 };
            self.spawn(Inst::Bass, bass_root + degree(c, &tones), 0.6, len, 0.0);
        }
        let c = on(sec.acid);
        if c != b'.' {
            let acid_root = 36.0 + root.rem_euclid(12.0);
            self.spawn(Inst::Acid, acid_root + degree(c, &tones), 0.24, 1.6, 0.25);
        }
        // The driving low sequence, doubled an octave down when the arrangement digs in.
        let c = on(sec.low);
        if c != b'.' {
            let mut p = 48.0 + root.rem_euclid(12.0);
            if p > 55.0 {
                p -= 12.0;
            }
            let note = p + degree(c, &tones);
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
            let (inst, vel) = if self.song.arp_inst == Inst::Pluck { (Inst::Pluck, 0.38) } else { (Inst::Arp, 0.32 * orch) };
            self.spawn(inst, 60.0 + root.rem_euclid(12.0) + t, vel, 1.0, pan);
        }
        // Stabs: the full chord.
        if on(sec.stab) == b'x' {
            let base = 60.0 + root.rem_euclid(12.0);
            for (i, t) in tones.iter().enumerate().take(3) {
                self.spawn(Inst::Stab, base + t, 0.2, 1.0, [-0.35, 0.0, 0.35][i]);
            }
            self.spawn(Inst::Stab, base - 12.0, 0.18, 1.0, 0.0);
        }
        // The melody, with the string machine an octave up at the peaks.
        let inst = self.song.lead_inst;
        let mut hits: [(f32, f32, Inst, f32, f32); 3] = [(0.0, 0.0, Inst::Lead, 0.0, 0.0); 3];
        let mut n = 0;
        {
            let p = &self.parsed[self.section];
            let mut take = |line: &Line, inst: Inst, vel: f32, shift: f32, pan: f32| {
                if line.len == 0 || n >= hits.len() {
                    return;
                }
                let pos = self.section_step % line.len;
                if let Some(ev) = line.events.iter().find(|ev| ev.step == pos) {
                    hits[n] = (ev.note + shift, ev.len as f32, inst, vel, pan);
                    n += 1;
                }
            };
            let lead_vel = match inst {
                Inst::Bell => 0.36,
                Inst::Pluck => 0.34,
                _ => 0.25,
            };
            take(&p.lead, inst, lead_vel * orch, 0.0, 0.08);
            take(&p.counter, Inst::SynthStrings, 0.2 * orch, 0.0, -0.35);
            take(&p.brass, Inst::Brass, 0.34 * orch, 0.0, 0.3);
        }
        for &(note, len, inst, vel, pan) in hits.iter().take(n) {
            self.spawn(inst, note, vel, len - 0.2, pan);
            if inst == Inst::Lead && e >= 0.85 {
                // The octave doubling is what gives the tune its size.
                self.spawn(Inst::SynthStrings, note + 12.0, vel * 0.5, len - 0.2, -0.2);
            }
            if inst == Inst::Brass && e >= 0.9 {
                self.spawn(Inst::LowSaw, note - 12.0, vel * 0.7, len - 0.2, 0.15);
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

    /// Mix one stereo frame. Returns left, right, and the send to the reverb.
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
        let side = 1.0 - 0.5 * self.duck * self.duck;
        let (mut l, mut r, mut send, mut section) = (0.0, 0.0, 0.0, 0.0);
        let mut i = 0;
        while i < self.voices.len() {
            let v = &mut self.voices[i];
            let s = v.sample(lfo);
            let inst = v.inst;
            let bus = bus_of(inst);
            let mut g = gains[bus];
            if bus != BUS_KICK && bus != BUS_PERC {
                g *= side;
            }
            let x = s * g;
            if is_section(inst) {
                section += x;
            }
            let (pl, pr) = (0.5 - v.pan * 0.5, 0.5 + v.pan * 0.5);
            let dry = if is_section(inst) { 0.55 } else { 1.0 };
            l += x * pl * 1.4 * dry;
            r += x * pr * 1.4 * dry;
            // Long tails on the pads and bells; the drums and bass stay dry and close.
            send += x * match inst {
                Inst::Pad | Inst::Choir => 0.75,
                Inst::Bell | Inst::Pluck => 0.6,
                Inst::Stab => 0.5,
                Inst::SynthStrings | Inst::Brass | Inst::Lead => 0.45,
                Inst::LowSaw | Inst::Riser => 0.4,
                Inst::Arp => 0.3,
                Inst::SynTom | Inst::Tom | Inst::Crash | Inst::Snare => 0.22,
                Inst::Acid => 0.15,
                _ => 0.0,
            };
            if v.t > v.duration() {
                self.voices.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let (el, er) = self.ensemble.process(section);
        (l + el * 0.8, r + er * 0.8, send)
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
        Track::Menu | Track::GameOver | Track::RestWell => [0.95, 0.5, 0.6, 0.8, 0.5, 0.7, 0.6],
        Track::Build => [0.85, 0.55, 0.7, 0.75, 0.55, 0.5, 0.5],
        Track::Rot => [0.9, 0.7, 0.5, 0.85, 0.8, 0.75, 0.6],
        Track::OneOfMany => [1.0, 0.8, 0.5, 0.8, 0.85, 0.7, 0.7],
        Track::Smitherines => [0.65, 0.85, 0.6, 0.8, 0.95, 0.75, 0.8],
        _ => [
            0.75,
            0.55 + 0.4 * ss(0.0, 0.25, i),
            0.6 + 0.3 * ss(0.1, 0.45, i),
            0.3 + 0.65 * ss(0.3, 0.7, i),
            0.65 + 0.35 * ss(0.0, 0.2, i),
            0.45 + 0.45 * ss(0.15, 0.5, i),
            0.25 + 0.7 * ss(0.4, 0.8, i),
        ],
    }
}

/// Plays a track, crossfades on change, and applies the shared delay and reverb.
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
    seed: u32,
    pub volume: f32,
}

const FADE_SECONDS: f32 = 2.4;
/// A cut is immediate to the ear. This is only long enough to stop the speakers clicking.
const CUT_SECONDS: f32 = 0.006;

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

    /// Changes track the way that track wants: death tracks cut in, the rest fade.
    pub fn set_track(&mut self, track: Track) {
        if track.is_death() || self.cutting_to.is_some() {
            return self.cut_to(track);
        }
        if self.current.1 == track {
            return;
        }
        self.seed = self.seed.wrapping_mul(31).wrapping_add(7);
        let new = (Player::new(track, self.seed), track);
        self.outgoing = Some(std::mem::replace(&mut self.current, new));
        self.fade = 0.0;
        self.update_delay();
    }

    /// Drops whatever is playing, echoes and all, and starts this track from the top.
    pub fn cut_to(&mut self, track: Track) {
        if self.cutting_to == Some(track) || (self.cutting_to.is_none() && self.current.1 == track) {
            return;
        }
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

    /// Adds into an interleaved stereo buffer.
    pub fn render_add(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            if let Some(track) = self.cutting_to {
                self.cut_gain -= 1.0 / (CUT_SECONDS * SR);
                if self.cut_gain <= 0.0 {
                    self.finish_cut(track);
                }
            }
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
            let gain = self.volume * self.cut_gain;
            frame[0] += (l + dl * 0.45 + rl) * gain;
            frame[1] += (r + dr * 0.45 + rr) * gain;
        }
    }
}
