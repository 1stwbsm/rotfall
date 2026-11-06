//! Where saves, settings and records live: files on the desktop, localStorage in a browser.
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::path::Path;

    pub fn read(path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    /// Write-then-rename, so a crash mid-save can never destroy the previous good save.
    pub fn write(path: &Path, data: &[u8]) -> Result<(), String> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn remove(path: &Path) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::path::Path;

    const CODE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn store() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    fn key(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// localStorage only holds text, so binary saves travel as base64.
    fn encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
        for c in data.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            out.push(CODE[(n >> 18) as usize & 63] as char);
            out.push(CODE[(n >> 12) as usize & 63] as char);
            out.push(if c.len() > 1 { CODE[(n >> 6) as usize & 63] as char } else { '=' });
            out.push(if c.len() > 2 { CODE[n as usize & 63] as char } else { '=' });
        }
        out
    }

    fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() * 3 / 4);
        for c in text.as_bytes().chunks(4) {
            let mut n = 0u32;
            let mut pad = 0;
            for &b in c {
                n <<= 6;
                if b == b'=' {
                    pad += 1;
                } else {
                    n |= CODE.iter().position(|&x| x == b)? as u32;
                }
            }
            out.push((n >> 16) as u8);
            if pad < 2 {
                out.push((n >> 8) as u8);
            }
            if pad < 1 {
                out.push(n as u8);
            }
        }
        Some(out)
    }

    pub fn read(path: &Path) -> Option<Vec<u8>> {
        decode(&store()?.get_item(&key(path)).ok()??)
    }

    pub fn write(path: &Path, data: &[u8]) -> Result<(), String> {
        let s = store().ok_or("browser storage is unavailable")?;
        s.set_item(&key(path), &encode(data)).map_err(|_| "browser storage is full".to_string())
    }

    pub fn remove(path: &Path) {
        if let Some(s) = store() {
            let _ = s.remove_item(&key(path));
        }
    }
}

pub fn read(path: &Path) -> Option<Vec<u8>> {
    imp::read(path)
}

pub fn read_text(path: &Path) -> Option<String> {
    String::from_utf8(read(path)?).ok()
}

pub fn write(path: &Path, data: &[u8]) -> Result<(), String> {
    imp::write(path, data)
}

pub fn remove(path: &Path) {
    imp::remove(path)
}
