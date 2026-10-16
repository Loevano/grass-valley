//! Explicit sleeping-chunk preparation, before authoritative lifecycle installation.
use super::{
    ChunkKey,
    json::Node,
    village_overlay::write_node,
    village_overlay::{position, rows, validate_resources, version},
};
use crate::BoundaryError;
use std::collections::BTreeSet;

const PATH: &str = "$.chunk_catch_up";

/// Private owned result of Python `ChunkMap.catch_up`, with no archive mutation.
/// The caller supplies occupied footprint tiles (buildings, roads, NPCs and
/// Python-rounded founders), not client snapshots. This does not renew active
/// trees, advance production, consume cache plans, or install an active chunk.
/// ```compile_fail
/// use lantern_host::legacy::CaughtUpChunk;
/// fn leak(record: &CaughtUpChunk) { serde_json::to_string(record).unwrap(); }
/// ```
pub struct CaughtUpChunk {
    source: Vec<u8>,
    record: Vec<u8>,
}
impl CaughtUpChunk {
    /// `clock` is a private JSON numeric scalar from the supplied simulation time.
    /// Absent landscape clocks must be supplied as `0`; no wall clock is read.
    /// Integral clocks above 2^53 reject rather than silently round elapsed time.
    pub fn prepare(
        source: &[u8],
        key: ChunkKey,
        world_size: u32,
        generator: u32,
        clock: &[u8],
        occupied_tiles: &[(i32, i32)],
    ) -> Result<Self, BoundaryError> {
        if ![480, 1920].contains(&world_size)
            || key.x >= world_size / 40
            || key.y >= world_size / 40
            || !(1..=7).contains(&generator)
        {
            return Err(BoundaryError::invalid(
                PATH,
                "unsupported bounds or generator",
            ));
        }
        let mut record = Node::parse(source, PATH)?;
        let terrain = rows(&record, PATH)?;
        let left = key.x as i32 * 40;
        let top = key.y as i32 * 40;
        validate_resources(&record, left, top, PATH)?;
        for (field, maximum) in [
            ("mineral_version", 1),
            ("industry_version", 2),
            ("village_version", 3),
            ("swamp_version", 2),
        ] {
            version(&record, field, maximum, PATH)?;
        }
        if version(&record, "terrain_version", 7, PATH)?.is_some_and(|v| v != u64::from(generator))
        {
            return Err(BoundaryError::invalid(PATH, "terrain version mismatch"));
        }
        let supplied = Node::parse(clock, &format!("{PATH}.clock"))?;
        let now = time(&supplied)?;
        let previous = record
            .optional("clock", PATH)?
            .map(time)
            .transpose()?
            .unwrap_or(0.0);
        let elapsed = (now - previous).max(0.0);
        // Validate relevant gameplay types before even filtered resources are lost.
        for field in ["map_resources", "tree_stumps"] {
            for (i, resource) in record
                .required(field, PATH)?
                .array(PATH)?
                .iter()
                .enumerate()
            {
                let path = format!("{PATH}.{field}[{i}].kind");
                let kind = resource.required("kind", &path)?.string(&path)?;
                if !matches!(
                    kind.as_str(),
                    "chest"
                        | "tree"
                        | "water"
                        | "rock"
                        | "berry_bush"
                        | "herb_patch"
                        | "wild_horse"
                        | "village_hut"
                        | "village_market"
                        | "village_decor"
                ) || (field == "tree_stumps" && kind != "tree")
                {
                    return Err(BoundaryError::invalid(path, "unsupported resource kind"));
                }
            }
        }
        if version(&record, "mineral_version", 1, PATH)? != Some(1) {
            for resource in items(&mut record, "map_resources") {
                if kind(resource) == "rock" {
                    let (x, y) = position(resource, PATH)?;
                    if (x * 73 + y * 137) % 7 == 0 {
                        set(resource, "ore", scalar("iron"));
                    } else {
                        remove(resource, "ore");
                    }
                }
            }
            set(&mut record, "mineral_version", scalar(1));
        }
        let industry = version(&record, "industry_version", 2, PATH)?.unwrap_or(0);
        if industry == 1 {
            items(&mut record, "map_resources").retain(|r| {
                let (x, y) = position(r, PATH).unwrap();
                kind(r) != "wild_horse" || (x * 31 + y * 53) % 10 == 0
            });
        } else if industry == 0 {
            let mut occupied: BTreeSet<_> = occupied_tiles.iter().copied().collect();
            for field in ["map_resources", "tree_stumps"] {
                for r in record.required(field, PATH)?.array(PATH)? {
                    occupied.insert(position(r, PATH)?);
                }
            }
            for (y, row) in terrain.iter().enumerate() {
                for (x, biome) in row.iter().enumerate() {
                    let (tx, ty) = (left + x as i32, top + y as i32);
                    if b"gm".contains(biome)
                        && (tx * 73 + ty * 137 + tx * ty * 11) % 499 == 71
                        && (tx * 31 + ty * 53) % 10 == 0
                        && occupied.insert((tx, ty))
                    {
                        items(&mut record, "map_resources").push(Node::Object(vec![
                            ("kind".into(), scalar("wild_horse")),
                            ("x".into(), scalar(tx)),
                            ("y".into(), scalar(ty)),
                        ]));
                    }
                }
            }
        }
        set(&mut record, "industry_version", scalar(2));
        for field in ["map_resources", "tree_stumps"] {
            let allowed: &[u8] = if generator < 6 {
                b"gfmb"
            } else if field == "tree_stumps" {
                b"fb"
            } else {
                b"gfb"
            };
            items(&mut record, field).retain(|r| {
                let (x, y) = position(r, PATH).unwrap();
                kind(r) != "tree"
                    || allowed.contains(&terrain[(y - top) as usize][(x - left) as usize])
            });
        }
        for resource in items(&mut record, "map_resources") {
            let (maximum, regeneration) = match kind(resource).as_str() {
                "tree" => {
                    let (x, y) = position(resource, PATH)?;
                    let swamp = terrain[(y - top) as usize][(x - left) as usize] == b'b';
                    let ancient = resource
                        .optional("species", PATH)?
                        .map(|s| s.string(PATH))
                        .transpose()?
                        .is_some_and(|s| s == "ancient");
                    (if swamp || ancient { 40.0 } else { 8.0 }, 0.0)
                }
                "water" => (12.0, 3.2),
                "rock" => (60.0, 0.15),
                "berry_bush" => (6.0, 0.3),
                "herb_patch" => (4.0, 0.2),
                _ => continue,
            };
            let supply = resource
                .optional("supply", PATH)?
                .map(|s| s.nonnegative_number(PATH))
                .transpose()?
                .unwrap_or(maximum);
            let available = (supply + elapsed / 3.0 * regeneration).min(maximum);
            set(resource, "supply", scalar(available));
            set(resource, "supply_max", scalar(maximum));
            set(resource, "regeneration", scalar(regeneration));
        }
        for stump in items(&mut record, "tree_stumps") {
            let remaining = stump
                .required("remaining", PATH)?
                .nonnegative_number(PATH)?;
            // CPython's short-float round(x, 9) uses correctly rounded decimal
            // conversion, not multiply/round/divide (which double rounds).
            let rounded: f64 = format!("{:.9}", remaining - elapsed * 0.9).parse().unwrap();
            set(stump, "remaining", scalar(rounded.max(3.0 * 0.9)));
        }
        set(&mut record, "clock", supplied);
        let mut output = Vec::new();
        write_node(&record, &mut output);
        Ok(Self {
            source: source.to_vec(),
            record: output,
        })
    }
    pub fn private_source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn private_record_bytes(&self) -> &[u8] {
        &self.record
    }
}
fn time(node: &Node) -> Result<f64, BoundaryError> {
    let path = format!("{PATH}.clock");
    let value = node.nonnegative_number(&path)?;
    if let Node::Scalar(raw) = node {
        let text = raw.get();
        if !text.contains(['.', 'e', 'E'])
            && text
                .strip_prefix('-')
                .unwrap_or(text)
                .parse::<u64>()
                .map_or(true, |n| n > 9007199254740992)
        {
            return Err(BoundaryError::invalid(path, "unsupported clock precision"));
        }
    }
    if value > 9007199254740992.0 {
        return Err(BoundaryError::invalid(path, "unsupported clock precision"));
    }
    Ok(value)
}
fn kind(node: &Node) -> String {
    node.required("kind", PATH).unwrap().string(PATH).unwrap()
}
fn scalar(value: impl serde::Serialize) -> Node {
    Node::Scalar(serde_json::value::to_raw_value(&value).unwrap())
}
fn set(node: &mut Node, name: &str, value: Node) {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    if let Some((_, old)) = fields.iter_mut().find(|(key, _)| key == name) {
        *old = value;
    } else {
        fields.push((name.into(), value));
    }
}
fn remove(node: &mut Node, name: &str) {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    fields.retain(|(key, _)| key != name);
}
fn items<'a>(node: &'a mut Node, name: &str) -> &'a mut Vec<Node> {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    let Node::Array(items) = &mut fields.iter_mut().find(|(key, _)| key == name).unwrap().1 else {
        unreachable!()
    };
    items
}
