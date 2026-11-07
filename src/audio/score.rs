//! The score: every note of ROTFALL's soundtrack, as data. `music.rs` explains the vocabulary, and
//! `mass.rs` the wall of sound behind it.
use super::mass::*;
use super::music::Inst::*;
use super::music::*;

// ---- The machine the night runs on.

/// The kick, on every beat, forever.
const KICK: &str = "x...x...x...x...";
/// Something the size of a building, landing on the third beat of the bar.
const SLAM_BAR: Part = hit(Slam, "C2", 0.5, 0.0, "........x.......");
/// The same, every other bar, for when there is still a little room left.
const SLAM_2: Part = hit(Slam, "C2", 0.5, 0.0, "........................x.......");
/// Dry metal ticks, on the beat and between the beats, never leaning on the offbeat.
const TICKS: &str = "x.x.x.x.x.x.x.x.";
const TICKS_FINE: &str = "xoxoxoxoxoxoxoxo";
/// One rivet driven home at the end of each bar.
const RIVET: Part = hit(Anvil, "F4", 0.15, -0.35, "............x...");
/// War drums, for when the whole machine is committed.
const WAR: &str = "x..x..x.x..x..x.";
const VENT: Part = hit(OpenHat, "C4", 0.32, 0.25, "..............................................................x.");

// How often the whole Wall slams down.
const EVERY_BAR: &str = "x...............";
const EVERY_2: &str = "x...............................";
const EVERY_4: &str = "x...............................................................";

// ---- The call sign, a falling tritone on the horn: ROT, then FALL. Once in every four or eight bars.
const CALL_A: Part = line(Horn, 0.28, 0.0, "D#3+D#2:8 A2+A1:28 -:28");
const CALL_E: Part = line(Horn, 0.28, 0.0, "A#2+A#1:8 E2+E1:28 -:28");
const CALL_D: Part = line(Horn, 0.28, 0.0, "G#2+G#1:8 D2+D1:28 -:28");
const CALL_F: Part = line(Horn, 0.28, 0.0, "B2+B1:8 F2+F1:28 -:28");
const CALL_C: Part = line(Horn, 0.28, 0.0, "F#2+F#1:8 C2+C1:28 -:28");
const CALL8_E: Part = line(Horn, 0.28, 0.0, "A#2+A#1:8 E2+E1:28 -:92");
const CALL8_D: Part = line(Horn, 0.28, 0.0, "G#2+G#1:8 D2+D1:28 -:92");
const CALL8_F: Part = line(Horn, 0.28, 0.0, "B2+B1:8 F2+F1:28 -:92");
const CALL8_C: Part = line(Horn, 0.28, 0.0, "F#2+F#1:8 C2+C1:28 -:92");

// ---- The drive that only starts when the machine is fully committed: a low rolling pulse between the
// kicks, on one note, however the harmony moves above it.
const ROLL: &str = ".xxx.xxx.xxx.xxx";
const ENGINE_D: Part = hit(Bass, "D2", 0.3, 0.0, ROLL);
const ENGINE_E: Part = hit(Bass, "E2", 0.3, 0.0, ROLL);
const ENGINE_F: Part = hit(Bass, "F2", 0.3, 0.0, ROLL);
const ENGINE_C: Part = hit(Bass, "C2", 0.3, 0.0, ROLL);

/// The end of the line: the machine stops, and the wall comes down. Every night song ends here.
const COLLAPSE: Section = Section {
    pad: 0, energy: 0.7, hall: 0.95, dark: (0.9, 0.35), rumble: (0.5, 0.0),
    mass: MassCue { level: (1.0, 0.0), lo: (28.0, 33.0), hi: (88.0, 67.0), grit: (0.5, 0.2), bite: (0.9, 0.15), ..QUIET },
    ..E
};

// DESCENT. The reactor alone in the dark, and then the sound of everything coming for it. Each time
// the tune comes back, the whole world has sunk a little further.
const DESCENT_THEME: &str = "E5:8 A5:4 G5:4 F5:8 E5:8 G5:6 E5:2 C5:8 D5:8 B4:8";
const DESCENT_GATES: Section = Section {
    chords: "A5 A5 F5 F5 C5 C5 G5 G5", pad: 0, kick: KICK, hat: TICKS, bass: "0...0...0...0...", pump: 0.6, shift: -1,
    ..E
};
static MENU: &[Section] = &[
    // A hull strike, a Geiger counter, and wind.
    Section {
        chords: "A5 A5 A5 A5", pad: 0, energy: 0.3, repeats: 2, hall: 0.95, dark: (0.4, 0.6),
        parts: &[
            line(Hum, 0.18, 0.0, "A2:64"),
            line(Hull, 0.42, 0.0, "A2:64"),
            line(Geiger, 0.26, 0.35, "C1:64"),
            line(Wind, 0.42, -0.3, "A3:64"),
        ],
        mass: MassCue { level: (0.0, 0.2), lo: (33.0, 33.0), hi: (50.0, 62.0), bite: (0.12, 0.25), ..QUIET },
        ..E
    },
    Section {
        chords: "Am Am F F", lead: "A4:16 C5:16 A4:16 F4:16", pad: 1, energy: 0.4, repeats: 2, dark: (0.55, 0.75),
        parts: &[
            line(Hum, 0.18, 0.0, "A2:32 F2:32"),
            hit(Heart, "C2", 0.42, 0.0, "x.......x......."),
            line(Geiger, 0.25, 0.35, "D1:64"),
        ],
        mass: MassCue { level: (0.2, 0.26), lo: (33.0, 33.0), hi: (62.0, 67.0), grit: (0.0, 0.05), bite: (0.22, 0.3), ..QUIET },
        ..E
    },
    Section {
        chords: "Am F C G", bass: "0...............", pad: 1, energy: 0.55, repeats: 2, rumble: (0.15, 0.4),
        lead: DESCENT_THEME,
        parts: &[
            line(SynthStrings, 0.13, -0.15, DESCENT_THEME),
            hit(Kick, "C4", 0.5, 0.0, "x.......x......."),
            line(Geiger, 0.25, 0.35, "F1:64"),
        ],
        mass: MassCue { level: (0.22, 0.34), lo: (33.0, 33.0), hi: (67.0, 74.0), grit: (0.03, 0.08), bite: (0.28, 0.42), ..QUIET },
        ..E
    },
    Section {
        chords: "Dm Am F E", bass: "0.......5.......", arp: "0...2...1...2...", pad: 1, energy: 0.65, swell: true, rumble: (0.4, 0.6),
        lead: "F5:4 E5:4 D5:8 C5:4 E5:4 A4:8 C5:4 D5:4 F5:8 E5:12 G#4:4",
        parts: &[
            CALL_A,
            line(SynthStrings, 0.14, -0.15, "F5:4 E5:4 D5:8 C5:4 E5:4 A4:8 C5:4 D5:4 F5:8 E5:12 G#4:4"),
            hit(Kick, "C4", 0.55, 0.0, "x.......x......."),
        ],
        mass: MassCue { level: (0.34, 0.6), lo: (33.0, 31.0), hi: (74.0, 84.0), grit: (0.08, 0.2), bite: (0.42, 0.65), tutti: EVERY_4, ..QUIET },
        ..E
    },
    // The horde reaches the gates. The machine starts, a semitone down.
    Section {
        energy: 0.8, rumble: (0.5, 0.65),
        kit: &[SLAM_2],
        parts: &[line(Hum, 0.18, 0.0, "A2:32 F2:32 C3:32 G2:32"), line(Geiger, 0.26, 0.3, "C2>C4:128")],
        mass: MassCue { level: (0.45, 0.68), lo: (31.0, 29.0), hi: (76.0, 84.0), grit: (0.1, 0.2), bite: (0.45, 0.68), tutti: EVERY_4, ..QUIET },
        ..DESCENT_GATES
    },
    // The tune returns, down in the machine, with the bell still ringing above it.
    Section {
        energy: 0.9, rumble: (0.65, 0.8), gasp: true,
        lead: "E5:16 A5:8 G5:8 F5:16 E5:16 G5:12 E5:4 C5:16 D5:16 B4:16",
        bass: ".000.000.000.000",
        kit: &[SLAM_BAR, RIVET],
        parts: &[
            line(Riff, 0.42, 0.0, "E3:16 A3:8 G3:8 F3:16 E3:16 G3:12 E3:4 C3:16 D3:16 B2:16"),
            line(Horn, 0.28, 0.0, "D#3+D#2:8 A2+A1:28 -:92"),
        ],
        mass: MassCue { level: (0.68, 0.9), lo: (29.0, 28.0), hi: (84.0, 88.0), grit: (0.2, 0.3), bite: (0.68, 0.85), tutti: EVERY_2, ..QUIET },
        ..DESCENT_GATES
    },
    // Half the speed, ten times the weight, and another semitone down.
    Section {
        chords: "Am F C G", kick: "x.......x.......", bass: "0.......0.......", pad: 0, shift: -2,
        tom: "x...............", low: "0...0...0...0...", brass: "E4:16 F4:16 G4:16 G4:16", energy: 1.0, repeats: 2, rumble: (0.9, 1.0), pump: 0.7,
        lead: "A5:8 C6:4 B5:4 A5:8 F5:8 G5:6 E5:2 C5:8 D5:8 B4:8",
        kit: &[SLAM_BAR],
        parts: &[
            line(Riff, 0.46, 0.0, "A3:8 C4:4 B3:4 A3:8 F3:8 G3:6 E3:2 C3:8 D3:8 B2:8"),
            CALL_A,
            line(Collapse, 0.42, 0.0, "C2:64"),
        ],
        mass: MassCue { level: (0.9, 0.98), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.28, 0.4), bite: (0.85, 0.95), tutti: EVERY_BAR, ..QUIET },
        ..E
    },
    // The power fails.
    Section {
        chords: "A5 A5", kick: "x.......x.......", pad: 0, energy: 1.0, speed: (1.0, 0.0), rumble: (0.9, 0.9), shift: -2,
        parts: &[line(Collapse, 0.6, 0.0, "C2:32")],
        mass: MassCue { level: (1.0, 0.9), lo: (26.0, 26.0), hi: (91.0, 86.0), grit: (0.4, 0.5), bite: (0.95, 0.6), ..QUIET },
        ..E
    },
    // Whatever is left.
    Section {
        chords: "Dm Am E Am", bass: "0...............", pad: 1, energy: 0.5, dark: (0.55, 0.36), hall: 0.95,
        lead: "F5:16 E5:16 E5:8 G#4:8 A4:16",
        parts: &[
            line(Hull, 0.4, 0.0, "A2:64"),
            line(Wind, 0.42, 0.3, "A3:64"),
            line(Geiger, 0.25, -0.35, "C1:64"),
            hit(Heart, "C2", 0.4, 0.0, "x..............................."),
        ],
        mass: MassCue { level: (0.34, 0.05), lo: (31.0, 33.0), hi: (70.0, 60.0), grit: (0.15, 0.0), bite: (0.32, 0.14), ..QUIET },
        ..E
    },
];

