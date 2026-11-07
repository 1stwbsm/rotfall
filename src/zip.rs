//! A minimal zip writer. Files are stored uncompressed: audio barely shrinks with deflate, and
//! this keeps the work small enough to do on a browser's main thread.
use crate::util::crc32;

/// The name, size and checksum a zip's closing directory repeats for each file.
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

// DOS time fields. Fixed, so that the same files always make the same archive.
const TIME: u16 = 0;
const DATE: u16 = (46 << 9) | (1 << 5) | 1; // 2026-01-01

impl ZipWriter {
    pub fn new() -> Self {
        ZipWriter { out: Vec::new(), central: Vec::new(), count: 0 }
    }

    pub fn add(&mut self, name: &str, data: &[u8]) {
        let (crc, size, offset) = (crc32(data), data.len() as u32, self.out.len() as u32);
        let (out, cen) = (&mut self.out, &mut self.central);
        let name = name.as_bytes();
        let shared = |v: &mut Vec<u8>| {
            for x in [20u16, 0, 0, TIME, DATE] {
                v.extend_from_slice(&x.to_le_bytes());
            }
            for x in [crc, size, size] {
                v.extend_from_slice(&x.to_le_bytes());
            }
            v.extend_from_slice(&(name.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
        };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        shared(out);
        out.extend_from_slice(name);
        out.extend_from_slice(data);

        cen.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        cen.extend_from_slice(&20u16.to_le_bytes());
        shared(cen);
        // Comment length, disk number, internal and external attributes.
        cen.extend_from_slice(&[0u8; 10]);
        cen.extend_from_slice(&offset.to_le_bytes());
        cen.extend_from_slice(name);
        self.count += 1;
    }

    pub fn finish(mut self) -> Vec<u8> {
        let (start, size) = (self.out.len() as u32, self.central.len() as u32);
        self.out.append(&mut self.central);
        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0u8; 4]);
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&start.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_has_the_right_structure() {
        let mut z = ZipWriter::new();
        z.add("01 One.wav", b"first file");
        z.add("02 Two.wav", b"second");
        let d = z.finish();
        assert_eq!(&d[..4], &[0x50, 0x4b, 3, 4]);
        // The end record sits last, and points at a directory listing both files.
        let end = d.len() - 22;
        assert_eq!(&d[end..end + 4], &[0x50, 0x4b, 5, 6]);
        assert_eq!(u16::from_le_bytes([d[end + 10], d[end + 11]]), 2);
        let at = u32::from_le_bytes(d[end + 16..end + 20].try_into().unwrap()) as usize;
        assert_eq!(&d[at..at + 4], &[0x50, 0x4b, 1, 2]);
        // Stored data is findable, with the standard checksum of "first file".
        assert!(d.windows(10).any(|w| w == b"first file"));
        assert_eq!(u32::from_le_bytes(d[14..18].try_into().unwrap()), crc32(b"first file"));
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "the published CRC-32 check value");
    }
}
