//! Tiny binary reader and writer shared by save files and the network protocol.
//! The reader never panics: malformed input yields `None`, which matters because
//! its input can come from the network or from a damaged file.
use glam::{Vec2, Vec3};

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn v2(&mut self, v: Vec2) {
        self.f32(v.x);
        self.f32(v.y);
    }
    pub fn v3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    pub fn str(&mut self, s: &str) {
        let b = &s.as_bytes()[..s.len().min(u16::MAX as usize)];
        self.u16(b.len() as u16);
        self.buf.extend_from_slice(b);
    }
    /// Fixed-point float, 1/100 unit steps, range about ±327.
    pub fn q(&mut self, v: f32) {
        self.buf.extend_from_slice(&((v * 100.0).clamp(-32767.0, 32767.0) as i16).to_le_bytes());
    }
    pub fn q2(&mut self, v: Vec2) {
        self.q(v.x);
        self.q(v.y);
    }
    pub fn q3(&mut self, v: Vec3) {
        self.q(v.x);
        self.q(v.y);
        self.q(v.z);
    }
    /// A 0..=1 value in one byte.
    pub fn unit(&mut self, v: f32) {
        self.u8((v.clamp(0.0, 1.0) * 255.0) as u8);
    }
}

pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    #[cfg(test)]
    pub fn left(&self) -> usize {
        self.data.len() - self.pos
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    pub fn bool(&mut self) -> Option<bool> {
        self.u8().map(|b| b != 0)
    }
    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    /// Rejects NaN and infinity, which would otherwise poison the simulation.
    pub fn f32(&mut self) -> Option<f32> {
        let v = self.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))?;
        v.is_finite().then_some(v)
    }
    pub fn v2(&mut self) -> Option<Vec2> {
        Some(Vec2::new(self.f32()?, self.f32()?))
    }
    pub fn v3(&mut self) -> Option<Vec3> {
        Some(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    pub fn str(&mut self) -> Option<String> {
        let n = self.u16()? as usize;
        let b = self.take(n)?;
        Some(String::from_utf8_lossy(b).into_owned())
    }
    pub fn q(&mut self) -> Option<f32> {
        self.take(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 100.0)
    }
    pub fn q2(&mut self) -> Option<Vec2> {
        Some(Vec2::new(self.q()?, self.q()?))
    }
    pub fn q3(&mut self) -> Option<Vec3> {
        Some(Vec3::new(self.q()?, self.q()?, self.q()?))
    }
    pub fn unit(&mut self) -> Option<f32> {
        self.u8().map(|b| b as f32 / 255.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn values_round_trip_and_truncation_is_safe() {
        let mut w = Writer::new();
        w.u8(7);
        w.u16(513);
        w.u32(70000);
        w.u64(1 << 40);
        w.f32(1.5);
        w.str("HELLO");
        w.q(-12.34);
        w.unit(0.5);
        let mut r = Reader::new(&w.buf);
        assert_eq!(r.u8(), Some(7));
        assert_eq!(r.u16(), Some(513));
        assert_eq!(r.u32(), Some(70000));
        assert_eq!(r.u64(), Some(1 << 40));
        assert_eq!(r.f32(), Some(1.5));
        assert_eq!(r.str().as_deref(), Some("HELLO"));
        assert!((r.q().unwrap() + 12.34).abs() < 0.011);
        assert!((r.unit().unwrap() - 0.5).abs() < 0.01);
        assert_eq!(r.u8(), None);
        // A string length that overruns the buffer, and a NaN.
        assert_eq!(Reader::new(&[200, 0, 1]).str(), None);
        assert_eq!(Reader::new(&f32::NAN.to_le_bytes()).f32(), None);
    }
}
