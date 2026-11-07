//! Audio engine front-end. The audio thread owns the mixer; the game talks to it through
//! a channel, so gameplay never blocks on the sound device.
#[cfg(all(unix, not(target_arch = "wasm32")))]
#[path = "alsa.rs"]
mod backend;
#[cfg(windows)]
#[path = "winmm.rs"]
mod backend;
pub mod dsp;
mod mass;
pub mod music;
mod score;
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
        // A wall of sound is no use if it hides the zombie behind you, so it makes room for gunfire,
        // groans and footsteps. A few decibels are enough, as they are all in the same range.
        let load: f32 = self.voices.iter().map(|v| v.gl + v.gr).sum::<f32>() * self.sfx_volume;
        self.music.room_wanted = (1.0 / (1.0 + load * 0.22)).max(0.62);
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
        let (mut total, mut worst, mut crowd) = (0.0f64, 0.0f64, 0);
        for _ in 0..blocks {
            let t = Instant::now();
            mixer.render(&mut buf);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            total += ms;
            worst = worst.max(ms);
            crowd = crowd.max(mixer.music.voices_playing());
        }
        let average = total / blocks as f64;
        report += &format!("{:<28} {:>4} blocks  avg {average:>5.2} ms ({:>3.0}%)  worst {worst:>5.2} ms ({:>3.0}%)  {crowd:>3} voices\n", track.name(), blocks, average / budget * 100.0, worst / budget * 100.0);
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

/// Finds a track by the start of its name, or by the short names `--render-music` has always used.
pub fn find_track(name: &str) -> Option<Track> {
    match name {
        "menu" => return Some(Track::Menu),
        "build" => return Some(Track::Build),
        "a" => return Some(Track::CombatA),
        "b" => return Some(Track::CombatB),
        "c" => return Some(Track::CombatC),
        "boss" => return Some(Track::Boss),
        "over" => return Some(Track::GameOver),
        _ => {}
    }
    let name = name.to_lowercase();
    Track::ALL.iter().copied().find(|t| t.name().to_lowercase().starts_with(&name))
}

/// One pass through a track, one file per mix layer, for checking how the mix sits together.
pub fn export_stems(track: Track, dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (bus, name) in music::BUS_NAMES.iter().enumerate() {
        let mut m = Mixer::music_only(track);
        m.music.set_intensity(1.0);
        m.music.intensity = 1.0;
        m.music.volume = 1.0;
        m.music.solo = Some(bus);
        let mut buf = vec![0.0f32; (track.loop_seconds() * SR) as usize * 2];
        for c in buf.chunks_mut(1024) {
            m.render(c);
        }
        let pcm: Vec<i16> = buf.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
        std::fs::write(std::path::Path::new(dir).join(format!("{name}.wav")), encode_wav(&pcm))?;
    }
    Ok(())
}

