use lantern_core::{ids::WorldId, seed_material::*, units::TerrainSeed};

fn material(seed: SeedSource<'_>, world: &str, subsystem: &str) -> Vec<u8> {
    gameplay_seed_material_v1(seed, &WorldId::new(world).unwrap(), subsystem).unwrap()
}

#[test]
fn v1_seed_encoding_matches_independent_python_struct_vectors() {
    // Frozen using Python struct.pack('>Q', ...), independently of Rust encoding.
    let vectors = [
        (
            SeedSource::Unsigned(TerrainSeed(0)),
            "legacy",
            "economy",
            "6c616e7465726e2d67616d65706c61792d73656564000100000000000000000000000000000000066c6567616379000000000000000765636f6e6f6d79",
        ),
        (
            SeedSource::Unsigned(TerrainSeed(u64::MAX)),
            "世界",
            "residents",
            "6c616e7465726e2d67616d65706c61792d73656564000100ffffffffffffffff0000000000000006e4b896e7958c00000000000000097265736964656e7473",
        ),
        (
            SeedSource::Text("0007"),
            "legacy",
            "production",
            "6c616e7465726e2d67616d65706c61792d7365656400010100000000000000043030303700000000000000066c6567616379000000000000000a70726f64756374696f6e",
        ),
    ];
    for (seed, world, subsystem, expected) in vectors {
        let hex: String = material(seed, world, subsystem)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hex, expected);
    }
}

#[test]
fn source_types_and_length_framing_prevent_ambiguous_streams() {
    let seed = SeedSource::Unsigned(TerrainSeed(7));
    assert_ne!(material(seed, "ab", "c"), material(seed, "a", "bc"));
    assert_ne!(
        material(seed, "world", "economy"),
        material(SeedSource::Text("7"), "world", "economy")
    );
    assert_ne!(
        material(seed, "world", "economy"),
        material(seed, "world", "residents")
    );
    assert_ne!(
        material(seed, "world", "economy"),
        material(seed, "other-world", "economy")
    );
    assert_ne!(
        material(seed, "é", "economy"),
        material(seed, "e\u{301}", "economy")
    );
    assert_eq!(
        gameplay_seed_material_v1(seed, &WorldId::new("world").unwrap(), ""),
        Err(SeedMaterialError::EmptySubsystem)
    );
}