// DAYTIME ESCAPE. Sun through the ash, a clock, and always something wrong at the edge of hearing.
const CLOCK: &[Part] = &[hit(Tick, "A5", 0.16, -0.2, "x.......x......."), hit(Tick, "E5", 0.13, 0.2, "....x.......x...")];
const CLOCK_FAST: &[Part] = &[hit(Tick, "A5", 0.2, -0.2, "x...x...x...x..."), hit(Tick, "E5", 0.17, 0.2, "..x...x...x...x.")];
const DAY_BEAT: Section = Section {
    chords: "Em C G D", kick: "x.......x.......", snare: "....x.......x...", hat: "..x...x...x...x.",
    bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.", low: "0...0...0...0...", pad: 1,
    ..E
};
const DAY_THEME: &str = "B4:3 E5:3 G5:2 F#5:4 E5:4 E5:3 C5:3 E5:2 G5:8 D5:3 G5:3 B5:2 A5:4 G5:4 F#5:6 D5:2 A4:8";
static BUILD: &[Section] = &[
    Section {
        chords: "Em C G D", arp: "0.1.2.3.4.3.2.1.", pad: 1, energy: 0.45, dark: (0.6, 0.85),
        kit: CLOCK,
        parts: &[line(Wind, 0.3, 0.3, "E4:64"), line(Geiger, 0.16, -0.4, "C1:64")],
        mass: MassCue { level: (0.05, 0.12), lo: (40.0, 40.0), hi: (64.0, 72.0), bite: (0.2, 0.3), ..QUIET },
        ..E
    },
    Section {
        lead: "-:16 -:16 -:16 -:16", energy: 0.6, repeats: 2, rumble: (0.05, 0.15),
        kit: CLOCK,
        mass: MassCue { level: (0.1, 0.15), lo: (40.0, 38.0), hi: (72.0, 76.0), bite: (0.25, 0.35), ..QUIET },
        ..DAY_BEAT
    },
    Section {
        energy: 0.75, rumble: (0.15, 0.2), lead: DAY_THEME, repeats: 2,
        kit: CLOCK,
        mass: MassCue { level: (0.15, 0.22), lo: (38.0, 36.0), hi: (76.0, 79.0), grit: (0.0, 0.03), bite: (0.32, 0.42), ..QUIET },
        ..DAY_BEAT
    },
    Section {
        chords: "C G Am Em",
        kick: "x.......x.......", hat: "..x...x...x...x.", bass: "0...0..3..5.0...", arp: "0.1.2.3.4.3.2.1.",
        brass: "G4:8 C5:8 B4:8 D5:8 C5:8 A4:8 G4:8 B4:8", energy: 0.8, swell: true, pad: 1, rumble: (0.2, 0.3),
        kit: CLOCK,
        mass: MassCue { level: (0.22, 0.4), lo: (36.0, 33.0), hi: (79.0, 86.0), grit: (0.03, 0.06), bite: (0.42, 0.62), ..QUIET },
        ..E
    },
    Section {
        chords: "Am C Em D",
        kick: "x...x...x...x...", snare: "....x.......x..x", hat: "x.x.x.x.x.x.x.x.",
        bass: "0.0.3.0.5.0.3.0.", arp: "4.3.2.1.0.1.2.3.", tom: "x.......x.......",
        energy: 0.9, fill: true, pad: 1, rumble: (0.3, 0.4),
        lead: "A5:4 C6:4 B5:4 A5:4 G5:6 E5:2 G5:8 B5:4 G5:4 E5:4 D5:4 F#5:12 -:4",
        mass: MassCue { level: (0.32, 0.46), lo: (33.0, 31.0), hi: (84.0, 88.0), grit: (0.05, 0.08), bite: (0.52, 0.68), tutti: EVERY_4, ..QUIET },
        ..E
    },
    Section {
        chords: "Em C G D",
        kick: "x...x...x...x...", snare: "....x.......x...", hat: "x.x.x.x.x.x.x.x.", ride: "x...x...x...x...",
        bass: "0.0.3.0.5.0.3.0.", arp: "0.1.2.3.4.3.2.1.", low: "0.0.0.0.0.0.0.0.", tom: "x...............",
        brass: "E4:16 E4:16 D4:16 D4:16", energy: 1.0, repeats: 2, pad: 1, rumble: (0.4, 0.5),
        lead: DAY_THEME,
        mass: MassCue { level: (0.42, 0.55), lo: (31.0, 28.0), hi: (86.0, 88.0), grit: (0.06, 0.1), bite: (0.58, 0.7), tutti: EVERY_2, ..QUIET },
        ..E
    },
    // The light goes, all at once, the way it does now. The tune is reduced to a semitone.
    Section {
        chords: "Em F Em F", pad: 1, energy: 0.7, repeats: 2, dark: (1.0, 0.42), rumble: (0.3, 0.7), gasp: true,
        kick: "x.......x.......", bass: "0.......2.......",
        lead: "B4:8 A#4:8 B4:8 A#4:8 B4:8 A#4:8 G4:8 F4:8",
        kit: CLOCK_FAST,
        parts: &[CALL_E, line(Geiger, 0.25, -0.3, "C2>C4:128")],
        mass: MassCue { level: (0.25, 0.68), lo: (33.0, 28.0), hi: (79.0, 86.0), grit: (0.1, 0.45), bite: (0.55, 0.45), ..QUIET },
        ..E
    },
    // The same harmless pattern as the first thing you heard, on chords that no longer belong together.
    Section {
        chords: "Em Cm G#m Em", arp: "0.1.2.3.4.3.2.1.", pad: 1, energy: 0.5, repeats: 2, dark: (0.5, 0.7), tempo: (1.0, 0.86),
        kit: CLOCK,
        parts: &[line(Hull, 0.36, 0.0, "E2:128"), line(Wind, 0.3, -0.3, "E4:128")],
        mass: MassCue { level: (0.42, 0.14), lo: (28.0, 36.0), hi: (84.0, 70.0), grit: (0.25, 0.1), bite: (0.4, 0.28), ..QUIET },
        ..E
    },
];

