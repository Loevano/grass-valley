//! Authored shared_world.new_world ground/resources and town sites, before NPCs,
//! chunk expansion, villages, tiny-water repair or gameplay preparation.
use crate::{
    legacy_map::{MapSeed, generate_map},
    python_random::PythonRandom,
    terrain::{GeneratedChunk, GenerationError, Resource, generate_chunk},
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TownSite {
    pub x: u32,
    pub y: u32,
}

pub struct Starter {
    pub landscape: GeneratedChunk,
    pub sites: [TownSite; 4],
}

fn resource(kind: &'static str, x: u32, y: u32) -> Resource {
    Resource {
        kind,
        x,
        y,
        species: None,
    }
}

/// Explicit seed/entropy input; integer and numeric-looking text are distinct.
/// Regional starters always sample the current 480-wide world at offset 160,
/// even when later reconstructed into a legacy 1920-wide world at offset 880.
pub fn generate_starter(seed: MapSeed<'_>, version: u32) -> Result<Starter, GenerationError> {
    if !(1..=7).contains(&version) {
        return Err(GenerationError::UnsupportedVersion);
    }
    let mut rng = match seed {
        MapSeed::Integer(value) => {
            PythonRandom::from_integer(value).ok_or(GenerationError::UnsupportedSeed)?
        }
        MapSeed::Text(value) => PythonRandom::from_bytes(value.as_bytes()),
    };
    // Python str(int) canonicalizes magnitude before formatted regional seeds.
    let namespace = match seed {
        MapSeed::Text(value) => String::from(value),
        MapSeed::Integer(value) => {
            let digits = value
                .strip_prefix('-')
                .unwrap_or(value)
                .trim_start_matches('0');
            if digits.is_empty() {
                String::from("0")
            } else if value.starts_with('-') {
                alloc::format!("-{digits}")
            } else {
                String::from(digits)
            }
        }
    };
    let mut sites = [TownSite { x: 0, y: 0 }; 4];
    for (site, (x, y)) in sites
        .iter_mut()
        .zip([(22, 22), (82, 22), (22, 82), (82, 82)])
    {
        site.x = rng.randrange(x, x + 17).unwrap() as u32;
        site.y = rng.randrange(y, y + 17).unwrap() as u32;
    }
    rng.shuffle(&mut sites);
    let base = if version == 1 {
        generate_map(120, seed)?
    } else {
        let mut chunks = Vec::new();
        for cy in 4..7 {
            for cx in 4..7 {
                chunks.push(generate_chunk(&namespace, cx, cy, version, 480)?);
            }
        }
        let mut terrain = Vec::new();
        for y in 0..120usize {
            let mut row = String::new();
            for cx in 0..3 {
                row.push_str(&chunks[y / 40 * 3 + cx].terrain[y % 40]);
            }
            terrain.push(row);
        }
        let resources = chunks
            .into_iter()
            .flat_map(|c| c.resources)
            .map(|mut r| {
                r.x -= 160;
                r.y -= 160;
                r
            })
            .collect();
        GeneratedChunk { terrain, resources }
    };
    let mut rows: Vec<Vec<u8>> = base.terrain.iter().map(|s| s.as_bytes().to_vec()).collect();
    let mut resources = base.resources;
    let mut cleared = BTreeSet::new();
    for site in sites {
        for y in site.y - 5..=site.y + 5 {
            for x in site.x - 5..=site.x + 5 {
                cleared.insert((x, y));
            }
        }
        let (mut x, mut y) = (site.x, site.y);
        while (x, y) != (sites[0].x, sites[0].y) {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    cleared.insert(((x as i32 + dx) as u32, (y as i32 + dy) as u32));
                }
            }
            if x != sites[0].x {
                x = (x as i32 + if x < sites[0].x { 1 } else { -1 }) as u32;
            } else {
                y = (y as i32 + if y < sites[0].y { 1 } else { -1 }) as u32;
            }
        }
    }
    for &(x, y) in &cleared {
        rows[y as usize][x as usize] = if version >= 6 { b'm' } else { b'g' };
    }
    resources.retain(|r| !cleared.contains(&(r.x, r.y)));
    let mut starters = Vec::new();
    for site in sites {
        for (kind, dx, dy) in [
            ("tree", -4, -3),
            ("tree", -4, -2),
            ("water", 4, -3),
            ("water", 4, -2),
        ] {
            starters.push(resource(
                kind,
                (site.x as i32 + dx) as u32,
                (site.y as i32 + dy) as u32,
            ));
        }
    }
    let occupied: BTreeSet<_> = resources
        .iter()
        .chain(&starters)
        .map(|r| (r.x, r.y))
        .collect();
    for site in sites {
        for dx in -7..-3 {
            for dy in -5..-1 {
                let (x, y) = ((site.x as i32 + dx) as u32, (site.y as i32 + dy) as u32);
                if !occupied.contains(&(x, y)) && rows[y as usize][x as usize] != b'v' {
                    resources.push(resource("tree", x, y));
                    rows[y as usize][x as usize] = b'f';
                }
            }
        }
    }
    if version >= 6 {
        for r in &starters {
            if r.kind == "tree" {
                rows[r.y as usize][r.x as usize] = b'f';
            }
        }
    }
    resources.extend(starters);
    if version >= 2 {
        let mut ponds = BTreeSet::new();
        for site in sites {
            for dy in -3..=3 {
                for dx in -5..=5 {
                    if libm::pow(f64::from(dx) / 5., 2.) + libm::pow(f64::from(dy) / 3., 2.) <= 1. {
                        ponds.insert((
                            (site.x as i32 + 9 + dx) as u32,
                            (site.y as i32 - 8 + dy) as u32,
                        ));
                    }
                }
            }
        }
        resources.retain(|r| !ponds.contains(&(r.x, r.y)));
        for (x, y) in ponds {
            rows[y as usize][x as usize] = b's';
            resources.push(resource("water", x, y));
        }
    }
    let (lake_x, lake_y) = (
        rng.randrange(54, 57).unwrap() as i32,
        rng.randrange(57, 62).unwrap() as i32,
    );
    let islands = [
        (lake_x - 7, lake_y - 3),
        (lake_x + 4, lake_y + 4),
        (lake_x + 9, lake_y - 5),
    ];
    let mut island_tiles = BTreeSet::new();
    for (cx, cy) in islands {
        for dx in -2..=2 {
            for dy in -2..=2 {
                if dx * dx + dy * dy <= 5 {
                    island_tiles.insert((cx + dx, cy + dy));
                }
            }
        }
    }
    let mut lake = BTreeSet::new();
    for y in lake_y - 14..=lake_y + 14 {
        for x in lake_x - 14..=lake_x + 14 {
            if libm::pow(f64::from(x - lake_x) / 14., 2.)
                + libm::pow(f64::from(y - lake_y) / 14., 2.)
                <= 1.
            {
                lake.insert((x as u32, y as u32));
            }
        }
    }
    resources.retain(|r| !lake.contains(&(r.x, r.y)));
    for (x, y) in lake {
        let island = island_tiles.contains(&(x as i32, y as i32));
        rows[y as usize][x as usize] = if island { b'g' } else { b's' };
        if !island {
            resources.push(resource("water", x, y));
        }
    }
    resources.extend(
        islands
            .into_iter()
            .map(|(x, y)| resource("chest", x as u32, y as u32)),
    );
    Ok(Starter {
        sites,
        landscape: GeneratedChunk {
            terrain: rows
                .into_iter()
                .map(|r| String::from_utf8(r).expect("ASCII terrain"))
                .collect(),
            resources,
        },
    })
}
