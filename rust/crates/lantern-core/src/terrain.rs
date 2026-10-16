//! Raw versioned Python chunk generators. No starter/village overlays, tiny-water
//! repair, supplies, activity clocks, persistence or authoritative world mutation.
use crate::python_random::PythonRandom;
use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use blake2::{
    Blake2bVar,
    digest::{Update, VariableOutput},
};
const TAU: f64 = core::f64::consts::TAU;

#[derive(Clone, PartialEq, Eq)]
pub struct Resource {
    pub kind: &'static str,
    pub x: u32,
    pub y: u32,
    pub species: Option<&'static str>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct GeneratedChunk {
    pub terrain: Vec<String>,
    pub resources: Vec<Resource>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationError {
    UnsupportedVersion,
    UnsupportedSize,
    UnsupportedSeed,
    OutsideWorld,
}

/// Seed is the exact Python str(seed) namespace, not a Rust hash or JSON string.
/// Imported integer seeds retain canonical decimal spelling, including sign and
/// arbitrary precision. Text seeds retain their exact Unicode spelling.
pub fn generate_chunk(
    seed: &str,
    cx: u32,
    cy: u32,
    version: u32,
    size: u32,
) -> Result<GeneratedChunk, GenerationError> {
    if !(1..=7).contains(&version) {
        return Err(GenerationError::UnsupportedVersion);
    }
    if ![480, 1920].contains(&size) {
        return Err(GenerationError::UnsupportedSize);
    }
    if cx >= size / 40 || cy >= size / 40 {
        return Err(GenerationError::OutsideWorld);
    }
    let mut field = Field {
        seed,
        values: BTreeMap::new(),
    };
    let mut rng =
        PythonRandom::from_bytes(format!("{seed}:chunk-v{}:{cx},{cy}", version.min(3)).as_bytes());
    let plan = Plan::new(seed, version, size, cx, cy);
    let mut result = GeneratedChunk {
        terrain: Vec::new(),
        resources: Vec::new(),
    };
    for y in cy * 40..(cy + 1) * 40 {
        let mut row = String::new();
        for x in cx * 40..(cx + 1) * 40 {
            let (biome, water, trees, rocks, species) = if version == 1 {
                let wet = field.sample("moisture", x, y, 18., true);
                let ground = field.sample("ground", x, y, 25., true);
                let lake = field.sample("lake", x, y, 45., true);
                let temp = field.sample("temperature", x, y, 90., true);
                let b = if temp < 0.22 {
                    'i'
                } else if temp > 0.65 && wet < 0.45 {
                    'd'
                } else if wet > 0.78 {
                    'b'
                } else if wet > 0.58 {
                    'f'
                } else if ground < 0.3 {
                    's'
                } else if ground > 0.75 {
                    'r'
                } else if wet > 0.4 {
                    'm'
                } else {
                    'g'
                };
                (
                    if lake < 0.25 && !"bdi".contains(b) {
                        's'
                    } else {
                        b
                    },
                    lake < 0.25,
                    if "fb".contains(b) { 0.45 } else { 0. },
                    if b == 'r' { 0.15 } else { 0. },
                    None,
                )
            } else if version == 2 {
                plan.v2(x, y, size, &mut field)
            } else {
                plan.regional(x, y, size, version, &mut field)
            };
            let kind = if water {
                Some("water")
            } else if "gfmb".contains(biome) && trees != 0. && rng.random() < trees {
                Some("tree")
            } else if rocks != 0. && rng.random() < rocks {
                Some("rock")
            } else {
                None
            };
            row.push(biome);
            if let Some(kind) = kind {
                result.resources.push(Resource {
                    kind,
                    x,
                    y,
                    species: if kind == "tree" { species } else { None },
                });
            } else if "gfmb".contains(biome) && rng.random() < 0.012 {
                result.resources.push(Resource {
                    kind: rng.choice(&["berry_bush", "herb_patch"]).unwrap(),
                    x,
                    y,
                    species: None,
                });
            }
        }
        result.terrain.push(row);
    }
    Ok(result)
}
struct Field<'a> {
    seed: &'a str,
    values: BTreeMap<(&'static str, i64, i64), f64>,
}
impl Field<'_> {
    fn value(&mut self, label: &'static str, x: i64, y: i64, integer: bool) -> f64 {
        *self.values.entry((label, x, y)).or_insert_with(|| {
            // Python's float floor-division produces float hash coordinates in
            // regional generators ("1.0"), integer coordinates in v1 ("1").
            let text = if integer {
                format!("{}:{label}:{x}:{y}", self.seed)
            } else {
                format!("{}:{label}:{x}.0:{y}.0", self.seed)
            };
            let mut h = Blake2bVar::new(8).unwrap();
            h.update(text.as_bytes());
            let mut bytes = [0; 8];
            h.finalize_variable(&mut bytes).unwrap();
            u64::from_be_bytes(bytes) as f64 / 18446744073709551616.0
        })
    }
    fn sample(&mut self, label: &'static str, x: u32, y: u32, scale: f64, integer: bool) -> f64 {
        let (x, y) = (f64::from(x), f64::from(y));
        let (ix, iy) = (python_floor_div(x, scale), python_floor_div(y, scale));
        let (fx, fy) = ((x % scale) / scale, (y % scale) / scale);
        let (fx, fy) = (fx * fx * (3. - 2. * fx), fy * fy * (3. - 2. * fy));
        let a = self.value(label, ix, iy, integer) * (1. - fx)
            + self.value(label, ix + 1, iy, integer) * fx;
        let b = self.value(label, ix, iy + 1, integer) * (1. - fx)
            + self.value(label, ix + 1, iy + 1, integer) * fx;
        a * (1. - fy) + b * fy
    }
}
// CPython float divmod derives the quotient from fmod, then snaps it to
// the nearest integer. floor(x / scale) differs at e.g. 160 // (480 / 9).
fn python_floor_div(value: f64, scale: f64) -> i64 {
    let div = (value - value % scale) / scale;
    let floor = libm::floor(div);
    (if div - floor > 0.5 {
        floor + 1.0
    } else {
        floor
    }) as i64
}
type Ellipse = (f64, f64, f64, f64);
type Segment = (f64, f64, f64, f64, f64, f64);
struct Plan {
    swamp: Ellipse,
    desert: Ellipse,
    ice: Ellipse,
    lake: Ellipse,
    islands: [Ellipse; 3],
    phase: f64,
    segments: Vec<Segment>,
}
fn distance(u: f64, v: f64, e: Ellipse) -> f64 {
    let a = (u - e.0) / e.2;
    let b = (v - e.1) / e.3;
    libm::pow(a, 2.0) + libm::pow(b, 2.0)
}
impl Plan {
    fn new(seed: &str, version: u32, size: u32, cx: u32, cy: u32) -> Self {
        let mut rng = PythonRandom::from_bytes(
            format!("{seed}:regions-v{}", if version < 3 { 2 } else { 3 }).as_bytes(),
        );
        let mut p = if version < 3 {
            Self {
                swamp: (0.17 + rng.uniform(-0.025, 0.025), 0.42, 0.22, 0.26),
                desert: (0.78 + rng.uniform(-0.025, 0.025), 0.43, 0.25, 0.30),
                ice: (0.28, 0.04, 0.36, 0.25),
                lake: (0.57 + rng.uniform(-0.025, 0.025), 0.79, 0.16, 0.13),
                islands: [(0., 0., 1., 1.); 3],
                phase: rng.uniform(0., TAU),
                segments: Vec::new(),
            }
        } else {
            let lx = rng.uniform(0.56, 0.60);
            Self {
                swamp: (rng.uniform(0.16, 0.19), 0.43, 0.22, 0.25),
                desert: (rng.uniform(0.79, 0.82), 0.43, 0.17, 0.21),
                ice: (0.26, 0.05, 0.29, 0.16),
                lake: (lx, 0.79, 0.145, 0.115),
                islands: [
                    (lx - 0.055, 0.765, 0.023, 0.018),
                    (lx + 0.035, 0.815, 0.030, 0.022),
                    (lx + 0.060, 0.755, 0.016, 0.020),
                ],
                phase: rng.uniform(0., TAU),
                segments: Vec::new(),
            }
        };
        if version >= 3 {
            p.rivers(seed, size, cx, cy);
        }
        p
    }
    fn v2(
        &self,
        x: u32,
        y: u32,
        size: u32,
        f: &mut Field<'_>,
    ) -> (char, bool, f64, f64, Option<&'static str>) {
        let s = f64::from(size);
        let (u, v) = (f64::from(x) / s, f64::from(y) / s);
        let detail = f.sample("region-edge", x, y, s / 20., false);
        let wet = f.sample("regional-moisture", x, y, s / 5., false);
        let soil = f.sample("regional-ground", x, y, s / 8., false);
        let mut b = if wet > 0.56 {
            'f'
        } else if soil > 0.76 {
            'r'
        } else if wet > 0.38 {
            'm'
        } else {
            'g'
        };
        let edge = 0.88 + detail * 0.24;
        let marsh = distance(u, v, self.swamp);
        if distance(u, v, self.ice) < edge {
            b = 'i';
        } else if distance(u, v, self.desert) < edge {
            b = 'd';
        } else if marsh < edge || marsh < 1.6 && detail > 0.67 {
            b = 'b';
        } else if soil < 0.23 && detail < 0.30 {
            b = 's';
        }
        let (lx, ly, _, _) = self.lake;
        let approach = ((v - 0.60) / (ly - 0.60)).clamp(0., 1.);
        let river_x = 0.65 * (1. - approach)
            + lx * approach
            + 0.012 * (libm::sin((v - ly) * TAU + self.phase) - libm::sin(self.phase));
        let tributary_y = ly + 0.035 * libm::sin((u - lx) * TAU);
        let mut water = distance(u, v, self.lake) < 0.92 + detail * 0.16
            || v <= ly && (u - river_x).abs() < 0.012
            || u <= lx && (v - tributary_y).abs() < 0.010;
        if b == 'b' && f.sample("marsh-pools", x, y, s / 24., false) < 0.27 {
            water = true;
        }
        if water && !"bdi".contains(b) {
            b = 's';
        }
        (
            b,
            water,
            if "fb".contains(b) { 0.45 } else { 0. },
            if b == 'r' { 0.15 } else { 0. },
            None,
        )
    }
    fn rivers(&mut self, seed: &str, size: u32, cx: u32, cy: u32) {
        let lake = (self.lake.0, self.lake.1);
        let junction = (0.66, 0.56);
        let west = (0.13, 0.63);
        let north = (0.65, 0.12);
        let paths: &[(f64, &[(f64, f64)])] = &[
            (
                0.0065,
                &[
                    (0.78, -0.03),
                    (0.70, 0.045),
                    north,
                    (0.71, 0.24),
                    (0.64, 0.34),
                    (0.65, 0.44),
                    junction,
                    (0.60, 0.67),
                    lake,
                ],
            ),
            (
                0.0050,
                &[
                    (-0.03, 0.55),
                    (0.04, 0.59),
                    west,
                    (0.23, 0.73),
                    (0.34, 0.66),
                    (0.43, 0.74),
                    (0.51, 0.70),
                    lake,
                ],
            ),
            (
                0.0035,
                &[
                    (-0.03, 0.22),
                    (0.045, 0.30),
                    (0.09, 0.39),
                    (0.045, 0.49),
                    west,
                ],
            ),
            (
                0.0040,
                &[
                    (1.03, 0.69),
                    (0.91, 0.62),
                    (0.83, 0.72),
                    (0.75, 0.63),
                    junction,
                ],
            ),
            (
                0.0035,
                &[
                    (0.08, -0.03),
                    (0.13, 0.065),
                    (0.075, 0.14),
                    (0.18, 0.22),
                    (0.30, 0.18),
                    (0.39, 0.25),
                    (0.52, 0.20),
                    north,
                ],
            ),
        ];
        for (branch, (width, controls)) in paths.iter().enumerate() {
            let mut rng = PythonRandom::from_bytes(format!("{seed}:river-v3:{branch}").as_bytes());
            let points: Vec<_> = controls
                .iter()
                .enumerate()
                .map(|(i, &p)| {
                    if [lake, junction, west, north].contains(&p)
                        || i == 0
                        || i == controls.len() - 1
                    {
                        p
                    } else {
                        (
                            p.0 + rng.uniform(-0.008, 0.008),
                            p.1 + rng.uniform(-0.008, 0.008),
                        )
                    }
                })
                .collect();
            let mut samples = Vec::new();
            for i in 0..points.len() - 1 {
                let (p0, p1, p2, p3) = (
                    points[i.saturating_sub(1)],
                    points[i],
                    points[i + 1],
                    points[(i + 2).min(points.len() - 1)],
                );
                for step in 0..12 {
                    let t = f64::from(step) / 12.;
                    let axis = |a: f64, b: f64, c: f64, d: f64| {
                        0.5 * (2. * b
                            + (-a + c) * t
                            + (2. * a - 5. * b + 4. * c - d) * t * t
                            + (-a + 3. * b - 3. * c + d) * t * t * t)
                            * f64::from(size)
                    };
                    samples.push((axis(p0.0, p1.0, p2.0, p3.0), axis(p0.1, p1.1, p2.1, p3.1)));
                }
            }
            let last = points.last().unwrap();
            samples.push((last.0 * f64::from(size), last.1 * f64::from(size)));
            for (i, pair) in samples.windows(2).enumerate() {
                let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
                let (dx, dy) = (bx - ax, by - ay);
                let radius = width
                    * f64::from(size)
                    * (0.85
                        + 0.25 * i as f64 / (samples.len() - 1) as f64
                        + 0.12 * libm::sin(i as f64 * 0.3 + self.phase));
                let margin = radius + 3.;
                let (left, top) = (f64::from(cx * 40), f64::from(cy * 40));
                if ax.max(bx) + margin >= left
                    && ax.min(bx) - margin < left + 40.
                    && ay.max(by) + margin >= top
                    && ay.min(by) - margin < top + 40.
                {
                    self.segments
                        .push((ax, ay, dx, dy, 1. / (dx * dx + dy * dy), radius));
                }
            }
        }
    }
    fn regional(
        &self,
        x: u32,
        y: u32,
        size: u32,
        version: u32,
        f: &mut Field<'_>,
    ) -> (char, bool, f64, f64, Option<&'static str>) {
        let s = f64::from(size);
        let (u, v) = (f64::from(x) / s, f64::from(y) / s);
        let detail = f.sample("v3-edge", x, y, s / 24., false);
        let woods = f.sample("v3-woods", x, y, s / 22., false);
        let canopy = f.sample("v3-canopy", x, y, s / 60., false);
        let wet = f.sample("v3-moisture", x, y, s / 9., false);
        let soil = f.sample("v3-ground", x, y, s / 14., false);
        let open = version >= 5;
        let green = version >= 6;
        let mut biome = if woods > 0.62 {
            'f'
        } else if soil > if open { 0.87 } else { 0.80 } {
            'r'
        } else if wet > 0.48 {
            'm'
        } else {
            'g'
        };
        let mut trees = (woods - 0.36).max(0.) * (0.45 + canopy) * 0.90;
        if version >= 4 && "gm".contains(biome) {
            trees *= 0.5;
        }
        let mut rock = if open {
            if biome == 'r' { 0.10 } else { 0.003 }
        } else if biome == 'r' {
            0.18
        } else {
            0.008
        };
        if open && f.sample("v5-pasture", x, y, s / 6., false) > 0.50 {
            biome = 'm';
            trees = 0.006 + canopy * 0.012;
            rock = 0.001;
        }
        if green && "gf".contains(biome) {
            if f.sample("v6-large-woods", x, y, s / 5., false) > 0.67 {
                biome = 'f';
            }
            trees = if biome == 'f' {
                0.40 + canopy * 0.22
            } else {
                0.025 + canopy * 0.05
            };
        }
        let mut species = None;
        let marsh = distance(u, v, self.swamp);
        let edge = 0.80 + detail * 0.40;
        if distance(u, v, self.ice) < edge {
            biome = if woods > 0.66 && canopy > 0.48 {
                'f'
            } else if soil > if open { 0.81 } else { 0.71 } {
                'r'
            } else {
                'i'
            };
            trees = if biome == 'f' {
                0.12 + (canopy - 0.45).max(0.) * 0.60
            } else {
                0.
            };
            rock = if open {
                if "ir".contains(biome) { 0.045 } else { 0.008 }
            } else if "ir".contains(biome) {
                0.09
            } else {
                0.015
            };
        } else if distance(u, v, self.desert) < edge {
            biome = if soil > if open { 0.80 } else { 0.69 } && detail > 0.42 {
                'r'
            } else {
                'd'
            };
            trees = 0.;
            rock = if open {
                if biome == 'r' { 0.08 } else { 0.008 }
            } else if biome == 'r' {
                0.15
            } else {
                0.018
            };
        } else if marsh < edge || marsh < 1.55 && detail > 0.65 {
            biome = 'b';
            trees = 0.30 + woods * 0.22 + canopy * 0.15;
            rock = 0.;
        } else if soil < 0.22 && detail < 0.30 {
            biome = 's';
            trees = 0.;
        }
        let (mut channel, mut bank) = (false, false);
        for &(ax, ay, dx, dy, inverse, radius) in &self.segments {
            let t = (((f64::from(x) - ax) * dx + (f64::from(y) - ay) * dy) * inverse).clamp(0., 1.);
            let (a, b) = (f64::from(x) - ax - t * dx, f64::from(y) - ay - t * dy);
            let squared = libm::pow(a, 2.0) + libm::pow(b, 2.0);
            if squared < radius * radius {
                channel = true;
                break;
            }
            if squared < libm::pow(radius + 2.5, 2.0) {
                bank = true;
            }
        }
        let lake_distance = distance(u, v, self.lake);
        let shoreline = 0.78 + detail * 0.44;
        let lake = lake_distance < shoreline;
        if green && !lake && lake_distance < shoreline + 0.14 {
            bank = true;
        }
        let island = lake
            && self
                .islands
                .iter()
                .any(|&e| distance(u, v, e) < 0.76 + detail * 0.38);
        let mut water = channel || lake && !island;
        if island && !channel {
            biome = if green || woods > 0.48 { 'f' } else { 'g' };
            trees = 0.25 + canopy * 0.25;
            rock = 0.015;
        }
        if biome == 'b' {
            let pools = f.sample("v3-marsh-pools", x, y, s / 40., false);
            let ribbons = f.sample("v3-marsh-ribbons", x, y, s / 14., false);
            water = water || pools < 0.34 || (ribbons - 0.5).abs() < 0.045 && pools < 0.57;
        }
        if "dr".contains(biome) {
            let oasis = [(0.85, 0.36, 0.013, 0.010), (0.79, 0.54, 0.016, 0.011)]
                .iter()
                .map(|&e| distance(u, v, e))
                .fold(f64::INFINITY, f64::min);
            if oasis < 1. {
                water = true;
            } else if oasis < 4. {
                biome = if green { 'f' } else { 'g' };
                trees = 0.22 + canopy * 0.30;
                species = Some("palm");
                rock = 0.;
            }
        }
        if "ir".contains(biome)
            && [(0.20, 0.075, 0.020, 0.012), (0.33, 0.045, 0.014, 0.010)]
                .iter()
                .any(|&e| distance(u, v, e) < 1.)
        {
            water = true;
        }
        if bank && !water && "gmf".contains(biome) {
            if green {
                biome = 'f';
                trees = 0.35 + canopy * 0.22;
            } else {
                biome = 'm';
                trees = trees.max(0.12 + canopy * 0.22);
            }
        }
        if green {
            if biome == 'm' {
                trees = 0.;
            } else if biome == 'g' {
                trees = trees.min(0.075);
            }
        }
        if water {
            trees = 0.;
            rock = 0.;
            if !"bdi".contains(biome) {
                biome = 's';
            }
        }
        (biome, water, trees, rock, species)
    }
}