// HOLDOUT. The walls hold, for now. A long march, and it gets heavier every time around.
const HOLD_THEME: &str = "D5:3 F5:3 A5:2 G5:4 F5:4 F5:3 D5:3 F5:2 A#5:8 A5:3 F5:3 C5:2 F5:4 A5:4 G5:6 E5:2 C5:8";
const DRONE_D: Part = line(Hum, 0.2, 0.0, "D2:128");
const HOLD_MARCH: Section = Section { kick: KICK, bass: "0...0...0...0...", hat: TICKS, pad: 0, ..E };
const HOLD_PEAK: Section = Section {
    chords: "Dm Dm Bb5 A", tom: WAR, stab: "x..x..x.x..x..x.", sub: "x...............", hat: TICKS_FINE, energy: 1.0, pump: 0.65,
    lead: "A5:2 D6:2 C6:2 A5:2 F5:2 A5:2 D6:4 A#5:2 D6:2 C6:2 A#5:2 F5:2 A#5:2 D6:4 A#5:2 F5:2 D5:2 F5:2 A#5:4 C6:4 C#6:8 A5:4 E5:4",
    ..HOLD_MARCH
};
static COMBAT_A: &[Section] = &[
    // A warning. Nothing is moving yet.
    Section {
        chords: "D5 D5 D5 D5", pad: 0, energy: 0.6, hall: 0.92, dark: (0.4, 0.75),
        parts: &[line(Hull, 0.5, 0.0, "D2:64"), line(Hum, 0.2, 0.0, "D2:64"), CALL_D],
        mass: MassCue { level: (0.12, 0.38), lo: (36.0, 33.0), hi: (60.0, 69.0), grit: (0.0, 0.1), bite: (0.2, 0.4), ..QUIET },
        ..E
    },
    // The machine starts. Kick and drone, and that is all it needs.
    Section {
        chords: "Dm Dm Bb5 C5", energy: 0.6, repeats: 2, rumble: (0.2, 0.35),
        parts: &[DRONE_D, SLAM_2],
        mass: MassCue { level: (0.3, 0.42), lo: (33.0, 33.0), hi: (67.0, 72.0), grit: (0.0, 0.05), bite: (0.42, 0.48), ..QUIET },
        ..HOLD_MARCH
    },
    Section {
        chords: "Dm Bb5 F5 C5", stab: "x..x..x.........", lead: HOLD_THEME, energy: 0.75, repeats: 2, rumble: (0.35, 0.45),
        parts: &[DRONE_D, SLAM_BAR],
        mass: MassCue { level: (0.42, 0.52), lo: (33.0, 33.0), hi: (72.0, 74.0), grit: (0.03, 0.08), bite: (0.48, 0.52), tutti: EVERY_4, ..QUIET },
        ..HOLD_MARCH
    },
    // Round again, and the bright chord has gone bad.
    Section {
        chords: "Dm Bbm F5 C5", stab: "x..x..x.........", lead: HOLD_THEME, hat: TICKS_FINE, energy: 0.85, repeats: 2, rumble: (0.45, 0.55),
        parts: &[ENGINE_D, DRONE_D, SLAM_BAR, RIVET, CALL8_D],
        mass: MassCue { level: (0.52, 0.66), lo: (33.0, 31.0), hi: (74.0, 79.0), grit: (0.08, 0.15), bite: (0.52, 0.6), tutti: EVERY_4, ..QUIET },
        ..HOLD_MARCH
    },
    Section {
        chords: "Gm Dm Bb5 A", tom: WAR, stab: "x.....x.....x...", energy: 0.9, fill: true, repeats: 2, rumble: (0.55, 0.65),
        lead: "G5:2 A5:2 A#5:4 A5:2 G5:2 F5:4 A5:6 D5:2 F5:8 A#5:4 A5:4 G5:4 F5:4 E5:6 C#5:2 A4:8",
        parts: &[DRONE_D, SLAM_BAR, RIVET],
        mass: MassCue { level: (0.6, 0.75), lo: (31.0, 29.0), hi: (79.0, 84.0), grit: (0.12, 0.2), bite: (0.55, 0.68), tutti: EVERY_2, ..QUIET },
        ..HOLD_MARCH
    },
    // The Trench: no tune, and two chords a semitone apart, grinding against each other.
    Section {
        chords: "D5 D#5 D5 D#5", energy: 0.85, repeats: 2, rumble: (0.5, 0.9), pump: 0.65, gasp: true, swell: true, dark: (0.45, 1.0),
        parts: &[line(Hum, 0.2, 0.0, "D2:16 D#2:16 D2:16 D#2:16"), SLAM_BAR, CALL_D],
        mass: MassCue { level: (0.3, 1.0), lo: (33.0, 28.0), hi: (62.0, 86.0), grit: (0.1, 0.4), bite: (0.3, 0.85), ..QUIET },
        ..HOLD_MARCH
    },
    // Everything.
    Section {
        repeats: 2, rumble: (0.8, 0.85),
        parts: &[ENGINE_D, SLAM_BAR, RIVET, CALL_D],
        mass: MassCue { level: (0.85, 0.92), lo: (28.0, 28.0), hi: (86.0, 88.0), grit: (0.2, 0.28), bite: (0.75, 0.82), tutti: EVERY_BAR, ..QUIET },
        ..HOLD_PEAK
    },
    // And then the floor gives way: the same again, a semitone further down.
    Section {
        repeats: 2, rumble: (0.85, 1.0), shift: -1,
        parts: &[ENGINE_D, SLAM_BAR, RIVET, CALL_D, line(Collapse, 0.4, 0.0, "C2:64")],
        mass: MassCue { level: (0.92, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.28, 0.4), bite: (0.82, 0.9), tutti: EVERY_BAR, ..QUIET },
        ..HOLD_PEAK
    },
    // The skeleton: the kick, the reactor, and the first three notes of the tune.
    Section {
        chords: "D5 D5 D5 D5", kick: KICK, pad: 0, energy: 0.55, repeats: 2, shift: -1, dark: (0.5, 0.8), rumble: (0.25, 0.4), hall: 0.93,
        parts: &[
            line(Riff, 0.3, 0.0, "D3:3 F3:3 A3:2 -:56"),
            line(Hum, 0.2, 0.0, "D2:64"),
            line(Hull, 0.4, 0.0, "D2:128"),
            line(Geiger, 0.25, -0.4, "C2>C4:128"),
            VENT,
        ],
        mass: MassCue { level: (0.12, 0.3), lo: (36.0, 33.0), hi: (60.0, 67.0), grit: (0.05, 0.2), bite: (0.25, 0.4), ..QUIET },
        ..E
    },
    // The last time round, two semitones under where it started.
    Section {
        chords: "Dm Bbm F5 C5", stab: "x..x..x.x..x..x.", lead: HOLD_THEME, tom: WAR, sub: "x...............", hat: TICKS_FINE, energy: 1.0, repeats: 2, shift: -2,
        rumble: (0.9, 1.0), pump: 0.7, gasp: true,
        parts: &[ENGINE_D, SLAM_BAR, RIVET, CALL8_D, line(Collapse, 0.42, 0.0, "C2:128")],
        mass: MassCue { level: (1.0, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.35, 0.5), bite: (0.85, 0.95), tutti: EVERY_BAR, ..QUIET },
        ..HOLD_MARCH
    },
    Section {
        chords: "D5 D5 D5 D5", shift: -2,
        parts: &[line(Hull, 0.55, 0.0, "D2:64"), line(Collapse, 0.7, 0.0, "C2:64"), line(Sub, 0.85, 0.0, "C2:64")],
        ..COLLAPSE
    },
];

