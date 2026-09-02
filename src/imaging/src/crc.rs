//! CRC-32, the one everything else calls CRC-32.
//!
//! Written here rather than taken as a dependency because it is twenty lines,
//! and because the number is going to be shown to a person next to a published
//! checksum — which means it has to be *the* CRC-32 (IEEE 802.3, reflected,
//! polynomial `0xEDB88320`, the one `gzip`, `zip` and `cksum -a crc32` all
//! produce) and not something merely similar. The test below pins it to the
//! published check value so a rewrite cannot quietly change what it computes.
//!
//! Not a security property. It catches a disk that wrote different bytes, which
//! is what disks do when they fail; it does not catch someone who chose the
//! bytes on purpose.

/// A running CRC-32.
#[derive(Debug, Clone, Copy)]
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    /// A fresh checksum.
    pub const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    /// Fold `bytes` in.
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let mut acc = (self.0 ^ u32::from(byte)) & 0xFF;
            for _ in 0..8 {
                acc = if acc & 1 == 1 {
                    (acc >> 1) ^ 0xEDB8_8320
                } else {
                    acc >> 1
                };
            }
            self.0 = acc ^ (self.0 >> 8);
        }
    }

    /// The finished value.
    pub const fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }

    /// The checksum of one slice.
    pub fn of(bytes: &[u8]) -> u32 {
        let mut crc = Self::new();
        crc.update(bytes);
        crc.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Crc32;

    #[test]
    fn it_is_the_crc32_everyone_elses_tool_prints() {
        // The published check value: the nine bytes "123456789" are 0xCBF43926
        // under CRC-32/ISO-HDLC. If this ever changes, the number shown next to
        // a downloaded image's published checksum has become a different number
        // wearing the same label.
        assert_eq!(Crc32::of(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn the_empty_input_is_zero() {
        assert_eq!(Crc32::of(b""), 0);
    }

    #[test]
    fn feeding_it_in_pieces_gives_the_same_answer_as_all_at_once() {
        // The image is checksummed a chunk at a time, so this is the property
        // the whole thing rests on.
        let data: Vec<u8> = (0..10_000).map(|i| (i % 251) as u8).collect();
        let whole = Crc32::of(&data);
        let mut piecewise = Crc32::new();
        for chunk in data.chunks(97) {
            piecewise.update(chunk);
        }
        assert_eq!(piecewise.finish(), whole);
    }

    #[test]
    fn one_flipped_bit_changes_it() {
        let mut data = vec![7_u8; 4_096];
        let before = Crc32::of(&data);
        data[2_000] ^= 1;
        assert_ne!(Crc32::of(&data), before);
    }
}
