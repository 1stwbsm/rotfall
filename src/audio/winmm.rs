//! Windows playback through winmm's waveOut API, which every version of Windows has.
use std::ffi::c_void;

type Handle = *mut c_void;

#[repr(C)]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    cb_size: u16,
}

#[repr(C)]
struct WaveHdr {
    data: *mut u8,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHdr,
    reserved: usize,
}

const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
const WAVE_FORMAT_PCM: u16 = 1;
const CALLBACK_EVENT: u32 = 0x0005_0000;
const WHDR_DONE: u32 = 1;
const WHDR_PREPARED: u32 = 2;

#[link(name = "winmm")]
unsafe extern "system" {
    fn waveOutOpen(h: *mut Handle, device: u32, fmt: *const WaveFormatEx, cb: usize, inst: usize, flags: u32) -> u32;
    fn waveOutPrepareHeader(h: Handle, hdr: *mut WaveHdr, size: u32) -> u32;
    fn waveOutUnprepareHeader(h: Handle, hdr: *mut WaveHdr, size: u32) -> u32;
    fn waveOutWrite(h: Handle, hdr: *mut WaveHdr, size: u32) -> u32;
    fn waveOutReset(h: Handle) -> u32;
    fn waveOutClose(h: Handle) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateEventW(attrs: *mut c_void, manual: i32, initial: i32, name: *const u16) -> Handle;
    fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
    fn CloseHandle(h: Handle) -> i32;
}

/// Buffers queued with the sound card. More is safer against stutter, fewer is more responsive.
const BUFFERS: usize = 4;

pub struct Pcm {
    wave: Handle,
    event: Handle,
    // Boxed so that their addresses stay put while the driver holds them.
    headers: Vec<Box<WaveHdr>>,
    data: Vec<Vec<i16>>,
    next: usize,
}

// The handles are only used from the audio thread once moved there.
unsafe impl Send for Pcm {}

impl Pcm {
    pub fn open(rate: u32, _latency_us: u32) -> Result<Self, String> {
        let fmt = WaveFormatEx {
            format_tag: WAVE_FORMAT_PCM,
            channels: 2,
            samples_per_sec: rate,
            avg_bytes_per_sec: rate * 4,
            block_align: 4,
            bits_per_sample: 16,
            cb_size: 0,
        };
        unsafe {
            let event = CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null());
            if event.is_null() {
                return Err("CreateEvent failed".into());
            }
            let mut wave: Handle = std::ptr::null_mut();
            let rc = waveOutOpen(&mut wave, WAVE_MAPPER, &fmt, event as usize, 0, CALLBACK_EVENT);
            if rc != 0 {
                CloseHandle(event);
                return Err(format!("waveOutOpen failed: {rc}"));
            }
            Ok(Pcm { wave, event, headers: Vec::new(), data: Vec::new(), next: 0 })
        }
    }

    /// Queues interleaved stereo frames, blocking until the card has room.
    pub fn write(&mut self, samples: &[i16]) {
        let size = std::mem::size_of::<WaveHdr>() as u32;
        unsafe {
            if self.headers.len() < BUFFERS {
                // Still filling the queue at startup.
                let mut data = samples.to_vec();
                let mut hdr = Box::new(WaveHdr {
                    data: data.as_mut_ptr() as *mut u8,
                    buffer_length: (data.len() * 2) as u32,
                    bytes_recorded: 0,
                    user: 0,
                    flags: 0,
                    loops: 0,
                    next: std::ptr::null_mut(),
                    reserved: 0,
                });
                waveOutPrepareHeader(self.wave, &mut *hdr, size);
                waveOutWrite(self.wave, &mut *hdr, size);
                self.headers.push(hdr);
                self.data.push(data);
                return;
            }
            let i = self.next;
            self.next = (i + 1) % BUFFERS;
            // Buffers finish in order, so wait for the oldest. The timeout guards a lost signal.
            while std::ptr::read_volatile(&self.headers[i].flags) & WHDR_DONE == 0 {
                WaitForSingleObject(self.event, 100);
            }
            let n = samples.len().min(self.data[i].len());
            self.data[i][..n].copy_from_slice(&samples[..n]);
            let hdr = &mut self.headers[i];
            hdr.flags &= !WHDR_DONE;
            waveOutWrite(self.wave, &mut **hdr, size);
        }
    }
}

impl Drop for Pcm {
    fn drop(&mut self) {
        let size = std::mem::size_of::<WaveHdr>() as u32;
        unsafe {
            waveOutReset(self.wave);
            for h in &mut self.headers {
                if h.flags & WHDR_PREPARED != 0 {
                    waveOutUnprepareHeader(self.wave, &mut **h, size);
                }
            }
            waveOutClose(self.wave);
            CloseHandle(self.event);
        }
    }
}