// NO SLEEP. A heartbeat for a kick drum, a clock you can't turn off, and it never quite stops.
const NOSLEEP_THEME: &str = "C6:6 G#5:2 F5:8 C#6:6 A#5:2 F5:8 D#6:4 C#6:4 C6:4 G#5:4 G5:6 D#5:2 C5:8";
const NOSLEEP_KICK: &str = "x.....x...x.....";
const DRONE_F: Part = line(Hum, 0.2, 0.0, "F2:128");
const WALL_CLOCK: Part = hit(Tick, "A4", 0.13, 0.0, "x...x...x...x...");
const NOSLEEP_BASE: Section = Section { kick: NOSLEEP_KICK, bass: "0.....0...0.....", hat: TICKS, pad: 0, ..E };
const NOSLEEP_PEAK: Section = Section {
    chords: "C#5 G#5 D#5 Fm", kick: KICK, bass: "0...0...0...0...", hat: TICKS_FINE, tom: WAR, stab: "x.....x.....x...", sub: "x...............",
    energy: 1.0, pad: 0, pump: 0.65,
    lead: "F6:2 D#6:2 C#6:4 C6:2 C#6:2 G#5:4 D#6:2 C6:2 G#5:4 G5:2 G#5:2 D#5:4 A#5:2 C6:2 C#6:4 D#6:2 F6:2 G#6:4 F6:12 -:4",
    ..E
};
static COMBAT_B: &[Section] = &[
    Section {
        chords: "F5 F5 F5 F5", pad: 0, energy: 0.55, dark: (0.4, 0.75),
        parts: &[hit(Heart, "C2", 0.42, 0.0, "x.......x......."), WALL_CLOCK, line(Hum, 0.2, 0.0, "F2:64"), line(Hull, 0.45, 0.0, "F2:64"), CALL_F],
        mass: MassCue { level: (0.12, 0.36), lo: (36.0, 33.0), hi: (62.0, 70.0), grit: (0.0, 0.1), bite: (0.2, 0.38), ..QUIET },
        ..E
    },
    Section {
        chords: "Fm C#5 D#5 Cm", energy: 0.6, repeats: 2, rumble: (0.2, 0.35),
        parts: &[DRONE_F, WALL_CLOCK, SLAM_2],
        mass: MassCue { level: (0.3, 0.42), lo: (33.0, 33.0), hi: (67.0, 72.0), grit: (0.0, 0.05), bite: (0.42, 0.48), ..QUIET },
        ..NOSLEEP_BASE
    },
    Section {
        chords: "Fm C#5 D#5 Cm", arp: "0120120120120120", lead: NOSLEEP_THEME, energy: 0.78, repeats: 2, rumble: (0.35, 0.45),
        parts: &[DRONE_F, WALL_CLOCK, SLAM_BAR],
        mass: MassCue { level: (0.42, 0.52), lo: (33.0, 33.0), hi: (72.0, 74.0), grit: (0.03, 0.08), bite: (0.48, 0.52), tutti: EVERY_4, ..QUIET },
        ..NOSLEEP_BASE
    },
    Section {
        chords: "Fm C#m D#5 Cm", arp: "0120120120120120", lead: NOSLEEP_THEME, hat: TICKS_FINE, energy: 0.86, repeats: 2, rumble: (0.45, 0.58),
        parts: &[ENGINE_F, DRONE_F, WALL_CLOCK, SLAM_BAR, RIVET, CALL8_F],
        mass: MassCue { level: (0.52, 0.66), lo: (33.0, 31.0), hi: (74.0, 79.0), grit: (0.08, 0.16), bite: (0.52, 0.6), tutti: EVERY_4, ..QUIET },
        ..NOSLEEP_BASE
    },
    // Nodding off, against your better judgement. Everything slows and sinks.
    Section {
        chords: "C#5 G#5 D#5 F5", pad: 0, energy: 0.7, swell: true, repeats: 2, dark: (0.8, 0.3), tempo: (1.0, 0.8), hall: 0.93, gasp: true,
        parts: &[hit(Heart, "C2", 0.45, 0.0, "x...............x.......x......."), WALL_CLOCK, line(Hum, 0.2, 0.0, "F2:128"), line(Hull, 0.36, 0.0, "F2:128")],
        mass: MassCue { level: (0.4, 0.18), lo: (33.0, 36.0), hi: (76.0, 62.0), grit: (0.15, 0.3), bite: (0.45, 0.2), ..QUIET },
        ..E
    },
    // Awake. For the first time the kick falls square on every beat, and it does not let up.
    Section {
        repeats: 2, rumble: (0.8, 0.88),
        parts: &[ENGINE_F, SLAM_BAR, RIVET, CALL_F],
        mass: MassCue { level: (0.85, 0.92), lo: (28.0, 28.0), hi: (86.0, 88.0), grit: (0.2, 0.28), bite: (0.75, 0.82), tutti: EVERY_BAR, ..QUIET },
        ..NOSLEEP_PEAK
    },
    Section {
        repeats: 2, rumble: (0.88, 1.0), shift: -1, fill: true,
        parts: &[ENGINE_F, SLAM_BAR, RIVET, CALL_F, line(Collapse, 0.4, 0.0, "C2:64")],
        mass: MassCue { level: (0.92, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.28, 0.4), bite: (0.82, 0.9), tutti: EVERY_BAR, ..QUIET },
        ..NOSLEEP_PEAK
    },
    // Three in the morning. Only the clock, and your own heart, and something knocking.
    Section {
        chords: "F5 F5 C#5 C#5", pad: 0, energy: 0.55, repeats: 2, shift: -1, kick: NOSLEEP_KICK, dark: (0.5, 0.8), rumble: (0.2, 0.4), hall: 0.93,
        parts: &[
            WALL_CLOCK,
            hit(Heart, "C2", 0.38, 0.0, "x...............x..............."),
            hit(Anvil, "E4", 0.12, 0.45, "...............x.............x.."),
            line(Hum, 0.2, 0.0, "F2:128"),
            line(Riff, 0.28, 0.0, "C4:6 G#3:2 F3:8 -:48"),
            line(Geiger, 0.25, -0.35, "C2>C4:128"),
        ],
        mass: MassCue { level: (0.12, 0.32), lo: (36.0, 33.0), hi: (60.0, 67.0), grit: (0.05, 0.2), bite: (0.25, 0.4), ..QUIET },
        ..E
    },
    Section {
        chords: "Fm C#m D#5 Cm", kick: KICK, bass: "0...0...0...0...", hat: TICKS_FINE, tom: WAR, stab: "x.....x.....x...", sub: "x...............",
        lead: NOSLEEP_THEME, energy: 1.0, pad: 0, repeats: 2, shift: -2, rumble: (0.9, 1.0), pump: 0.7, gasp: true,
        parts: &[ENGINE_F, SLAM_BAR, RIVET, CALL8_F, line(Collapse, 0.42, 0.0, "C2:128")],
        mass: MassCue { level: (1.0, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.35, 0.5), bite: (0.85, 0.95), tutti: EVERY_BAR, ..QUIET },
        ..E
    },
    Section {
        chords: "F5 F5 F5 F5", shift: -2,
        parts: &[line(Hull, 0.55, 0.0, "F2:64"), line(Collapse, 0.7, 0.0, "C2:64"), line(Sub, 0.85, 0.0, "C2:64")],
        ..COLLAPSE
    },
];

