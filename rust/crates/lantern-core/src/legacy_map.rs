//! Python terrain.generate_map compatibility, separate from regional chunks.
use crate::{
    python_random::PythonRandom,
    terrain::{GeneratedChunk, GenerationError, Resource},
};
use alloc::{string::String, vec::Vec};

/// Direct Random(seed) distinguishes integers from text, unlike formatted chunk seeds.
#[derive(Clone, Copy)]
pub enum MapSeed<'a> {
    Integer(&'a str),
    Text(&'a str),
}

struct Field {
    scale: u32,
    width: usize,
    values: Vec<f64>,
}
impl Field {
    fn new(size: u32, scale: u32, rng: &mut PythonRandom) -> Self {
        let width = size.div_ceil(scale) as usize + 1;
        Self {
            scale,
            width,
            values: (0..width * width).map(|_| rng.random()).collect(),
        }
    }
    fn sample(&self, x: u32, y: u32) -> f64 {
        let (ix, iy) = ((x / self.scale) as usize, (y / self.scale) as usize);
        let (fx, fy) = (
            f64::from(x % self.scale) / f64::from(self.scale),
            f64::from(y % self.scale) / f64::from(self.scale),
        );
        let (fx, fy) = (fx * fx * (3. - 2. * fx), fy * fy * (3. - 2. * fy));
        let top = self.values[iy * self.width + ix] * (1. - fx)
            + self.values[iy * self.width + ix + 1] * fx;
        let bottom = self.values[(iy + 1) * self.width + ix] * (1. - fx)
            + self.values[(iy + 1) * self.width + ix + 1] * fx;
        top * (1. - fy) + bottom * fy
    }
}
fn resource(kind: &'static str, x: u32, y: u32) -> Resource {
    Resource {
        kind,
        x,
        y,
        species: None,
    }
}

/// Raw map including authored corner resources. No town clearing or hydration migrations.
/// Sizes 1..=1920 are supported; entropy and allocation policy belong to the host.
pub fn generate_map(size: u32, seed: MapSeed<'_>) -> Result<GeneratedChunk, GenerationError> {
    if size == 0 || size > 1920 {
        return Err(GenerationError::UnsupportedSize);
    }
    let mut rng = match seed {
        MapSeed::Integer(decimal) => {
            PythonRandom::from_integer(decimal).ok_or(GenerationError::UnsupportedSeed)?
        }
        MapSeed::Text(text) => PythonRandom::from_bytes(text.as_bytes()),
    };
    let moisture = Field::new(size, 8, &mut rng);
    let ground = Field::new(size, 11, &mut rng);
    let lakes = Field::new(size, 23, &mut rng);
    let lake_detail = Field::new(size, 6, &mut rng);
    let coast = Field::new(size, 17, &mut rng);
    let (center_x, center_y) = (rng.uniform(0.45, 0.55), rng.uniform(0.45, 0.55));
    let (radius_x, radius_y) = (rng.uniform(0.49, 0.55), rng.uniform(0.49, 0.55));
    let n = f64::from(size);
    let (swamp_x, swamp_y) = (rng.uniform(0.3, 0.7) * n, rng.uniform(0.3, 0.7) * n);
    let (swamp_rx, swamp_ry) = (rng.uniform(0.12, 0.18) * n, rng.uniform(0.12, 0.18) * n);
    let mut terrain = Vec::<Vec<u8>>::new();
    let mut resources = Vec::new();
    for y in 0..size {
        let mut row = Vec::new();
        for x in 0..size {
            let (xf, yf) = (f64::from(x), f64::from(y));
            let outline = libm::pow((xf / n - center_x).abs() / radius_x, 4.)
                + libm::pow((yf / n - center_y).abs() / radius_y, 4.);
            if size > 12 && !(x < 12 && y < 12) && outline > 0.8 + coast.sample(x, y) * 0.5 {
                row.push(b'v');
                continue;
            }
            let (wet, soil) = (moisture.sample(x, y), ground.sample(x, y));
            let mut biome = if wet > 0.58 {
                b'f'
            } else if soil < 0.3 {
                b's'
            } else if soil > 0.75 {
                b'r'
            } else if wet > 0.4 {
                b'm'
            } else {
                b'g'
            };
            let marsh =
                libm::pow((xf - swamp_x) / swamp_rx, 2.) + libm::pow((yf - swamp_y) / swamp_ry, 2.);
            let swamp = size > 12 && !(x < 12 && y < 12) && marsh < 0.8 + wet * 0.5;
            if swamp {
                biome = b'b';
            }
            let mut kind = if lakes.sample(x, y) * 0.75 + lake_detail.sample(x, y) * 0.25
                < (if swamp { 0.48 } else { 0.28 })
            {
                biome = if swamp { b'b' } else { b's' };
                Some("water")
            } else if (swamp && rng.random() < 0.22) || (biome == b'f' && rng.random() < 0.7) {
                Some("tree")
            } else if biome == b'r' && rng.random() < 0.18 {
                Some("rock")
            } else if rng.random() < 0.025 {
                Some("tree")
            } else {
                None
            };
            if x < 12 && y < 12 {
                kind = None;
                if (1..=8).contains(&x) && (1..=9).contains(&y) {
                    biome = b'g';
                }
            }
            row.push(biome);
            if let Some(kind) = kind {
                let mut r = resource(kind, x, y);
                if kind == "tree" && !swamp && rng.random() < 0.06 {
                    r.species = Some("ancient");
                }
                if kind != "tree" || b"gfmb".contains(&biome) {
                    resources.push(r);
                }
            }
        }
        terrain.push(row);
    }
    const TREES: [(u32, u32); 18] = [
        (0, 2),
        (0, 3),
        (1, 4),
        (2, 0),
        (3, 0),
        (3, 1),
        (0, 7),
        (1, 8),
        (2, 9),
        (3, 10),
        (7, 0),
        (8, 1),
        (8, 2),
        (10, 6),
        (11, 7),
        (10, 9),
        (9, 10),
        (11, 11),
    ];
    // Draw all three widths even when the map clips the entire pond.
    let mut water = Vec::new();
    for y in 0..3 {
        for x in 9..9 + rng.randrange(1, 4).unwrap() as u32 {
            water.push((x, y));
        }
    }
    for (x, y) in TREES {
        if x < size && y < size {
            if !b"gfmb".contains(&terrain[y as usize][x as usize]) {
                terrain[y as usize][x as usize] = b'g';
            }
            resources.push(resource("tree", x, y));
        }
    }
    for (kind, tiles) in [
        ("water", water.as_slice()),
        ("rock", &[(10, 3), (11, 4), (0, 10)][..]),
    ] {
        for &(x, y) in tiles {
            if x < size && y < size {
                resources.push(resource(kind, x, y));
            }
        }
    }
    Ok(GeneratedChunk {
        terrain: terrain
            .into_iter()
            .map(|row| String::from_utf8(row).unwrap())
            .collect(),
        resources,
    })
}
