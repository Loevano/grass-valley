//! Non-village Python ensure_npcs over an explicit terrain window. Does not load
//! chunks or mutate the immutable import. Apply returned visit-key moves to other
//! towns when the scene is shared, in the same aggregate transaction (R20).
use super::{
    metadata::{array_mut, field_mut},
    position, scalar, set, version, write_node,
};
use crate::{
    BoundaryError,
    legacy::json::Node,
    social::{self, Ground},
};
use lantern_core::{python_random::PythonRandom, spatial::Footprint};
use std::collections::BTreeSet;
const PATH: &str = "$.npc_roster";
fn bad(s: &'static str) -> BoundaryError {
    BoundaryError::invalid(PATH, s)
}
fn bytes(n: &Node) -> Vec<u8> {
    let mut out = vec![];
    write_node(n, &mut out);
    out
}
fn name(n: &Node) -> Result<String, BoundaryError> {
    n.required("name", PATH)?.string(PATH)
}
fn key(n: &Node) -> Result<String, BoundaryError> {
    let (x, y) = position(n, PATH)?;
    Ok(format!("{}:{x}:{y}", name(n)?))
}
fn points(n: &Node, budget: &mut i64) -> Result<Vec<(i32, i32)>, BoundaryError> {
    let (x, y) = position(n, PATH)?;
    let (w, h) = if let Some(fp) = n.optional("footprint", PATH)? {
        let a = fp.array(PATH)?;
        if a.len() != 2 {
            return Err(bad("invalid footprint"));
        }
        (a[0].tile(PATH)?, a[1].tile(PATH)?)
    } else {
        (1, 1)
    };
    *budget -= i64::from(w.max(0)) * i64::from(h.max(0));
    if *budget < 0 {
        return Err(bad("footprint budget"));
    }
    let mut out = vec![];
    for dx in 0..w {
        for dy in 0..h {
            out.push((
                x.checked_add(dx).ok_or_else(|| bad("footprint overflow"))?,
                y.checked_add(dy).ok_or_else(|| bad("footprint overflow"))?,
            ));
        }
    }
    Ok(out)
}
pub struct NpcRoster {
    source: Vec<u8>,
    town: Vec<u8>,
    moved: Vec<(String, String)>,
}
impl NpcRoster {
    pub fn materialize(
        source: &[u8],
        terrain: &[Ground],
        obstacles: &[Footprint],
    ) -> Result<Self, BoundaryError> {
        let mut town = Node::parse(source, PATH)?;
        let roster = version(&town, "npc_roster_version", 3, PATH)?.unwrap_or(0);
        let placed = version(&town, "npc_placement_version", 1, PATH)?.unwrap_or(0) >= 1;
        let existing = town.optional("npcs", PATH)?.is_some();
        let current = existing && roster >= 3;
        let mut ground = terrain.to_vec();
        ground.sort_by_key(|g| (g.tile[1], g.tile[0]));
        let mut tiles = BTreeSet::new();
        if ground
            .iter()
            .any(|g| !tiles.insert(g.tile) || g.biome.chars().count() != 1)
        {
            return Err(bad("invalid terrain window"));
        }
        if let Some(ns) = town.optional("npcs", PATH)? {
            for n in ns.array(PATH)? {
                name(n)?;
                position(n, PATH)?;
            }
        }
        if existing {
            for n in array_mut(field_mut(&mut town, "npcs", PATH)?) {
                let name = name(n)?;
                if matches!(name.as_str(), "Dara" | "Petra")
                    && n.optional("dialogue", PATH)?.is_some()
                {
                    for theme in social::catalog()["themes"].as_array().unwrap() {
                        for person in theme[1].as_array().unwrap() {
                            if person[0].as_str() == Some(&name) {
                                set(n, "dialogue", scalar(person[2].as_str().unwrap()));
                            }
                        }
                    }
                }
            }
        }
        if current && placed {
            return Ok(Self {
                source: source.to_vec(),
                town: bytes(&town),
                moved: vec![],
            });
        }
        let mut budget = 1_000_000i64;
        let mut occupied = BTreeSet::new();
        let mut obscured = BTreeSet::new();
        for field in ["buildings", "map_resources", "npcs", "roads"] {
            if let Some(items) = town.optional(field, PATH)? {
                for n in items.array(PATH)? {
                    occupied.extend(points(n, &mut budget)?);
                }
            } else if field == "map_resources" {
                return Err(bad("missing resource window"));
            }
        }
        for field in ["map_resources", "tree_stumps"] {
            if let Some(items) = town.optional(field, PATH)? {
                for n in items.array(PATH)? {
                    if n.required("kind", PATH)?.string(PATH)? != "tree" {
                        continue;
                    }
                    let (x, y) = position(n, PATH)?;
                    let ancient = n
                        .optional("species", PATH)?
                        .is_some_and(|v| v.string(PATH).is_ok_and(|s| s == "ancient"));
                    for dx in -4i32..=0 {
                        for dy in -4i32..=0 {
                            if (dx - dy).abs() <= 1 && -(dx + dy) <= if ancient { 7 } else { 5 } {
                                obscured.insert((
                                    x.checked_add(dx).ok_or_else(|| bad("canopy overflow"))?,
                                    y.checked_add(dy).ok_or_else(|| bad("canopy overflow"))?,
                                ));
                            }
                        }
                    }
                }
            }
        }
        for fp in obstacles {
            let area = u64::from(fp.dimensions().0) * u64::from(fp.dimensions().1);
            if area > budget as u64 {
                return Err(bad("obstacle budget"));
            }
            budget -= area as i64;
            occupied.extend(fp.tiles().map(|t| (t.x, t.y)));
        }
        if let Some(player) = town.optional("player", PATH)? {
            let number = |k| -> Result<i32, BoundaryError> {
                let Node::Scalar(v) = player.required(k, PATH)? else {
                    return Err(bad("invalid player position"));
                };
                let n = v
                    .get()
                    .parse::<f64>()
                    .map_err(|_| bad("invalid player position"))?;
                if !n.is_finite()
                    || n.round_ties_even() < i32::MIN as f64
                    || n.round_ties_even() > i32::MAX as f64
                {
                    return Err(bad("invalid player position"));
                }
                Ok(n.round_ties_even() as i32)
            };
            occupied.insert((number("x")?, number("y")?));
        }
        occupied.extend(&obscured);
        if !existing {
            set(&mut town, "npcs", Node::Array(vec![]));
        }
        let ns = town.required("npcs", PATH)?.array(PATH)?;
        let names: BTreeSet<_> = ns.iter().map(name).collect::<Result<_, _>>()?;
        let hidden: Vec<_> = ns
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                (!placed && obscured.contains(&position(n, PATH).unwrap())).then_some(i)
            })
            .collect();
        let mut candidates: std::collections::BTreeMap<String, Vec<(i32, i32)>> = social::catalog()
            ["themes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t[0].as_str().unwrap().into(), vec![]))
            .collect();
        for g in &ground {
            if (g.tile[0] >= 12 || g.tile[1] >= 12)
                && !occupied.contains(&(g.tile[0], g.tile[1]))
                && let Some(c) = candidates.get_mut(&g.biome)
            {
                c.push((g.tile[0], g.tile[1]));
            }
        }
        let seed = if let Some(n) = town.optional("terrain_seed", PATH)? {
            let Node::Scalar(v) = n else {
                return Err(bad("unsupported seed"));
            };
            if v.get().starts_with('"') {
                n.string(PATH)?
            } else if v
                .get()
                .strip_prefix('-')
                .unwrap_or(v.get())
                .bytes()
                .all(|b| b.is_ascii_digit())
            {
                if v.get() == "-0" {
                    "0".into()
                } else {
                    v.get().into()
                }
            } else {
                return Err(bad("unsupported seed"));
            }
        } else {
            "0".into()
        };
        let mut rng = PythonRandom::from_bytes(format!("{seed}:npcs").as_bytes());
        let mut moved: Vec<(String, String)> = vec![];
        for i in hidden {
            let ns = array_mut(field_mut(&mut town, "npcs", PATH)?);
            let n = &mut ns[i];
            let biome = n
                .optional("biome", PATH)?
                .map(|v| v.string(PATH))
                .transpose()?
                .unwrap_or_default();
            let Some(c) = candidates.get_mut(&biome).filter(|c| !c.is_empty()) else {
                continue;
            };
            let (x, y) = position(n, PATH)?;
            let j = (0..c.len())
                .min_by_key(|&i| {
                    let (px, py) = c[i];
                    let (dx, dy) = (
                        i128::from(px) - i128::from(x),
                        i128::from(py) - i128::from(y),
                    );
                    (dx * dx + dy * dy, py, px)
                })
                .unwrap();
            let (nx, ny) = c.remove(j);
            let old = key(n)?;
            set(n, "x", scalar(nx));
            set(n, "y", scalar(ny));
            let new = key(n)?;
            if let Some(pair) = moved.iter_mut().find(|p| p.0 == old) {
                pair.1 = new;
            } else {
                moved.push((old, new));
            }
        }
        if !current {
            for theme in social::catalog()["themes"].as_array().unwrap() {
                let biome = theme[0].as_str().unwrap();
                let c = candidates.get_mut(biome).unwrap();
                for person in theme[1].as_array().unwrap() {
                    let name = person[0].as_str().unwrap();
                    if names.contains(name)
                        || (existing
                            && !social::catalog()["new_locals"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|v| v.as_str() == Some(name)))
                    {
                        continue;
                    }
                    if c.is_empty() {
                        break;
                    }
                    let i = rng.randbelow(c.len() as u64).unwrap() as usize;
                    let (x, y) = c.remove(i);
                    array_mut(field_mut(&mut town, "npcs", PATH)?).push(Node::Object(vec![
                        ("name".into(), scalar(name)),
                        ("role".into(), scalar(person[1].as_str().unwrap())),
                        ("dialogue".into(), scalar(person[2].as_str().unwrap())),
                        ("biome".into(), scalar(biome)),
                        ("x".into(), scalar(x)),
                        ("y".into(), scalar(y)),
                    ]));
                }
            }
        }
        set(&mut town, "npc_roster_version", scalar(3));
        set(&mut town, "npc_placement_version", scalar(1));
        move_visits(&mut town, &moved)?;
        Ok(Self {
            source: source.to_vec(),
            town: bytes(&town),
            moved,
        })
    }
    pub fn move_visit_keys(
        source: &[u8],
        moved: &[(String, String)],
    ) -> Result<Vec<u8>, BoundaryError> {
        let mut town = Node::parse(source, PATH)?;
        move_visits(&mut town, moved)?;
        Ok(bytes(&town))
    }
    pub fn private_source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn private_town_bytes(&self) -> &[u8] {
        &self.town
    }
    pub fn moved_visit_keys(&self) -> &[(String, String)] {
        &self.moved
    }
}

fn move_visits(town: &mut Node, moved: &[(String, String)]) -> Result<(), BoundaryError> {
    if town.optional("local_visits", PATH)?.is_some() {
        let visits = field_mut(town, "local_visits", PATH)?;
        visits.object(PATH)?;
        let Node::Object(fields) = visits else {
            unreachable!()
        };
        for (old, new) in moved {
            if let Some(i) = fields.iter().position(|(k, _)| k == old) {
                let (_, value) = fields.remove(i);
                if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == new) {
                    *v = value;
                } else {
                    fields.push((new.clone(), value));
                }
            }
        }
    }
    Ok(())
}