// RUST CHOIR. An assembly line that was never told to stop. Its choir is the machinery itself.
const LINE_CLANG: Part = hit(Anvil, "C4", 0.22, -0.4, "x.......x.......");
const LINE_CLANK: Part = hit(Anvil, "F#4", 0.15, 0.45, "............x...");
const DRONE_C: Part = line(Hum, 0.2, 0.0, "C2:128");
const RUST_THEME: &str = "C5:8 D#5:4 F5:4 G5:12 D#5:4 G#5:8 G5:4 F5:4 D5:12 B4:4";
const RUST_MARCH: Section = Section {
    kick: "x..x..x.x..x..x.", bass: "0..0..0.0..0..0.", stab: "x..x..x.........", hat: "x...x...x...x...", pad: 0, pump: 0.6,
    ..E
};
const RUST_PEAK: Section = Section {
    chords: "G#5 D#5 Fm G5", kick: KICK, bass: "0...0...0...0...", hat: TICKS_FINE, tom: WAR, stab: "x.....x.x.....x.", sub: "x...............",
    energy: 1.0, pad: 0, pump: 0.65,
    lead: "C6:4 A#5:4 G#5:4 G5:4 G5:4 F5:4 D#5:8 F5:4 G5:4 G#5:4 A#5:4 B5:8 G5:8",
    ..E
};
static COMBAT_C: &[Section] = &[
    Section {
        chords: "C5 C5 G#5 G5", pad: 0, energy: 0.55, dark: (0.45, 0.75),
        parts: &[LINE_CLANK, line(Hum, 0.2, 0.0, "C2:32 G#1:16 G1:16"), line(Hull, 0.45, 0.0, "C2:64"), CALL_C],
        mass: MassCue { level: (0.12, 0.36), lo: (36.0, 33.0), hi: (60.0, 69.0), grit: (0.0, 0.1), bite: (0.2, 0.4), ..QUIET },
        ..E
    },
    Section {
        chords: "Cm Cm G#5 G5", kick: "x..x..x.x.......", bass: "0..0..0.0.......", energy: 0.6, repeats: 2, rumble: (0.25, 0.35),
        parts: &[LINE_CLANK, DRONE_C, SLAM_2],
        mass: MassCue { level: (0.3, 0.42), lo: (33.0, 33.0), hi: (67.0, 72.0), grit: (0.0, 0.05), bite: (0.42, 0.48), ..QUIET },
        ..RUST_MARCH
    },
    Section {
        chords: "Cm G#5 Fm G5", lead: RUST_THEME, energy: 0.78, repeats: 2, rumble: (0.35, 0.48),
        parts: &[LINE_CLANG, LINE_CLANK, DRONE_C, SLAM_BAR],
        mass: MassCue { level: (0.42, 0.52), lo: (33.0, 33.0), hi: (72.0, 74.0), grit: (0.03, 0.08), bite: (0.48, 0.52), tutti: EVERY_4, ..QUIET },
        ..RUST_MARCH
    },
    Section {
        chords: "Cm G#5 C#5 G5", lead: RUST_THEME, hat: TICKS, energy: 0.88, repeats: 2, rumble: (0.48, 0.62),
        parts: &[LINE_CLANG, LINE_CLANK, DRONE_C, SLAM_BAR, CALL8_C],
        mass: MassCue { level: (0.52, 0.66), lo: (33.0, 31.0), hi: (74.0, 79.0), grit: (0.08, 0.16), bite: (0.52, 0.6), tutti: EVERY_4, ..QUIET },
        ..RUST_MARCH
    },
    // The line stops. Nobody has heard that before, and nobody knows what it means.
    Section {
        chords: "C5 C5 C5 C5", pad: 0, energy: 0.6, dark: (0.55, 0.95), gasp: true, swell: true, hall: 0.93,
        parts: &[VENT, LINE_CLANK, line(Hull, 0.5, 0.0, "C2:64"), line(Hum, 0.2, 0.0, "C2:64")],
        mass: MassCue { level: (0.2, 0.85), lo: (33.0, 28.0), hi: (60.0, 86.0), grit: (0.05, 0.35), bite: (0.28, 0.85), ..QUIET },
        ..E
    },
    Section {
        repeats: 2, rumble: (0.8, 0.88),
        parts: &[ENGINE_C, LINE_CLANG, LINE_CLANK, SLAM_BAR, CALL_C],
        mass: MassCue { level: (0.85, 0.92), lo: (28.0, 28.0), hi: (86.0, 88.0), grit: (0.2, 0.28), bite: (0.75, 0.82), tutti: EVERY_BAR, ..QUIET },
        ..RUST_PEAK
    },
    Section {
        repeats: 2, rumble: (0.88, 1.0), shift: -1, fill: true,
        parts: &[ENGINE_C, LINE_CLANG, LINE_CLANK, SLAM_BAR, CALL_C, line(Collapse, 0.4, 0.0, "C2:64")],
        mass: MassCue { level: (0.92, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.28, 0.42), bite: (0.82, 0.92), tutti: EVERY_BAR, ..QUIET },
        ..RUST_PEAK
    },
    Section {
        chords: "C5 C5 C5 C5", shift: -1,
        parts: &[line(Hull, 0.55, 0.0, "C2:64"), line(Collapse, 0.7, 0.0, "C2:64"), line(Sub, 0.85, 0.0, "C2:64")],
        ..COLLAPSE
    },
];

// COLOSSUS. You hear it long before you see it. It isn't running. It has never needed to.
const DRONE_E: Part = line(Hum, 0.2, 0.0, "E2:128");
const STEP: Part = hit(Sub, "C2", 0.85, 0.0, "x...............................");
const BOSS_VERSE: Section = Section {
    chords: "Em F5 Em C5", kick: KICK, bass: "0...0...0...0...", hat: TICKS, pad: 0, pump: 0.65,
    lead: "E5:4 F5:4 G5:4 F5:4 E5:12 B4:4 C5:4 D5:4 E5:4 F5:4 G5:8 E5:8",
    ..E
};
const BOSS_PEAK: Section = Section {
    chords: "C5 F5 Dm E5", kick: KICK, bass: ".000.000.000.000", hat: TICKS_FINE, tom: WAR, stab: "x..x..x.x..x..x.", sub: "x.......x.......",
    energy: 1.0, pad: 0, pump: 0.7,
    lead: "G5:4 C6:4 E6:8 F6:4 E6:4 C6:8 D6:4 C6:4 A5:8 G#5:4 B5:4 E6:8",
    ..E
};
static BOSS: &[Section] = &[
    // Its footsteps, and the silence between them, and each one closer.
    Section {
        chords: "E5 E5 E5 F5", pad: 0, energy: 0.7, repeats: 2, dark: (0.25, 0.72), hall: 0.95, rumble: (0.7, 0.9),
        parts: &[
            STEP,
            hit(Collapse, "C2", 0.5, 0.0, "x..............................."),
            line(Hull, 0.45, 0.0, "E2:32 E2:32"),
            line(Hum, 0.2, 0.0, "E2:64"),
        ],
        mass: MassCue { level: (0.15, 0.55), lo: (28.0, 28.0), hi: (52.0, 72.0), grit: (0.0, 0.15), bite: (0.2, 0.5), ..QUIET },
        ..E
    },
    // It arrives.
    Section {
        chords: "Em F5 Em F5", tom: "x.x.x.x.x.x.x.x.", sub: "x...............", energy: 0.7, swell: true, gasp: true, pad: 0, rumble: (0.6, 0.8),
        parts: &[CALL_E, line(Hum, 0.2, 0.0, "E2:16 F2:16 E2:16 F2:16")],
        mass: MassCue { level: (0.6, 0.85), lo: (28.0, 28.0), hi: (72.0, 84.0), grit: (0.15, 0.25), bite: (0.55, 0.8), tutti: EVERY_4, ..QUIET },
        ..E
    },
    Section {
        energy: 0.8, repeats: 2, rumble: (0.5, 0.6),
        parts: &[DRONE_E, SLAM_BAR],
        mass: MassCue { level: (0.5, 0.6), lo: (28.0, 28.0), hi: (76.0, 79.0), grit: (0.08, 0.12), bite: (0.6, 0.68), tutti: EVERY_4, ..QUIET },
        ..BOSS_VERSE
    },
    Section {
        energy: 0.88, repeats: 2, rumble: (0.6, 0.75), stab: "x..x..x.........",
        parts: &[ENGINE_E, DRONE_E, SLAM_BAR, RIVET, CALL8_E],
        mass: MassCue { level: (0.62, 0.78), lo: (28.0, 28.0), hi: (79.0, 84.0), grit: (0.12, 0.18), bite: (0.65, 0.78), tutti: EVERY_2, ..QUIET },
        ..BOSS_VERSE
    },
    Section {
        chords: "Am F5 Em Em", kick: KICK, bass: "0...0...0...0...", hat: TICKS_FINE, tom: WAR, stab: "x.......x.......", sub: "x...............",
        energy: 0.95, pad: 0, repeats: 2, rumble: (0.75, 0.9), pump: 0.7,
        lead: "A5:2 B5:2 C6:4 B5:2 A5:2 F5:4 A5:2 C6:2 F6:4 E6:8 G5:2 B5:2 E6:4 D#6:4 E6:4 B5:2 G5:2 E5:8 -:4",
        parts: &[ENGINE_E, SLAM_BAR, RIVET],
        mass: MassCue { level: (0.78, 0.92), lo: (28.0, 28.0), hi: (84.0, 88.0), grit: (0.18, 0.25), bite: (0.78, 0.88), tutti: EVERY_BAR, ..QUIET },
        ..E
    },
    // It stops, and looks at you.
    Section {
        chords: "Am F5 C5 E5", pad: 0, energy: 0.7, swell: true, gasp: true, dark: (0.45, 1.0), rumble: (0.2, 0.45),
        parts: &[line(Hull, 0.5, 0.0, "E2:64"), line(Hum, 0.2, 0.0, "E2:64")],
        mass: MassCue { level: (0.2, 1.0), lo: (33.0, 28.0), hi: (62.0, 88.0), grit: (0.1, 0.35), bite: (0.3, 0.95), ..QUIET },
        ..E
    },
    Section {
        repeats: 2, rumble: (0.9, 0.95),
        parts: &[SLAM_BAR, RIVET, CALL_E, line(Collapse, 0.4, 0.0, "C2:64")],
        mass: MassCue { level: (0.92, 1.0), lo: (28.0, 28.0), hi: (88.0, 88.0), grit: (0.25, 0.35), bite: (0.85, 0.92), tutti: EVERY_BAR, ..QUIET },
        ..BOSS_PEAK
    },
    // The same, and lower, because it has no reason to stop.
    Section {
        repeats: 2, rumble: (0.95, 1.0), shift: -1, fill: true,
        parts: &[SLAM_BAR, RIVET, CALL_E, line(Collapse, 0.42, 0.0, "C2:64")],
        mass: MassCue { level: (1.0, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.35, 0.5), bite: (0.92, 0.98), tutti: EVERY_BAR, ..QUIET },
        ..BOSS_PEAK
    },
    Section {
        chords: "E5 E5 E5 E5", shift: -1,
        parts: &[line(Hull, 0.55, 0.0, "E2:64"), line(Collapse, 0.7, 0.0, "C2:64"), line(Sub, 0.85, 0.0, "C2:64")],
        ..COLLAPSE
    },
];

