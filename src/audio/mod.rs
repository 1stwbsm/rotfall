//! Audio engine front-end. The audio thread owns the mixer; the game talks to it through
//! a channel, so gameplay never blocks on the sound device.
#[cfg(unix)]
#[path = "alsa.rs"]
mod backend;
#[cfg(windows)]
#[path = "winmm.rs"]
mod backend;
pub mod dsp;
pub mod music;
pub mod sfx;

use dsp::*;
pub use music::Track;
pub use sfx::Sfx;
use std::sync::mpsc::{channel, Receiver, Sender};

pub enum Cmd {
    Play { id: Sfx, gain: f32, pan: f32, pitch: f32 },
    Music(Track),
    Intensity(f32),
    MusicVolume(f32),
    SfxVolume(f32),
}

#[derive(Clone, Copy)]
struct SfxVoice {
    id: usize,
    pos: f32,
    rate: f32,
    gl: f32,
    gr: f32,
}

const MAX_VOICES: usize = 72;
const MAX_SAME: usize = 7;

pub struct Mixer {
    bank: Vec<Vec<f32>>,
    voices: Vec<SfxVoice>,
    pub music: music::Music,
    sfx_volume: f32,
    env: f32,
    pub peak: f32,
}

impl Mixer {
    pub fn new(track: Track) -> Self {
        Mixer {
            bank: sfx::build_bank(),
            voices: Vec::with_capacity(MAX_VOICES),
            music: music::Music::new(track),
            sfx_volume: 0.9,
            env: 1.0,
            peak: 0.0,
        }
    }

    pub fn apply(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Play { id, gain, pan, pitch } => {
                let id = id as usize;
                let same = self.voices.iter().filter(|v| v.id == id).count();
                if same >= MAX_SAME {
                    // Cut the oldest instance of this sound so rapid fire stays punchy.
                    if let Some((i, _)) = self
                        .voices
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| v.id == id)
                        .max_by(|a, b| a.1.pos.total_cmp(&b.1.pos))
                    {
                        self.voices.swap_remove(i);
                    }
                }
                if self.voices.len() >= MAX_VOICES {
                    if let Some((i, _)) = self.voices.iter().enumerate().min_by(|a, b| {
                        (a.1.gl + a.1.gr).total_cmp(&(b.1.gl + b.1.gr))
                    }) {
                        self.voices.swap_remove(i);
                    }
                }
                let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                self.voices.push(SfxVoice { id, pos: 0.0, rate: pitch, gl: gain * a.cos(), gr: gain * a.sin() });
            }
            Cmd::Music(t) => self.music.set_track(t),
            Cmd::Intensity(i) => self.music.set_intensity(i),
            Cmd::MusicVolume(v) => self.music.volume = v,
            Cmd::SfxVolume(v) => self.sfx_volume = v,
        }
    }

    /// Render interleaved stereo; output is limited to [-1, 1].
    pub fn render(&mut self, out: &mut [f32]) {
        out.iter_mut().for_each(|s| *s = 0.0);
        self.music.render_add(out);
        let vol = self.sfx_volume;
        let bank = &self.bank;
        self.voices.retain_mut(|v| {
            let data = &bank[v.id];
            for frame in out.chunks_exact_mut(2) {
                let i = v.pos as usize;
                if i + 1 >= data.len() {
                    return false;
                }
                let fr = v.pos - i as f32;
                let s = data[i] + (data[i + 1] - data[i]) * fr;
                frame[0] += s * v.gl * vol;
                frame[1] += s * v.gr * vol;
                v.pos += v.rate;
            }
            true
        });
        // Master bus: look-at-the-peak gain riding, followed by a soft clip safety.
        for frame in out.chunks_exact_mut(2) {
            let p = frame[0].abs().max(frame[1].abs());
            let wanted = if p > 0.85 { 0.85 / p } else { 1.0 };
            if wanted < self.env {
                self.env += (wanted - self.env) * 0.25;
            } else {
                self.env += (1.0 - self.env) * 0.00012;
            }
            frame[0] = soft_clip(frame[0] * self.env * 1.05);
            frame[1] = soft_clip(frame[1] * self.env * 1.05);
            self.peak = self.peak.max(frame[0].abs()).max(frame[1].abs());
        }
    }
}

