//! Audio engine front-end. The audio thread owns the mixer; the game talks to it through
//! a channel, so gameplay never blocks on the sound device.
#[cfg(all(unix, not(target_arch = "wasm32")))]
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
    /// Switch track with no fade.
    MusicCut(Track),
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
            Cmd::MusicCut(t) => self.music.cut_to(t),
            Cmd::Intensity(i) => self.music.set_intensity(i),
            Cmd::MusicVolume(v) => self.music.volume = v,
            Cmd::SfxVolume(v) => self.sfx_volume = v,
        }
    }

    /// A mixer for rendering music alone, without the cost of making the sound effects.
    pub fn music_only(track: Track) -> Self {
        Mixer {
            bank: Vec::new(),
            voices: Vec::new(),
            music: music::Music::new(track),
            sfx_volume: 0.0,
            env: 1.0,
            peak: 0.0,
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

    /// Browser audio: the mixer runs in Web Audio's callback on the main thread, since there
    /// are no threads to give it. Browsers keep sound off until the player first interacts.
    #[cfg(target_arch = "wasm32")]
    pub fn start() -> Self {
        let (tx, rx) = channel();
        if let Err(e) = web_audio(rx) {
            web_sys::console::warn_1(&e);
            return Audio::silent();
        }
        Audio { tx: Some(tx) }
    }

    /// Starts the audio thread; falls back to silence if the device cannot be opened.
    #[cfg(not(target_arch = "wasm32"))]
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

    /// A handle whose commands a test can read back.
    #[cfg(test)]
    pub fn capture() -> (Audio, Receiver<Cmd>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Audio { tx: Some(tx) }, rx)
    }

    pub fn send(&self, c: Cmd) {
        if let Some(t) = &self.tx {
            let _ = t.send(c);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn web_audio(rx: Receiver<Cmd>) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::{closure::Closure, JsCast};
    const FRAMES: u32 = 2048;
    let opts = web_sys::AudioContextOptions::new();
    opts.set_sample_rate(SR);
    let ctx = web_sys::AudioContext::new_with_context_options(&opts)?;
    let node = ctx.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(FRAMES, 0, 2)?;
    let mut mixer = Mixer::new(Track::Menu);
    let mut stereo = vec![0.0f32; FRAMES as usize * 2];
    let mut left = vec![0.0f32; FRAMES as usize];
    let mut right = vec![0.0f32; FRAMES as usize];
    let mut unlocked_once = false;
    let process = Closure::<dyn FnMut(web_sys::AudioProcessingEvent)>::new(move |e: web_sys::AudioProcessingEvent| {
        // Browsers keep audio suspended until the first input, so anything played before then
        // has piled up. Settings still apply, but those sounds must not all go off at once.
        while let Ok(c) = rx.try_recv() {
            if unlocked_once || !matches!(c, Cmd::Play { .. }) {
                mixer.apply(c);
            }
        }
        unlocked_once = true;
        mixer.render(&mut stereo);
        for (i, f) in stereo.chunks_exact(2).enumerate() {
            left[i] = f[0];
            right[i] = f[1];
        }
        if let Ok(out) = e.output_buffer() {
            let _ = out.copy_to_channel(&mut left, 0);
            let _ = out.copy_to_channel(&mut right, 1);
        }
    });
    node.set_onaudioprocess(Some(process.as_ref().unchecked_ref()));
    node.connect_with_audio_node(&ctx.destination())?;
    // The callback and the unlock listener live as long as the page.
    process.forget();
    let target: web_sys::EventTarget = web_sys::window().ok_or("no window")?.into();
    let unlock = Closure::<dyn FnMut()>::new(move || {
        let _ = ctx.resume();
    });
    for kind in ["pointerdown", "keydown", "touchend"] {
        target.add_event_listener_with_callback(kind, unlock.as_ref().unchecked_ref())?;
    }
    unlock.forget();
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
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

/// Renders one soundtrack file a slice at a time, so that the game stays responsive even in a
/// browser, where there are no threads to hide it on.
/// How hard each track works the CPU, played through once at full intensity. The browser renders
/// on the page's own thread in 2048 frame blocks, so its slowest block is what can cause a dropout.
pub fn bench_music() -> String {
    #[cfg(target_arch = "wasm32")]
    use web_time::Instant;
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::Instant;
    const BLOCK: usize = 2048;
    let budget = BLOCK as f64 / SR as f64 * 1000.0;
    let mut report = format!("budget per block: {budget:.1} ms\n");
    for track in Track::ALL {
        let mut mixer = Mixer::music_only(track);
        mixer.apply(Cmd::Intensity(1.0));
        let mut buf = vec![0.0f32; BLOCK * 2];
        let blocks = (track.loop_seconds() * SR / BLOCK as f32) as usize + 1;
        let (mut total, mut worst) = (0.0f64, 0.0f64);
        for _ in 0..blocks {
            let t = Instant::now();
            mixer.render(&mut buf);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            total += ms;
            worst = worst.max(ms);
        }
        let average = total / blocks as f64;
        report += &format!("{:<28} {:>4} blocks  avg {average:>5.2} ms ({:>3.0}%)  worst {worst:>5.2} ms ({:>3.0}%)\n", track.name(), blocks, average / budget * 100.0, worst / budget * 100.0);
    }
    report
}

pub struct OstRender {
    pub track: Track,
    mixer: Mixer,
    pcm: Vec<i16>,
    frames: usize,
    done: usize,
}

const OST_FADE_SECONDS: f32 = 2.5;

impl OstRender {
    pub fn new(track: Track) -> Self {
        let mut mixer = Mixer::music_only(track);
        mixer.music.set_intensity(1.0);
        mixer.music.intensity = 1.0;
        mixer.music.volume = 1.0;
        // One full pass, plus room for the last notes to ring out as it fades.
        let frames = ((track.loop_seconds() + OST_FADE_SECONDS) * SR) as usize;
        OstRender { track, mixer, pcm: Vec::with_capacity(frames * 2), frames, done: 0 }
    }

    pub fn progress(&self) -> f32 {
        self.done as f32 / self.frames.max(1) as f32
    }

    /// Renders up to `frames` more frames. Returns true when the track is complete.
    pub fn advance(&mut self, frames: usize) -> bool {
        let n = frames.min(self.frames - self.done);
        let mut buf = vec![0.0f32; n * 2];
        self.mixer.render(&mut buf);
        let fade_from = self.frames - (OST_FADE_SECONDS * SR) as usize;
        for (i, f) in buf.chunks_exact(2).enumerate() {
            let at = self.done + i;
            let k = if at > fade_from { 1.0 - (at - fade_from) as f32 / (self.frames - fade_from) as f32 } else { 1.0 };
            self.pcm.push((f[0].clamp(-1.0, 1.0) * k * 32767.0) as i16);
            self.pcm.push((f[1].clamp(-1.0, 1.0) * k * 32767.0) as i16);
        }
        self.done += n;
        self.done >= self.frames
    }

    pub fn file_name(&self) -> String {
        let n = Track::ALL.iter().position(|t| *t == self.track).unwrap_or(0) + 1;
        let clean: String = self.track.name().chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
        format!("{n:02} {clean}.wav")
    }

    pub fn wav(&self) -> Vec<u8> {
        encode_wav(&self.pcm)
    }
}

fn encode_wav(pcm: &[i16]) -> Vec<u8> {
    let n = pcm.len() * 2;
    let mut out = Vec::with_capacity(44 + n);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + n) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&(SR as u32).to_le_bytes());
    out.extend_from_slice(&((SR as u32) * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Offline rendering of the score; used by `--render-music`.
pub fn render_track_wav(track: Track, seconds: f32, intensity: f32, path: &str) -> std::io::Result<()> {
    let mut m = Mixer::music_only(track);
    m.music.set_intensity(intensity);
    m.music.intensity = intensity;
    let n = (seconds * SR) as usize * 2;
    let mut buf = vec![0.0f32; n];
    for c in buf.chunks_mut(1024) {
        m.render(c);
    }
    let pcm: Vec<i16> = buf.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
    std::fs::write(path, encode_wav(&pcm))
}

/// Writes the whole soundtrack to a folder.
pub fn export_soundtrack(dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for t in Track::ALL {
        let mut r = OstRender::new(t);
        while !r.advance(44100) {}
        let path = std::path::Path::new(dir).join(r.file_name());
        std::fs::write(&path, r.wav())?;
        println!("{} ({:.0} s)", path.display(), t.loop_seconds());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::music::*;
    use super::*;

    const TRACKS: [Track; 11] = Track::ALL;

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|x| x * x).sum::<f32>() / b.len().max(1) as f32).sqrt()
    }

    #[test]
    fn track_names_are_unique_and_loops_have_length() {
        let names: std::collections::HashSet<_> = Track::ALL.iter().map(|t| t.name()).collect();
        assert_eq!(names.len(), Track::ALL.len());
        assert_eq!(Track::Build.name(), "Daytime Escape");
        assert_eq!(Track::Menu.name(), "Descent");
        for t in Track::ALL {
            let secs = t.loop_seconds();
            assert!((8.0..300.0).contains(&secs), "{t:?} loops every {secs} s");
        }
    }

    #[test]
    fn soundtrack_files_are_valid_and_audible() {
        for t in [Track::Rot, Track::RestWell] {
            let mut r = OstRender::new(t);
            let mut steps = 0;
            while !r.advance(8192) {
                steps += 1;
                assert!(steps < 10_000);
            }
            assert!((r.progress() - 1.0).abs() < 1e-6);
            let wav = r.wav();
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            let expected = ((t.loop_seconds() + OST_FADE_SECONDS) * SR) as usize * 4;
            assert_eq!(wav.len(), 44 + expected, "{t:?}");
            let peak = wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap();
            assert!(peak > 1500, "{t:?} is nearly silent: {peak}");
            // It fades out rather than cutting off.
            let tail = &wav[wav.len() - 400..];
            let end = tail.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap();
            assert!(end < 400, "{t:?} ends abruptly: {end}");
        }
        assert_eq!(OstRender::new(Track::RestWell).file_name(), "11 Rest Well for It Is Over.wav");
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
                for tok in s.lead.split_whitespace().chain(s.counter.split_whitespace()).chain(s.brass.split_whitespace()) {
                    let n = tok.split(':').next().unwrap();
                    assert!(n == "-" || parse_note(n).is_some(), "bad note {tok} in {t:?}");
                    assert!(tok.split(':').nth(1).and_then(|v| v.parse::<u32>().ok()).is_some(), "bad len {tok}");
                }
                for p in [s.kick, s.snare, s.hat, s.ride, s.bass, s.acid, s.arp, s.low, s.tom, s.sub, s.stab] {
                    assert!(p.is_empty() || p.len() == 16, "pattern `{p}` in {t:?} is not 16 steps");
                }
            }
        }
    }

    fn lines_of(s: &Section) -> [(&'static str, &'static str); 3] {
        [("lead", s.lead), ("counter", s.counter), ("brass", s.brass)]
    }

    #[test]
    fn melodies_line_up_with_their_chords() {
        for t in TRACKS {
            for (si, s) in song(t).sections.iter().enumerate() {
                let bars = s.chords.split_whitespace().count() as u32;
                for (name, line) in lines_of(s) {
                    let total: u32 = line.split_whitespace().map(|tok| tok.split(':').nth(1).unwrap().parse::<u32>().unwrap()).sum();
                    assert_eq!(total % (bars * 16), 0, "{t:?} section {si} {name}: {total} steps over {bars} bars");
                }
            }
        }
    }

    /// Fraction of melody time on chord tones. Reports every line, and fails on real clashes.
    #[test]
    fn melodies_stay_consonant() {
        for t in TRACKS {
            let (mut chord_time, mut total_time) = (0u32, 0u32);
            for (si, s) in song(t).sections.iter().enumerate() {
                let chords: Vec<_> = s.chords.split_whitespace().map(parse_chord).collect();
                for (name, line) in lines_of(s) {
                    let (mut on, mut all, mut step) = (0, 0, 0u32);
                    for tok in line.split_whitespace() {
                        let mut it = tok.split(':');
                        let (n, l) = (it.next().unwrap(), it.next().unwrap().parse::<u32>().unwrap());
                        if let Some(note) = parse_note(n) {
                            let (root, tones) = chords[((step / 16) as usize) % chords.len()];
                            let pc = (note - root).rem_euclid(12.0);
                            all += l;
                            if tones.iter().any(|x| (x.rem_euclid(12.0) - pc).abs() < 0.01) {
                                on += l;
                            }
                        }
                        step += l;
                    }
                    chord_time += on;
                    total_time += all;
                    // Support lines exist to hold the harmony together, so they are held to more.
                    if name != "lead" && all > 0 {
                        assert!(on * 10 >= all * 8, "{t:?} section {si} {name} is only {on}/{all} on chord tones");
                    }
                }
            }
            let ratio = chord_time as f32 / total_time.max(1) as f32;
            println!("{t:?}: {:.0}% of melody time on chord tones", ratio * 100.0);
            assert!(ratio > 0.45, "{t:?} melody clashes with its harmony: {ratio}");
        }
    }

    #[test]
    fn every_section_sits_in_a_song_that_loops_cleanly() {
        for t in TRACKS {
            let s = song(t);
            assert!(s.loop_to < s.sections.len(), "{t:?}");
            for sec in s.sections {
                assert!(!sec.chords.is_empty() && sec.repeats > 0 && (0.0..=1.0).contains(&sec.energy), "{t:?}");
            }
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
    fn death_music_and_previews_cut_in_at_once() {
        let mut m = Music::new(Track::Boss);
        let mut b = vec![0.0; 88200];
        m.render_add(&mut b);
        let mut after = vec![0.0; 2 * 512];
        // However it is asked for, a death track takes over within a dozen milliseconds.
        m.set_track(Track::Rot);
        m.render_add(&mut after);
        assert_eq!(m.track(), Track::Rot);
        assert!(!m.is_crossfading(), "nothing of the old track may linger");
        assert!(after.iter().all(|x| x.is_finite() && x.abs() < 2.0));
        // Asking again, as the game does every frame, must not restart it.
        let before = m.position();
        m.set_track(Track::Rot);
        m.render_add(&mut b);
        assert!(m.position() > before, "restarted");
        // Leaving a death track still fades, as nobody asked for that to be sudden.
        m.set_track(Track::Build);
        m.render_add(&mut after);
        assert!(m.is_crossfading());
        assert_eq!(m.track(), Track::Build);
        // The soundtrack page cuts between anything, in any direction.
        m.cut_to(Track::CombatA);
        m.render_add(&mut after);
        assert_eq!(m.track(), Track::CombatA);
        assert!(!m.is_crossfading());
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
    fn menu_sounds_stay_under_the_music() {
        let bank = sfx::build_bank();
        let check = |id: Sfx, max_secs: f32, max_peak: f32| {
            let v = &bank[id as usize];
            let peak = v.iter().fold(0.0f32, |p, x| p.max(x.abs()));
            let secs = v.len() as f32 / dsp::SR;
            println!("{id:?}: {secs:.2} s, peak {peak:.2}, rms {:.3}", rms(v));
            assert!(secs <= max_secs && peak <= max_peak, "{id:?}: {secs} s, peak {peak}");
            // No click at the end of the sound.
            assert!(v[v.len() - 1].abs() < 0.05, "{id:?} ends abruptly");
        };
        // Passed over constantly, so it has to be barely there.
        check(Sfx::MenuHover, 0.1, 0.35);
        check(Sfx::MenuTick, 0.1, 0.5);
        check(Sfx::MenuToggle, 0.2, 0.8);
        check(Sfx::MenuSelect, 0.2, 0.7);
        check(Sfx::MenuBack, 0.2, 0.7);
        check(Sfx::MenuError, 0.4, 0.95);
        check(Sfx::MenuConfirm, 2.0, 1.0);
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