// OVERRUN. The reactor goes. There is nothing to be sad about any more, and nobody to be sad.
const OVER_THEME: &str = "A4:16 F5:12 D5:4 G5:8 F5:8 E5:12 C#5:4";
const MELTDOWN: Section = Section {
    chords: "Dm Dm A#m A#m Gm Gm A5 A5", tempo: (2.0, 2.0), pad: 0, pump: 0.7, kick: KICK, bass: ".000.000.000.000", hat: TICKS,
    ..E
};
static GAMEOVER: &[Section] = &[
    Section {
        chords: "Dm Bb5 Gm A5", bass: "0...............", pad: 0, energy: 0.6, dark: (0.7, 0.95),
        lead: OVER_THEME, repeats: 2, rumble: (0.6, 0.5),
        parts: &[
            line(Horn, 0.4, 0.0, "G#2+G#1:6 D2+D1:26 -:32"),
            line(Sub, 0.85, 0.0, "C2:64"),
            line(Collapse, 0.6, 0.0, "C2:64"),
            line(Hull, 0.45, 0.0, "D2:32"),
            line(Hum, 0.18, 0.0, "D2:16 A#1:16 G1:16 A1:16"),
            hit(Kick, "C4", 0.55, 0.0, "x...............x..............."),
            line(Geiger, 0.25, 0.3, "C3:64"),
        ],
        mass: MassCue { level: (0.4, 0.34), lo: (28.0, 28.0), hi: (79.0, 76.0), grit: (0.25, 0.15), bite: (0.6, 0.45), tutti: EVERY_4, ..QUIET },
        ..E
    },
    Section {
        chords: "Gm Dm Bb5 A5", bass: "0...............", pad: 0, tom: "x...............",
        brass: "G4:16 F4:16 F4:16 E4:16", energy: 0.8, swell: true, rumble: (0.5, 0.75),
        lead: "A#4:16 F5:16 D5:16 E5:16",
        parts: &[hit(Kick, "C4", 0.6, 0.0, "x.......x......."), line(Geiger, 0.26, 0.3, "C4:64")],
        mass: MassCue { level: (0.45, 0.8), lo: (28.0, 28.0), hi: (76.0, 86.0), grit: (0.15, 0.3), bite: (0.5, 0.82), ..QUIET },
        ..E
    },
    // Meltdown: the machine at full speed for the last time, and the Geiger counter stops being clicks.
    Section {
        energy: 0.9, rumble: (0.7, 0.85),
        kit: &[SLAM_BAR, RIVET],
        parts: &[
            line(Riff, 0.42, 0.0, "A2:32 F3:24 D3:8 G3:16 F3:16 E3:24 C#3:8"),
            line(Horn, 0.3, 0.0, "G#2+G#1:8 D2+D1:28 -:92"),
            line(Geiger, 0.28, 0.2, "C4>C6:128"),
        ],
        mass: MassCue { level: (0.7, 0.88), lo: (28.0, 28.0), hi: (84.0, 88.0), grit: (0.2, 0.3), bite: (0.72, 0.85), tutti: EVERY_4, ..QUIET },
        ..MELTDOWN
    },
    Section {
        energy: 1.0, repeats: 2, rumble: (0.85, 1.0), shift: -1, hat: TICKS_FINE, tom: WAR,
        kit: &[SLAM_BAR, RIVET],
        parts: &[
            line(Riff, 0.44, 0.0, "A2:32 F3:24 D3:8 G3:16 F3:16 E3:24 C#3:8"),
            line(Horn, 0.3, 0.0, "G#2+G#1:8 D2+D1:28 -:92"),
            line(Geiger, 0.3, 0.2, "C6>C7:256"),
            line(Collapse, 0.4, 0.0, "C2:64"),
        ],
        mass: MassCue { level: (0.88, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.3, 0.55), bite: (0.85, 0.98), tutti: EVERY_BAR, ..QUIET },
        ..MELTDOWN
    },
    // White.
    Section {
        chords: "D5 D5 D5 D5", pad: 0, energy: 1.0, hall: 0.96, dark: (1.0, 0.25),
        parts: &[
            line(Collapse, 0.8, 0.0, "C2:16 C2:48"),
            line(Hull, 0.6, 0.0, "D2:64"),
            line(Sub, 0.85, 0.0, "C2:64"),
        ],
        mass: MassCue { level: (1.0, 0.0), lo: (26.0, 33.0), hi: (91.0, 60.0), grit: (0.55, 0.1), bite: (1.0, 0.1), ..QUIET },
        ..E
    },
    // Ash, settling.
    Section {
        chords: "Dm Bb Gm A", bass: "0...............", pad: 1, energy: 0.45, dark: (0.4, 0.55), hall: 0.95, repeats: 2,
        lead: OVER_THEME,
        parts: &[line(Wind, 0.45, -0.3, "D4:128"), line(Geiger, 0.25, 0.35, "C3>G0:128"), line(Hum, 0.12, 0.0, "D2:128")],
        mass: MassCue { level: (0.06, 0.16), lo: (40.0, 36.0), hi: (62.0, 72.0), bite: (0.15, 0.25), ..QUIET },
        ..E
    },
];

