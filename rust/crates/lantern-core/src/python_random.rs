//! CPython Random version-2 byte/integer seeding and MT19937 draws.
//! Terrain uses formatted text seeds; gameplay byte material uses this same
//! SHA-512 mixing, never an implementation-defined Rust RNG.
use alloc::vec::Vec;
use sha2::{Digest, Sha512};

pub struct PythonRandom {
    state: [u32; 624],
    index: usize,
}
impl PythonRandom {
    /// Python integers seed from their absolute magnitude, without narrowing.
    pub fn from_integer(decimal: &str) -> Option<Self> {
        let digits = decimal.strip_prefix('-').unwrap_or(decimal);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut words = alloc::vec![0u32];
        for byte in digits.bytes() {
            let mut carry = u64::from(byte - b'0');
            for word in &mut words {
                let value = u64::from(*word) * 10 + carry;
                *word = value as u32;
                carry = value >> 32;
            }
            if carry != 0 {
                words.push(carry as u32);
            }
        }
        Some(Self::from_words(&words))
    }
    /// Half-open integer interval, preserving CPython's rejection draws.
    pub fn randrange(&mut self, start: i64, stop: i64) -> Option<i64> {
        let width = i128::from(stop) - i128::from(start);
        if width <= 0 || width > i128::from(u64::MAX) {
            return None;
        }
        Some((i128::from(start) + i128::from(self.randbelow(width as u64)?)) as i64)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let digest = Sha512::digest(bytes);
        let material: Vec<u8> = bytes.iter().chain(digest.iter()).copied().collect();
        let words: Vec<u32> = material
            .rchunks(4)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0, |value, byte| (value << 8) | u32::from(*byte))
            })
            .collect();
        Self::from_words(&words)
    }
    /// Absolute integer magnitude, least-significant words first, as CPython.
    pub fn from_words(words: &[u32]) -> Self {
        let fallback = [0];
        let words = if words.is_empty() { &fallback } else { words };
        let mut rng = Self {
            state: [0; 624],
            index: 624,
        };
        rng.state[0] = 19650218;
        for i in 1..624 {
            rng.state[i] = 1812433253u32
                .wrapping_mul(rng.state[i - 1] ^ (rng.state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let (mut i, mut j) = (1, 0);
        for _ in 0..624.max(words.len()) {
            rng.state[i] = (rng.state[i]
                ^ (rng.state[i - 1] ^ (rng.state[i - 1] >> 30)).wrapping_mul(1664525))
            .wrapping_add(words[j])
            .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                rng.state[0] = rng.state[623];
                i = 1;
            }
            if j >= words.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            rng.state[i] = (rng.state[i]
                ^ (rng.state[i - 1] ^ (rng.state[i - 1] >> 30)).wrapping_mul(1566083941))
            .wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                rng.state[0] = rng.state[623];
                i = 1;
            }
        }
        rng.state[0] = 0x80000000;
        rng
    }
    fn word(&mut self) -> u32 {
        if self.index >= 624 {
            for i in 0..624 {
                let y = (self.state[i] & 0x80000000) | (self.state[(i + 1) % 624] & 0x7fffffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^= y >> 18;
        y
    }
    pub fn random(&mut self) -> f64 {
        let a = self.word() >> 5;
        let b = self.word() >> 6;
        (f64::from(a) * 67108864.0 + f64::from(b)) / 9007199254740992.0
    }
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }
    /// CPython's little-endian word order; supported widths include 0 and 64.
    pub fn getrandbits(&mut self, bits: u32) -> Option<u64> {
        match bits {
            0 => Some(0),
            1..=32 => Some(u64::from(self.word() >> (32 - bits))),
            33..=64 => {
                let low = u64::from(self.word());
                Some(low | (u64::from(self.word() >> (64 - bits)) << 32))
            }
            _ => None,
        }
    }
    pub fn randbelow(&mut self, n: u64) -> Option<u64> {
        if n == 0 {
            return None;
        }
        let bits = 64 - n.leading_zeros();
        loop {
            let r = self.getrandbits(bits)?;
            if r < n {
                return Some(r);
            }
        }
    }
    pub fn choice<'a, T>(&mut self, values: &'a [T]) -> Option<&'a T> {
        self.randbelow(values.len() as u64)
            .map(|i| &values[i as usize])
    }
    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            let j = self.randbelow((i + 1) as u64).unwrap() as usize;
            values.swap(i, j);
        }
    }
}
