//! Minimal ALSA playback binding, loaded with dlopen at runtime so that neither the
//! ALSA headers nor pkg-config are required to build, and a missing libasound just
//! means a silent game rather than a failure to launch.
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};

type OpenFn = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type SetParamsFn =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type WriteiFn = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
type RecoverFn = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type CloseFn = unsafe extern "C" fn(*mut c_void) -> c_int;

const SND_PCM_STREAM_PLAYBACK: c_int = 0;
const SND_PCM_FORMAT_S16_LE: c_int = 2;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;

pub struct Pcm {
    pcm: *mut c_void,
    writei: WriteiFn,
    recover: RecoverFn,
    close: CloseFn,
}

// The handle is only ever used from the audio thread once moved there.
unsafe impl Send for Pcm {}

impl Pcm {
    pub fn open(rate: u32, latency_us: u32) -> Result<Self, String> {
        unsafe {
            let lib = ["libasound.so.2", "libasound.so"].iter().find_map(|name| {
                let c = CString::new(*name).unwrap();
                let h = libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                (!h.is_null()).then_some(h)
            });
            let lib = lib.ok_or("libasound not found")?;
            let sym = |name: &str| -> Result<*mut c_void, String> {
                let c = CString::new(name).unwrap();
                let p = libc::dlsym(lib, c.as_ptr());
                if p.is_null() {
                    Err(format!("missing symbol {name}"))
                } else {
                    Ok(p)
                }
            };
            let open: OpenFn = std::mem::transmute(sym("snd_pcm_open")?);
            let set_params: SetParamsFn = std::mem::transmute(sym("snd_pcm_set_params")?);
            let writei: WriteiFn = std::mem::transmute(sym("snd_pcm_writei")?);
            let recover: RecoverFn = std::mem::transmute(sym("snd_pcm_recover")?);
            let close: CloseFn = std::mem::transmute(sym("snd_pcm_close")?);

            let mut pcm: *mut c_void = std::ptr::null_mut();
            let dev = CString::new("default").unwrap();
            let rc = open(&mut pcm, dev.as_ptr(), SND_PCM_STREAM_PLAYBACK, 0);
            if rc < 0 {
                return Err(format!("snd_pcm_open failed: {rc}"));
            }
            let rc = set_params(
                pcm,
                SND_PCM_FORMAT_S16_LE,
                SND_PCM_ACCESS_RW_INTERLEAVED,
                2,
                rate,
                1,
                latency_us,
            );
            if rc < 0 {
                close(pcm);
                return Err(format!("snd_pcm_set_params failed: {rc}"));
            }
            Ok(Pcm { pcm, writei, recover, close })
        }
    }

    /// Blocking write of interleaved stereo frames.
    pub fn write(&mut self, samples: &[i16]) {
        let mut off = 0usize;
        let frames = samples.len() / 2;
        while off < frames {
            let n = unsafe {
                (self.writei)(
                    self.pcm,
                    samples[off * 2..].as_ptr() as *const c_void,
                    (frames - off) as c_ulong,
                )
            };
            if n < 0 {
                let rc = unsafe { (self.recover)(self.pcm, n as c_int, 1) };
                if rc < 0 {
                    return;
                }
            } else {
                off += n as usize;
            }
        }
    }
}

impl Drop for Pcm {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.pcm);
        }
    }
}