// ROT. It starts with the blow. Then the same few bars go round and round on a pulse that no longer
// matters, and each time, a little more of the music has gone.
const ROT_THEME: &str = "A4:8 F5:8 D5:8 A4:8 G4:8 A#4:8 D5:16 F4:8 A#4:8 D5:8 F5:8 E5:8 C#5:8 A4:16";
const ROT_LOOP: Section = Section {
    chords: "Dm Gm Bb5 A5", bass: "0...............", pad: 1, kick: KICK, lead: ROT_THEME,
    ..E
};
static ROT: &[Section] = &[
    Section {
        chords: "Dm Gm Bb5 A5", kick: "x..x............", bass: "0...............", pad: 0, energy: 0.65, rumble: (0.45, 0.3),
        lead: ROT_THEME,
        parts: &[
            line(Horn, 0.42, 0.0, "G#2+G#1:4 D2+D1:28 -:96"),
            line(Sub, 0.85, 0.0, "C2:128"),
            line(Hull, 0.55, 0.0, "D2:128"),
            line(Collapse, 0.6, 0.0, "C2:128"),
            line(Hum, 0.14, 0.0, "D2:128"),
            line(Geiger, 0.25, 0.35, "F1:128"),
        ],
        mass: MassCue { level: (0.2, 0.08), lo: (28.0, 36.0), hi: (79.0, 62.0), grit: (0.3, 0.1), bite: (0.6, 0.25), tutti: ONCE, ..QUIET },
        ..E
    },
    Section {
        chords: "Bb5 Gm A5 Dm", bass: "0...............", pad: 0, energy: 0.8, rumble: (0.3, 0.55),
        brass: "D4:16 D4:16 C#4:16 D4:16",
        lead: "F4:16 G4:16 A4:16 A4:16",
        parts: &[hit(Kick, "C4", 0.45, 0.0, "x...x...x...x..."), line(Hull, 0.36, 0.0, "D2:64")],
        mass: MassCue { level: (0.2, 0.42), lo: (33.0, 30.0), hi: (70.0, 79.0), grit: (0.1, 0.2), bite: (0.3, 0.5), ..QUIET },
        ..E
    },
    // Each pass, the same notes. Listen to what happens to them.
    Section {
        energy: 0.7, rumble: (0.3, 0.4), rot: (0.04, 0.22),
        mass: MassCue { level: (0.45, 0.55), lo: (30.0, 30.0), hi: (79.0, 84.0), grit: (0.1, 0.15), bite: (0.58, 0.65), ..QUIET },
        ..ROT_LOOP
    },
    Section {
        energy: 0.7, rumble: (0.4, 0.5), rot: (0.22, 0.5), dark: (1.0, 0.72),
        mass: MassCue { level: (0.55, 0.46), lo: (30.0, 30.0), hi: (84.0, 79.0), grit: (0.15, 0.25), bite: (0.65, 0.48), ..QUIET },
        ..ROT_LOOP
    },
    Section {
        energy: 0.6, rumble: (0.5, 0.45), rot: (0.5, 0.82), dark: (0.72, 0.5), kick: "x.......x.......",
        mass: MassCue { level: (0.44, 0.26), lo: (30.0, 33.0), hi: (79.0, 70.0), grit: (0.25, 0.3), bite: (0.42, 0.25), ..QUIET },
        ..ROT_LOOP
    },
    Section {
        energy: 0.5, rumble: (0.5, 0.0), rot: (0.82, 1.0), dark: (0.5, 0.28), hall: 0.96, pad: 0, kick: "x...............",
        parts: &[line(Wind, 0.4, 0.3, "D4:128")],
        mass: MassCue { level: (0.26, 0.0), lo: (33.0, 36.0), hi: (70.0, 55.0), grit: (0.3, 0.1), bite: (0.25, 0.1), ..QUIET },
        ..ROT_LOOP
    },
];

// ONE OF MANY. One voice, alone. Then the same tune again, and again, and each time there are more of
// you, exactly in step, until there is nothing left that could be called you.
const MANY_THEME: &str = "G4:8 C5:8 C#5:16 C5:8 G4:8 D#5:16 D5:8 B4:8 G4:16 G4:8 D#5:8 F5:16 D#5:8 C5:8 G4:8 C5:8 F4:8 D5:8 B4:8 G4:8";
const MANY_DOWN_ONE: &str = "G3:8 C4:8 C#4:16 C4:8 G3:8 D#4:16 D4:8 B3:8 G3:16 G3:8 D#4:8 F4:16 D#4:8 C4:8 G3:8 C4:8 F3:8 D4:8 B3:8 G3:8";
const MANY_DOWN_TWO: &str = "G2:8 C3:8 C#3:16 C3:8 G2:8 D#3:16 D3:8 B2:8 G2:16 G2:8 D#3:8 F3:16 D#3:8 C3:8 G2:8 C3:8 F2:8 D3:8 B2:8 G2:8";
static ONE_OF_MANY: &[Section] = &[
    // One.
    Section {
        chords: "Cm C#5 Cm G5", kick: "x.......x.x.....", bass: "0..0....0..0....", pad: 0, energy: 0.7, rumble: (0.4, 0.4),
        lead: MANY_THEME,
        parts: &[
            line(Horn, 0.38, 0.0, "F#2+F#1:4 C2+C1:28 -:160"),
            line(Sub, 0.85, 0.0, "C2:192"),
            line(Hum, 0.16, 0.0, "C2:192"),
        ],
        mass: MassCue { level: (0.3, 0.16), lo: (28.0, 33.0), hi: (76.0, 67.0), grit: (0.2, 0.05), bite: (0.6, 0.3), tutti: ONCE, ..QUIET },
        ..E
    },
    // Two, an octave apart.
    Section {
        chords: "Cm C#5 Cm G5", kick: KICK, bass: "0...0...0...0...", hat: TICKS, stab: "x...............", pad: 0, energy: 0.8, rumble: (0.4, 0.55),
        lead: MANY_THEME,
        parts: &[line(Riff, 0.36, 0.0, MANY_DOWN_ONE), line(Hum, 0.16, 0.0, "C2:192"), SLAM_2],
        mass: MassCue { level: (0.26, 0.45), lo: (33.0, 30.0), hi: (67.0, 76.0), grit: (0.05, 0.12), bite: (0.32, 0.5), tutti: EVERY_4, ..QUIET },
        ..E
    },
    // Four, and now you can't tell which one you were.
    Section {
        chords: "Cm C#5 Cm G5", kick: KICK, bass: ".000.000.000.000", hat: TICKS_FINE, stab: "x..x..x.........", tom: WAR, pad: 0, energy: 0.9, rumble: (0.55, 0.75), pump: 0.65,
        lead: MANY_THEME,
        parts: &[line(Riff, 0.4, 0.0, MANY_DOWN_ONE), line(Riff, 0.36, 0.0, MANY_DOWN_TWO), line(Brass, 0.26, -0.3, MANY_THEME), SLAM_BAR, RIVET, CALL_C],
        mass: MassCue { level: (0.5, 0.75), lo: (30.0, 28.0), hi: (76.0, 84.0), grit: (0.12, 0.22), bite: (0.5, 0.75), tutti: EVERY_2, ..QUIET },
        ..E
    },
    // All of them, and down it goes.
    Section {
        chords: "Cm C#5 Cm G5", kick: KICK, bass: ".000.000.000.000", hat: TICKS_FINE, stab: "x..x..x.x..x..x.", tom: WAR, sub: "x...............", pad: 0, energy: 1.0, rumble: (0.8, 1.0), pump: 0.7, gasp: true, shift: -1,
        lead: MANY_THEME,
        parts: &[
            line(Riff, 0.42, 0.0, MANY_DOWN_ONE),
            line(Riff, 0.38, 0.0, MANY_DOWN_TWO),
            line(Brass, 0.28, -0.3, MANY_THEME),
            line(Brass, 0.26, 0.3, MANY_DOWN_ONE),
            SLAM_BAR,
            RIVET,
            CALL_C,
            line(Collapse, 0.4, 0.0, "C2:64"),
        ],
        mass: MassCue { level: (0.85, 1.0), lo: (28.0, 26.0), hi: (86.0, 91.0), grit: (0.25, 0.5), bite: (0.8, 0.95), tutti: EVERY_BAR, ..QUIET },
        ..E
    },
    // Nobody is left to pick out.
    Section {
        chords: "C5 C5 C5 C5", shift: -1,
        parts: &[line(Hull, 0.5, 0.0, "C2:64"), line(Collapse, 0.6, 0.0, "C2:64"), line(Hum, 0.16, 0.0, "C2:64")],
        ..COLLAPSE
    },
];

