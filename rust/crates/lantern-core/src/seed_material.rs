//! D05 gameplay seed-material encoding v1. This is not a random generator or
//! terrain seed migration. R07 must implement Python's bytes-seed v2 / MT19937
//! semantics before adoption. Existing injected replay draws remain unchanged.

use crate::{ids::WorldId, units::TerrainSeed};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug)]
pub enum SeedSource<'a> {
    Unsigned(TerrainSeed),
    Text(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedMaterialError {
    EmptySubsystem,
}

fn text(output: &mut Vec<u8>, value: &str) {
    // Supported targets have 32/64-bit usize; UTF-8 byte lengths are exact.
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}

/// Domain/version prefix, typed seed, then length-prefixed world/subsystem UTF-8.
/// No trimming, case folding, Unicode normalization or platform-native integers.
/// Feed these bytes to Python-compatible random.seed(bytes, version=2), rather
/// than using a platform hash or interpreting them as an integer seed directly.
pub fn gameplay_seed_material_v1(
    seed: SeedSource<'_>,
    world: &WorldId,
    subsystem: &str,
) -> Result<Vec<u8>, SeedMaterialError> {
    if subsystem.is_empty() {
        return Err(SeedMaterialError::EmptySubsystem);
    }
    let mut output = Vec::from(&b"lantern-gameplay-seed\0\x01"[..]);
    match seed {
        SeedSource::Unsigned(seed) => {
            output.push(0);
            output.extend_from_slice(&seed.0.to_be_bytes());
        }
        SeedSource::Text(seed) => {
            output.push(1);
            text(&mut output, seed);
        }
    }
    text(&mut output, world.as_str());
    text(&mut output, subsystem);
    Ok(output)
}