/// Handle owned by the game.
#[derive(Clone)]
pub struct Audio {
    tx: Option<Sender<Cmd>>,
}

impl Audio {
    pub fn silent() -> Self {
        Audio { tx: None }
    }

    /// Starts the audio thread; falls back to silence if the device cannot be opened.
    pub fn start() -> Self {
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new().name("audio".into()).spawn(move || audio_thread(rx));
        match spawned {
            Ok(_) => Audio { tx: Some(tx) },
            Err(e) => {
                eprintln!("audio: thread failed to start: {e}");
                Audio::silent()
            }
        }
    }

    pub fn send(&self, c: Cmd) {
        if let Some(t) = &self.tx {
            let _ = t.send(c);
        }
    }
}

fn audio_thread(rx: Receiver<Cmd>) {
    const RATE: u32 = SR as u32;
    let mut pcm = match backend::Pcm::open(RATE, 45_000) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("audio: {e}; continuing without sound");
            return;
        }
    };
    let mut mixer = Mixer::new(Track::Menu);
    let mut fbuf = vec![0.0f32; 512 * 2];
    let mut ibuf = vec![0i16; 512 * 2];
    loop {
        loop {
            match rx.try_recv() {
                Ok(c) => mixer.apply(c),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
        mixer.render(&mut fbuf);
        for (d, s) in ibuf.iter_mut().zip(&fbuf) {
            *d = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        }
        pcm.write(&ibuf);
    }
}

/// Offline rendering of the score; used by `--render-music` and the tests.
pub fn render_track_wav(track: Track, seconds: f32, intensity: f32, path: &str) -> std::io::Result<()> {
    let mut m = Mixer::new(track);
    m.music.set_intensity(intensity);
    m.music.intensity = intensity;
    let n = (seconds * SR) as usize * 2;
    let mut buf = vec![0.0f32; n];
    for c in buf.chunks_mut(1024) {
        m.render(c);
    }
    let mut out = Vec::with_capacity(44 + n * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + n * 2) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&(SR as u32).to_le_bytes());
    out.extend_from_slice(&((SR as u32) * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((n * 2) as u32).to_le_bytes());
    for s in buf {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::music::*;
    use super::*;

    const TRACKS: [Track; 7] =
        [Track::Menu, Track::Build, Track::CombatA, Track::CombatB, Track::CombatC, Track::Boss, Track::GameOver];

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|x| x * x).sum::<f32>() / b.len().max(1) as f32).sqrt()
    }

    #[test]
    fn note_parser() {
        assert_eq!(parse_note("A4"), Some(69.0));
        assert_eq!(parse_note("C#5"), Some(73.0));
        assert_eq!(parse_note("Bb3"), Some(58.0));
        assert_eq!(parse_note("-"), None);
    }

    #[test]
    fn every_melody_note_parses() {
        for t in TRACKS {
            for s in song(t).sections {
                for tok in s.lead.split_whitespace() {
                    let n = tok.split(':').next().unwrap();
                    assert!(n == "-" || parse_note(n).is_some(), "bad note {tok} in {t:?}");
                    assert!(tok.split(':').nth(1).and_then(|v| v.parse::<u32>().ok()).is_some(), "bad len {tok}");
                }
                for p in [s.kick, s.snare, s.hat, s.bass, s.arp, s.stab] {
                    assert!(p.is_empty() || p.len() == 16, "pattern `{p}` in {t:?} is not 16 steps");
                }
            }
        }
    }

    #[test]
    fn melodies_line_up_with_their_chords() {
        for t in TRACKS {
            for (si, s) in song(t).sections.iter().enumerate() {
                let bars = s.chords.split_whitespace().count() as u32;
                let total: u32 = s.lead.split_whitespace().map(|tok| tok.split(':').nth(1).unwrap().parse::<u32>().unwrap()).sum();
                assert_eq!(total % (bars * 16), 0, "{t:?} section {si}: melody is {total} steps over {bars} bars");
            }
        }
    }

    /// Fraction of melody time spent on chord tones or the song's scale; prints a report.
    #[test]
    fn melodies_stay_consonant() {
        for t in TRACKS {
            let (mut chord_time, mut total_time) = (0u32, 0u32);
            for s in song(t).sections {
                let chords: Vec<_> = s.chords.split_whitespace().map(parse_chord).collect();
                let mut step = 0u32;
                for tok in s.lead.split_whitespace() {
                    let mut it = tok.split(':');
                    let (n, l) = (it.next().unwrap(), it.next().unwrap().parse::<u32>().unwrap());
                    if let Some(note) = parse_note(n) {
                        let (root, tones) = chords[((step / 16) as usize) % chords.len()];
                        let pc = (note - root).rem_euclid(12.0);
                        total_time += l;
                        if tones.iter().any(|x| (x.rem_euclid(12.0) - pc).abs() < 0.01) {
                            chord_time += l;
                        }
                    }
                    step += l;
                }
            }
            let ratio = chord_time as f32 / total_time.max(1) as f32;
            println!("{t:?}: {:.0}% of melody time on chord tones", ratio * 100.0);
            assert!(ratio > 0.45, "{t:?} melody clashes with its harmony: {ratio}");
        }
    }

    #[test]
    fn soundtrack_renders_faster_than_real_time() {
        let mut m = Mixer::new(Track::Boss);
        m.music.set_intensity(1.0);
        m.music.intensity = 1.0;
        let secs = 20;
        let mut b = vec![0.0; 1024];
        let start = std::time::Instant::now();
        for _ in 0..(secs * 44100 * 2 / 1024) {
            m.render(&mut b);
        }
        let took = start.elapsed().as_secs_f32();
        println!("20 s of boss music took {took:.2} s ({:.1}x real time)", secs as f32 / took);
        assert!(took < secs as f32 * 0.5, "audio thread too slow: {took}s");
    }

    #[test]
    fn every_track_is_audible_and_limited() {
        for t in TRACKS {
            let mut m = Music::new(t);
            m.set_intensity(1.0);
            m.intensity = 1.0;
            let mut b = vec![0.0; 44100 * 2 * 8];
            m.render_add(&mut b);
            assert!(b.iter().all(|x| x.is_finite()), "{t:?} produced NaN");
            let level = rms(&b);
            assert!(level > 0.01, "{t:?} too quiet: {level}");
            assert!(level < 0.7, "{t:?} too loud: {level}");
        }
    }

    #[test]
    fn mixer_output_stays_in_range_under_sfx_abuse() {
        let mut m = Mixer::new(Track::Boss);
        for i in 0..200 {
            m.apply(Cmd::Play { id: Sfx::ExplosionBig, gain: 2.0, pan: (i as f32).sin(), pitch: 1.0 });
            m.apply(Cmd::Play { id: Sfx::Shotgun, gain: 2.0, pan: 0.0, pitch: 1.2 });
        }
        let mut b = vec![0.0; 2048];
        for _ in 0..200 {
            m.render(&mut b);
            assert!(b.iter().all(|x| x.abs() <= 1.0 && x.is_finite()));
        }
        assert!(m.voices.len() <= MAX_VOICES);
    }

    #[test]
    fn crossfade_between_tracks_is_clean() {
        let mut m = Music::new(Track::Build);
        let mut b = vec![0.0; 44100];
        m.render_add(&mut b);
        m.set_track(Track::Boss);
        for _ in 0..8 {
            m.render_add(&mut b);
            assert!(b.iter().all(|x| x.is_finite()));
        }
        assert_eq!(m.track(), Track::Boss);
    }

    #[test]
    fn all_sfx_generated_nonempty_and_finite() {
        let bank = sfx::build_bank();
        assert_eq!(bank.len(), sfx::SFX_COUNT);
        for (i, s) in bank.iter().enumerate() {
            assert!(!s.is_empty(), "sfx {i} empty");
            assert!(s.iter().all(|x| x.is_finite()), "sfx {i} has NaN");
            assert!(rms(s) > 0.003, "sfx {i} silent");
        }
    }
}