// SMITHERINES. Everything goes white, and your ears ring, and the world comes back piece by piece.
const SMITH_THEME: &str = "B5:4 E6:4 G6:8 F#6:4 E6:4 B5:8 G5:4 B5:4 E6:8 B5:8 G5:8 C6:4 E6:4 G6:8 E6:8 D#6:4 B5:4 F#5:8 B5:16 -:8";
static SMITHERINES: &[Section] = &[
    Section {
        chords: "Em Em C5 B5", kick: KICK, bass: "0...0...0...0...", hat: TICKS, stab: "x..x..x.........",
        energy: 0.9, pad: 0, repeats: 2, dark: (0.14, 1.0), rumble: (0.7, 0.7), pump: 0.65,
        lead: SMITH_THEME,
        parts: &[
            line(Sub, 0.85, 0.0, "C2:256"),
            line(Collapse, 0.75, 0.0, "C2:256"),
            line(Hull, 0.55, 0.0, "E2:256"),
            line(Ring, 0.1, 0.0, "E7:64 -:192"),
            SLAM_BAR,
        ],
        mass: MassCue { level: (0.85, 0.7), lo: (26.0, 28.0), hi: (86.0, 84.0), grit: (0.5, 0.2), bite: (0.5, 0.8), tutti: ONCE, ..QUIET },
        ..E
    },
    Section {
        chords: "C5 B5 Em Em", kick: KICK, bass: ".000.000.000.000", hat: TICKS_FINE, tom: WAR, sub: "x...............",
        brass: "E4:16 D#4:16 E4:16 B4:16", energy: 1.0, fill: true, pad: 0, rumble: (0.75, 0.9), pump: 0.65, repeats: 2,
        parts: &[CALL_E, SLAM_BAR, RIVET],
        mass: MassCue { level: (0.8, 0.95), lo: (28.0, 28.0), hi: (84.0, 88.0), grit: (0.2, 0.3), bite: (0.8, 0.9), tutti: EVERY_BAR, ..QUIET },
        ..E
    },
    Section {
        chords: "Em Em C5 B5", kick: KICK, bass: ".000.000.000.000", hat: TICKS_FINE, stab: "x..x..x.x..x..x.", tom: WAR, energy: 1.0, pad: 0, rumble: (0.9, 1.0), pump: 0.7, shift: -1,
        lead: SMITH_THEME,
        parts: &[SLAM_BAR, RIVET, CALL_E, line(Collapse, 0.4, 0.0, "C2:64")],
        mass: MassCue { level: (0.95, 1.0), lo: (28.0, 26.0), hi: (88.0, 91.0), grit: (0.3, 0.5), bite: (0.9, 0.98), tutti: EVERY_BAR, ..QUIET },
        ..E
    },
    // And then, a long way off, that ringing again.
    Section {
        chords: "E5 E5 E5 E5", shift: -1,
        parts: &[line(Ring, 0.05, 0.0, "E7:64"), line(Collapse, 0.5, 0.0, "C2:64"), line(Hum, 0.16, 0.0, "E2:64")],
        ..COLLAPSE
    },
];

// REST WELL, FOR IT IS OVER. Nothing left to fight. Wind over what used to be somewhere, and a heart
// that hasn't been told yet.
static REST_WELL: &[Section] = &[
    Section {
        chords: "F C Dm Bb", bass: "0...............", pad: 1, energy: 0.45, dark: (0.55, 0.7), repeats: 2,
        lead: "A4:8 C5:8 G4:8 C5:8 F4:8 A4:8 F4:8 D5:8",
        parts: &[
            line(Wind, 0.4, -0.3, "F4:128"),
            hit(Heart, "C2", 0.36, 0.0, "x...x...x...x..."),
            line(Geiger, 0.15, 0.35, "C1:128"),
        ],
        mass: MassCue { level: (0.06, 0.13), lo: (40.0, 36.0), hi: (62.0, 70.0), bite: (0.15, 0.22), ..QUIET },
        ..E
    },
    Section {
        chords: "F C Bb F", bass: "0.......5.......", arp: "0...1...2...1...", pad: 1, energy: 0.55, dark: (0.7, 0.78), repeats: 2,
        lead: "C5:8 F5:8 E5:8 C5:8 D5:8 A#4:8 A4:16",
        parts: &[line(Wind, 0.4, 0.3, "F4:128"), hit(Heart, "C2", 0.32, 0.0, "x.......x.......")],
        mass: MassCue { level: (0.13, 0.22), lo: (36.0, 33.0), hi: (70.0, 76.0), bite: (0.22, 0.32), ..QUIET },
        ..E
    },
    Section {
        chords: "Bb F C F", bass: "0...............", pad: 1, energy: 0.4, dark: (0.72, 0.55), repeats: 2,
        lead: "D5:16 C5:16 C5:16 A4:16",
        parts: &[
            line(Wind, 0.42, -0.3, "F4:128"),
            hit(Heart, "C2", 0.28, 0.0, "x..............................."),
            line(Hull, 0.2, 0.0, "F2:128"),
        ],
        mass: MassCue { level: (0.2, 0.08), lo: (33.0, 36.0), hi: (76.0, 67.0), bite: (0.28, 0.16), ..QUIET },
        ..E
    },
    // The last beat, somewhere in here, and no announcement of which one it was.
    Section {
        chords: "F F F F", pad: 1, energy: 0.3, dark: (0.5, 0.3), hall: 0.96, repeats: 2,
        parts: &[
            line(Wind, 0.5, 0.2, "F4:128"),
            hit(Heart, "C2", 0.2, 0.0, "x..............................................................................................................................."),
            line(Hull, 0.12, 0.0, "F2:128"),
        ],
        mass: MassCue { level: (0.08, 0.0), lo: (36.0, 36.0), hi: (67.0, 55.0), bite: (0.16, 0.08), ..QUIET },
        ..E
    },
];

pub fn song(t: Track) -> Song {
    #[allow(clippy::too_many_arguments)]
    let mk = |bpm, swing, pad_cut, bass_drive, lead_inst, arp_inst, low, loop_to, hall, sections| Song {
        bpm, swing, pad_cut, bass_drive, lead_inst, arp_inst, low, loop_to, hall, sections,
    };
    match t {
        Track::Menu => mk(66.0, 0.0, 900.0, 1.0, Bell, Pluck, 0.0, 0, 0.9, MENU),
        Track::Build => mk(96.0, 0.08, 1500.0, 1.2, Pluck, Pluck, 0.0, 1, 0.84, BUILD),
        // The night: slower than it has any right to be, and two octaves further down.
        Track::CombatA => mk(100.0, 0.0, 1300.0, 2.2, Riff, Arp, -24.0, 1, 0.88, COMBAT_A),
        Track::CombatB => mk(112.0, 0.0, 1300.0, 2.0, Riff, Arp, -24.0, 1, 0.88, COMBAT_B),
        Track::CombatC => mk(88.0, 0.0, 1200.0, 2.6, Riff, Arp, -24.0, 1, 0.88, COMBAT_C),
        Track::Boss => mk(76.0, 0.0, 1100.0, 3.0, Riff, Arp, -24.0, 2, 0.9, BOSS),
        Track::GameOver => mk(58.0, 0.0, 700.0, 1.0, Bell, Pluck, 0.0, 0, 0.92, GAMEOVER),
        Track::Rot => mk(60.0, 0.0, 700.0, 2.0, Bell, Pluck, 0.0, 0, 0.9, ROT),
        Track::OneOfMany => mk(72.0, 0.0, 800.0, 3.0, Bell, Pluck, 0.0, 0, 0.9, ONE_OF_MANY),
        Track::Smitherines => mk(112.0, 0.0, 1300.0, 2.6, Riff, Arp, -24.0, 0, 0.88, SMITHERINES),
        Track::RestWell => mk(50.0, 0.0, 1100.0, 1.0, Bell, Pluck, 0.0, 0, 0.93, REST_WELL),
    }
}
