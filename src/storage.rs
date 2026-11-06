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

    pub fn deliver(name: &str, data: &[u8]) -> Result<String, String> {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).ok_or("no home folder")?;
        deliver_under(&std::path::PathBuf::from(home), name, data)
    }

    pub fn deliver_under(home: &Path, name: &str, data: &[u8]) -> Result<String, String> {
        let dir = home.join("Music").join("ROTFALL Soundtrack");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(name), data).map_err(|e| e.to_string())?;
        Ok(dir.display().to_string())
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

    /// Starts an ordinary browser download of bytes made on the spot.
    pub fn deliver(name: &str, data: &[u8]) -> Result<String, String> {
        use wasm_bindgen::JsCast;
        let fail = |_| "the browser refused the download".to_string();
        let bytes = js_sys::Uint8Array::from(data);
        let parts = js_sys::Array::of1(&bytes);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type("audio/wav");
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(fail)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(fail)?;
        let doc = web_sys::window().and_then(|w| w.document()).ok_or("no page")?;
        let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(fail)?.unchecked_into();
        a.set_href(&url);
        a.set_download(name);
        a.click();
        Ok("YOUR DOWNLOADS".to_string())
    }
}

pub fn read(path: &Path) -> Option<Vec<u8>> {
    imp::read(path)
}

/// Hands a finished file to the player. Returns where it went, in words for the screen.
pub fn deliver(name: &str, data: &[u8]) -> Result<String, String> {
    imp::deliver(name, data)
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn downloads_land_in_the_music_folder() {
        let home = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-saves/home");
        let place = imp::deliver_under(&home, "01 Test.wav", b"RIFF").unwrap();
        assert!(place.ends_with("ROTFALL Soundtrack"), "{place}");
        assert_eq!(std::fs::read(home.join("Music/ROTFALL Soundtrack/01 Test.wav")).unwrap(), b"RIFF");
    }
}
