//! Stable hashing primitives.
//!
//! FNV-1a is used everywhere a value must be identical across runs, machines,
//! and architectures. We only ever feed it fixed-width little-endian bytes, so
//! the result never depends on pointer width or `Hash` derive ordering — the
//! canonical state hash needs that guarantee to make cross-device replay
//! verification meaningful.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub struct FnvHasher(u64);

impl FnvHasher {
    pub fn new() -> Self {
        Self(FNV_OFFSET)
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_i32(&mut self, v: i32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for FnvHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// SplitMix64 — bit-mixer used to expand a 64-bit key into well-distributed
/// PRNG seed material.
pub fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