/// Writes down every note of the soundtrack as text, one file per track.
pub fn dump_soundtrack(dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for t in Track::ALL {
        let (notes, marks) = music::dump_score(t);
        let mut out = String::new();
        for (time, section, chords) in marks {
            out += &format!("s,{time:.3},{section},{chords}\n");
        }
        for n in notes {
            out += &format!("n,{:.3},{:?},{:.2},{:.2},{:.3},{:.3},{:.2}\n", n.time, n.inst, n.note, n.to, n.vel, n.gate, n.pan);
        }
        std::fs::write(std::path::Path::new(dir).join(format!("{}.csv", t.name().split(',').next().unwrap_or(""))), out)?;
    }
    Ok(())
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
            assert!((8.0..480.0).contains(&secs), "{t:?} loops every {secs} s");
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
                for p in [s.kick, s.snare, s.hat, s.ride, s.bass, s.arp, s.low, s.tom, s.sub, s.stab] {
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
                            // A bare fifth doesn't say whether it is major or minor, so a tune can.
                            let open = tones[1] == 12.0 && (pc == 3.0 || pc == 4.0);
                            if open || tones.iter().any(|x| (x.rem_euclid(12.0) - pc).abs() < 0.01) {
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

    /// Everything written in `Note:length` form has to fit its section, and a typo would otherwise
    /// just play quietly wrong forever.
    #[test]
    fn every_part_parses_and_lines_up() {
        for t in TRACKS {
            for (si, s) in song(t).sections.iter().enumerate() {
                let len = parse_section(s).len;
                for (pi, part) in s.kit.iter().chain(s.parts).enumerate() {
                    match part.score {
                        Score::Line(text) => {
                            let mut total = 0;
                            for tok in text.split_whitespace() {
                                let (notes, l) = tok.split_once(':').unwrap_or_else(|| panic!("{t:?} section {si} part {pi}: `{tok}` has no length"));
                                total += l.parse::<u32>().unwrap_or_else(|_| panic!("{t:?} section {si} part {pi}: bad length in `{tok}`"));
                                if notes != "-" {
                                    for n in notes.split('+').flat_map(|n| n.split('>')) {
                                        assert!(parse_note(n).is_some(), "{t:?} section {si} part {pi}: bad note `{n}`");
                                    }
                                }
                            }
                            assert!(total > 0);
                            let whole = len * s.repeats;
                            assert!(total % len == 0 || len % total == 0, "{t:?} section {si} part {pi}: {total} steps in a section of {len}");
                            assert!(total <= whole || total % whole == 0, "{t:?} section {si} part {pi}: {total} steps, but only {whole} to play them in");
                        }
                        Score::Hits(note, pattern) => {
                            assert!(parse_note(note).is_some() && !pattern.is_empty(), "{t:?} section {si} part {pi}");
                            assert!(pattern.len() % 16 == 0, "{t:?} section {si} part {pi}: `{pattern}` doesn't fit the bar");
                        }
                    }
                }
            }
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

    /// The things that make it this soundtrack and not some other one.
    #[test]
    fn the_album_keeps_its_rules() {
        for t in TRACKS {
            let (notes, marks) = dump_score(t);
            // ROT, FALL: every song has the call sign, except the one where it is all over.
            assert_eq!(notes.iter().any(|n| n.inst == Inst::Horn), t != Track::RestWell, "{t:?}");
            for s in song(t).sections {
                // The world only ever sinks.
                assert!(s.shift <= 0, "{t:?} has a key change going up");
            }
            // The orchestra is somewhere in every song, and there is more than one section to each.
            assert!(song(t).sections.iter().any(|s| s.mass.level.0.max(s.mass.level.1) > 0.05), "{t:?}");
            assert!(marks.len() >= 4, "{t:?} has only {} sections", marks.len());
        }
        // The night is slow, low and heavy. Nothing about it should want to dance.
        for t in [Track::CombatA, Track::CombatB, Track::CombatC, Track::Boss, Track::Smitherines] {
            let s = song(t);
            assert!(s.bpm <= 112.0 && s.low <= -24.0, "{t:?}");
            assert_eq!(s.lead_inst, Inst::Riff, "{t:?}");
            for sec in s.sections {
                // No hat or vent leaning on the offbeat without one on the beat, which is what makes a
                // groove bounce.
                let hat = sec.hat.as_bytes();
                if hat.len() == 16 {
                    assert!(hat[0] != b'.', "{t:?} has a bouncing hat: {}", sec.hat);
                }
            }
        }
    }

    /// Nothing may wobble. A note has to stay where it was put, from its first moment to its last.
    #[test]
    fn pitched_voices_do_not_warble() {
        let f0 = dsp::midi_hz(57.0);
        // The fundamental's phase in a stretch of sound. How fast it turns is the pitch actually heard.
        let phase_at = |x: &[f32]| -> f32 {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, v) in x.iter().enumerate() {
                let a = std::f32::consts::TAU * f0 * i as f32 / SR;
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / x.len() as f32).cos();
                re += v * w * a.cos();
                im -= v * w * a.sin();
            }
            im.atan2(re)
        };
        for inst in [Inst::Pad, Inst::SynthStrings, Inst::Brass, Inst::Riff, Inst::Hum, Inst::Horn, Inst::Bass, Inst::Ring] {
            let x = music::render_note(inst, 57.0, 3.0);
            let (hop, size) = (2205, 8820);
            let mut last = None;
            for w in 0..((x.len() - 22050 - size) / hop) {
                let at = 22050 + w * hop;
                let phase = phase_at(&x[at..at + size]);
                // Each window starts `hop` samples later, so a perfectly steady note turns by exactly this.
                let turn = std::f32::consts::TAU * f0 * at as f32 / SR;
                let drift = phase + turn;
                if let Some(before) = last {
                    let mut change: f32 = drift - before;
                    change = (change + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                    let hz = change / std::f32::consts::TAU * SR / hop as f32;
                    // Six cents is about as far as a note can move before it starts to sound like it moved.
                    assert!(hz.abs() < f0 * 0.0035, "{inst:?} is moving by {hz:.2} Hz around {f0:.0} Hz");
                }
                last = Some(drift);
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
    fn the_music_makes_room_for_the_game() {
        let mut m = Mixer::new(Track::Boss);
        let mut b = vec![0.0; 2048];
        m.render(&mut b);
        assert_eq!(m.music.room_wanted, 1.0, "nothing to make room for yet");
        for _ in 0..6 {
            m.apply(Cmd::Play { id: Sfx::Pistol, gain: 0.8, pan: 0.0, pitch: 1.0 });
        }
        m.render(&mut b);
        assert!(m.music.room_wanted < 0.8, "six gunshots should move it: {}", m.music.room_wanted);
        // Never by so much that the fight loses its score.
        for _ in 0..60 {
            m.apply(Cmd::Play { id: Sfx::ExplosionBig, gain: 2.0, pan: 0.0, pitch: 1.0 });
        }
        m.render(&mut b);
        assert!(m.music.room_wanted >= 0.62);
        // And all the way back once the sounds have finished.
        for _ in 0..600 {
            m.render(&mut b);
        }
        assert_eq!(m.music.room_wanted, 1.0);
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
    fn songs_pick_up_where_they_left_off() {
        let mut m = Music::new(Track::CombatA);
        let mut b = vec![0.0; 44100 * 2 * 12];
        m.render_add(&mut b);
        let reached = m.section();
        assert!(reached >= 1, "twelve seconds should be past the intro");
        // Off to build for a while, and back: the next section, not the intro again.
        m.set_track(Track::Build);
        m.render_add(&mut b);
        m.set_track(Track::CombatA);
        assert_eq!(m.section(), reached + 1);
        // A death track always starts with its blow, and doesn't lose the fight's place.
        m.set_track(Track::Rot);
        m.render_add(&mut b[..1024]);
        assert_eq!(m.track(), Track::Rot);
        assert_eq!(m.section(), 0);
        m.set_track(Track::CombatA);
        assert_eq!(m.section(), reached + 2);
        // The title screen's own song is an overture, and is always heard from the start.
        let mut m = Music::new(Track::Menu);
        m.render_add(&mut b);
        m.render_add(&mut b);
        m.render_add(&mut b);
        assert!(m.section() >= 1);
        m.set_track(Track::Build);
        m.render_add(&mut b[..1024]);
        m.set_track(Track::Menu);
        assert_eq!(m.section(), 0);
        // And a new run starts everything over, wherever the last one got to.
        m.set_track(Track::CombatA);
        m.render_add(&mut b);
        m.set_track(Track::Menu);
        m.set_track(Track::CombatA);
        assert_eq!(m.section(), 0);
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
