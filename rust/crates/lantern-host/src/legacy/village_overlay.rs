//! Ordinary village phase only: Python villages._apply_villages, not apply_villages.
//! Also exposes the independent swamp-v1 phase; no placement, organic upgrades,
//! water repair, activation, supply defaults or writes.
use super::{ChunkKey, json::Node, loaded::decode_resources};
use crate::BoundaryError;
use std::collections::BTreeSet;

const PATH: &str = "$.ordinary_village_overlay";
const BASE: [(&str, i32, i32); 4] = [
    ("village_hut", -3, -2),
    ("village_hut", 3, -2),
    ("village_hut", -3, 3),
    ("village_market", 3, 3),
];
const EXTRA: [(&str, i32, i32); 6] = [
    ("village_hut", -7, -4),
    ("village_hut", 7, -4),
    ("village_hut", -7, 6),
    ("village_hut", 7, 6),
    ("village_hut", -3, -7),
    ("village_hut", 3, 8),
];
const LIFE: [(&str, i32, i32, &str, &str); 4] = [
    ("village_hut", -3, 8, "hall", "Village hall"),
    ("village_hut", -7, -7, "barn", "Animal shelter"),
    ("village_market", 3, 5, "stall", "Produce stall"),
    ("village_market", 5, 3, "stall", "Travelers’ stall"),
];
const PLANTS: [(&str, i32, i32); 7] = [
    ("tree", -5, -4),
    ("tree", 5, -4),
    ("herb_patch", -5, 6),
    ("berry_bush", 5, 6),
    ("herb_patch", 3, -6),
    ("berry_bush", -4, 7),
    ("water", 7, -7),
];

/// Independent private materialization of only the ordinary village phase.
/// Exact source bytes remain available; retained scalar lexemes and arbitrary
/// extensions survive in the materialized record. Neither payload is public.
/// This deliberately does not implement ChunkMap.read or modify ImportedWorld.
/// ```compile_fail
/// use lantern_host::legacy::OrdinaryVillageOverlay;
/// fn leak(view: &OrdinaryVillageOverlay) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct OrdinaryVillageOverlay {
    source: Vec<u8>,
    record: Vec<u8>,
}
impl OrdinaryVillageOverlay {
    /// `villages` is a JSON array of already selected/versioned village metadata.
    /// Callers still need placement/migration, swamp and post-overlay water phases.
    /// Explicit unsupported markers and malformed relevant fields are rejected.
    pub fn materialize(
        source: &[u8],
        villages: &[u8],
        key: ChunkKey,
        world_size: u32,
    ) -> Result<Self, BoundaryError> {
        if ![480, 1920].contains(&world_size)
            || key.x >= world_size / 40
            || key.y >= world_size / 40
        {
            return Err(BoundaryError::invalid(
                PATH,
                "unsupported world bounds or chunk key",
            ));
        }
        let mut record = Node::parse(source, PATH)?;
        let previous = version(&record, "village_version", 3, PATH)?.unwrap_or(0);
        version(&record, "swamp_version", 2, PATH)?;
        version(&record, "terrain_version", 7, PATH)?;
        let left = key.x as i32 * 40;
        let top = key.y as i32 * 40;
        rows(&record, PATH)?;
        let mut pending_rows: Option<Vec<Vec<u8>>> = None;
        validate_resources(&record, left, top, PATH)?;
        if let Some(clock) = record.optional("clock", PATH)? {
            clock.nonnegative_number(&format!("{PATH}.clock"))?;
        }
        let village_nodes = Node::parse(villages, &format!("{PATH}.villages"))?;
        let villages = village_nodes
            .array(&format!("{PATH}.villages"))?
            .iter()
            .enumerate()
            .map(|(i, n)| Village::parse(n, i, world_size, PATH))
            .collect::<Result<Vec<_>, _>>()?;
        // Validate even a completed marker: unsupported metadata must not become
        // silently accepted simply because no edits happen for this record.
        if previous != 3 {
            for village in villages {
                if previous >= 2 || (previous == 1 && !village.expanded) {
                    if village.life {
                        // Python edits record terrain in this branch without
                        // updating any pending expansion buffer from a prior
                        // village. Keep that ordering even for mixed layouts.
                        let mut life_rows = rows(&record, PATH)?;
                        add_life(&mut record, &mut life_rows, &village, left, top)?;
                        commit_rows(&mut record, &life_rows);
                    }
                    continue;
                }
                let (vx, vy) = (village.x, village.y);
                if left > vx + 24 || left + 40 <= vx - 24 || top > vy + 24 || top + 40 <= vy - 24 {
                    continue;
                }
                if pending_rows.is_none() {
                    pending_rows = Some(rows(&record, PATH)?);
                }
                let rows = pending_rows.as_mut().unwrap();
                let mut cleared = BTreeSet::new();
                for y in top.max(vy - 24)..(top + 40).min(vy + 25) {
                    for x in left.max(vx - 24)..(left + 40).min(vx + 25) {
                        let (dx, dy) = (x - vx, y - vy);
                        if dx * dx + dy * dy > 24 * 24 {
                            continue;
                        }
                        if previous == 0 {
                            rows[(y - top) as usize][(x - left) as usize] = village.biome;
                        }
                        if dx.abs().max(dy.abs()) <= if village.expanded { 9 } else { 5 }
                            || (previous == 0 && (dx.abs() <= 1 || dy.abs() <= 1))
                        {
                            cleared.insert((x, y));
                            if village.biome == b'b' {
                                rows[(y - top) as usize][(x - left) as usize] = b'g';
                            }
                        }
                    }
                }
                filter_resources(&mut record, |x, y| {
                    !cleared.contains(&(x, y))
                        && !(previous == 0
                            && matches!(village.biome, b'd' | b'i')
                            && (x - vx).pow(2) + (y - vy).pow(2) <= 24 * 24)
                })?;
                let buildings =
                    BASE.iter()
                        .chain(EXTRA.iter().take(if village.expanded { 6 } else { 0 }));
                for (index, &(kind, dx, dy)) in buildings.enumerate() {
                    let (x, y) = (vx + dx, vy + dy);
                    if !inside(x, y, left, top) {
                        continue;
                    }
                    let mut item = structure(kind, x, y, &village, true);
                    if let Some(label) = match index {
                        4 => Some(village.landmarks[0]),
                        8 => Some(village.landmarks[1]),
                        9 => Some(village.landmarks[2]),
                        _ => None,
                    } {
                        set(
                            &mut item,
                            "variant",
                            scalar(if index == 9 { "inn" } else { "workshop" }),
                        );
                        set(&mut item, "label", scalar(label));
                    }
                    append(&mut record, item);
                }
                if village.life {
                    commit_rows(&mut record, rows);
                    add_life(&mut record, rows, &village, left, top)?;
                    commit_rows(&mut record, rows);
                }
            }
            if let Some(rows) = pending_rows {
                commit_rows(&mut record, &rows);
            }
            set(&mut record, "village_version", scalar(3));
        }
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

struct Village {
    name: String,
    biome: u8,
    x: i32,
    y: i32,
    expanded: bool,
    life: bool,
    landmarks: [&'static str; 3],
}
impl Village {
    fn parse(node: &Node, index: usize, size: u32, root: &str) -> Result<Self, BoundaryError> {
        let path = format!("{root}.villages[{index}]");
        let name = node
            .required("name", &path)?
            .string(&format!("{path}.name"))?;
        let biome = node
            .required("biome", &path)?
            .string(&format!("{path}.biome"))?;
        let (expected, landmarks) = match name.as_str() {
            "Reedhaven" => ("b", ["Willow Still", "Reedworks", "Lantern Rest"]),
            "Sunwell" => ("d", ["Sunwell cistern", "Dune Loom", "Amber Rest"]),
            "Frostbrook" => ("i", ["Pineworks", "Snowthread", "Hearth Rest"]),
            _ => {
                return Err(BoundaryError::invalid(
                    format!("{path}.name"),
                    "unsupported village theme",
                ));
            }
        };
        if biome != expected {
            return Err(BoundaryError::invalid(
                format!("{path}.biome"),
                "unsupported village biome",
            ));
        }
        let (x, y) = position(node, &path)?;
        if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
            return Err(BoundaryError::invalid(&path, "village outside world"));
        }
        let layout = version(node, "layout_version", 2, &path)?;
        if layout == Some(0) {
            return Err(BoundaryError::invalid(
                format!("{path}.layout_version"),
                "unsupported layout version",
            ));
        }
        let life = version(node, "life_version", 1, &path)? == Some(1);
        version(node, "swamp_version", 2, &path)?;
        Ok(Self {
            name,
            biome: expected.as_bytes()[0],
            x,
            y,
            expanded: layout == Some(2),
            life,
            landmarks,
        })
    }
}
fn commit_rows(record: &mut Node, rows: &[Vec<u8>]) {
    set(
        record,
        "terrain",
        Node::Array(
            rows.iter()
                .map(|row| scalar(std::str::from_utf8(row).unwrap()))
                .collect(),
        ),
    );
}

fn version(node: &Node, field: &str, max: u64, path: &str) -> Result<Option<u64>, BoundaryError> {
    let value = node
        .optional(field, path)?
        .map(|n| n.unsigned(&format!("{path}.{field}")))
        .transpose()?;
    if value.is_some_and(|v| v > max || (field == "terrain_version" && v == 0)) {
        return Err(BoundaryError::invalid(
            format!("{path}.{field}"),
            "unsupported version",
        ));
    }
    Ok(value)
}
fn position(node: &Node, path: &str) -> Result<(i32, i32), BoundaryError> {
    Ok((
        node.required("x", path)?.tile(&format!("{path}.x"))?,
        node.required("y", path)?.tile(&format!("{path}.y"))?,
    ))
}
fn rows(node: &Node, path: &str) -> Result<Vec<Vec<u8>>, BoundaryError> {
    let rows = node
        .required("terrain", path)?
        .array(&format!("{path}.terrain"))?;
    if rows.len() != 40 {
        return Err(BoundaryError::invalid(
            format!("{path}.terrain"),
            "invalid row count",
        ));
    }
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let path = format!("{path}.terrain[{i}]");
            let row = row.string(&path)?;
            if row.len() != 40 || !row.bytes().all(|b| b"gfmsrbdivpwc".contains(&b)) {
                return Err(BoundaryError::invalid(path, "unsupported terrain row"));
            }
            Ok(row.into_bytes())
        })
        .collect()
}
fn validate_resources(node: &Node, left: i32, top: i32, path: &str) -> Result<(), BoundaryError> {
    decode_resources(node, path)?;
    for field in ["map_resources", "tree_stumps"] {
        for (index, item) in node.required(field, path)?.array(path)?.iter().enumerate() {
            let path = format!("{path}.{field}[{index}]");
            let (x, y) = position(item, &path)?;
            if !inside(x, y, left, top) {
                return Err(BoundaryError::invalid(
                    path,
                    "position outside owning chunk",
                ));
            }
            if field == "map_resources" && item.required("kind", &path)?.string(&path)?.is_empty() {
                return Err(BoundaryError::invalid(
                    format!("{path}.kind"),
                    "empty resource kind",
                ));
            }
        }
    }
    Ok(())
}
fn inside(x: i32, y: i32, left: i32, top: i32) -> bool {
    (left..left + 40).contains(&x) && (top..top + 40).contains(&y)
}
fn scalar(value: impl serde::Serialize) -> Node {
    Node::Scalar(serde_json::value::to_raw_value(&value).unwrap())
}
fn set(node: &mut Node, key: &str, value: Node) {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    if let Some((_, old)) = fields.iter_mut().find(|(k, _)| k == key) {
        *old = value;
    } else {
        fields.push((key.into(), value));
    }
}
fn field_mut<'a>(node: &'a mut Node, name: &str) -> &'a mut Vec<Node> {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    let Node::Array(items) = &mut fields.iter_mut().find(|(key, _)| key == name).unwrap().1 else {
        unreachable!()
    };
    items
}
fn append(record: &mut Node, item: Node) {
    field_mut(record, "map_resources").push(item);
}
fn filter_resources(
    record: &mut Node,
    keep: impl Fn(i32, i32) -> bool,
) -> Result<(), BoundaryError> {
    for field in ["map_resources", "tree_stumps"] {
        // Every item was validated before editing; errors never escape with a partial result.
        field_mut(record, field).retain(|item| {
            let (x, y) = position(item, PATH).unwrap();
            keep(x, y)
        });
    }
    Ok(())
}
fn structure(kind: &str, x: i32, y: i32, village: &Village, biome: bool) -> Node {
    let mut node = Node::Object(vec![
        ("kind".into(), scalar(kind)),
        ("x".into(), scalar(x)),
        ("y".into(), scalar(y)),
    ]);
    if biome {
        set(
            &mut node,
            "biome",
            scalar((village.biome as char).to_string()),
        );
    }
    set(&mut node, "village", scalar(&village.name));
    node
}
fn add_life(
    record: &mut Node,
    rows: &mut [Vec<u8>],
    village: &Village,
    left: i32,
    top: i32,
) -> Result<(), BoundaryError> {
    let mut additions = Vec::new();
    for (kind, dx, dy, variant, label) in LIFE {
        let (x, y) = (village.x + dx, village.y + dy);
        if !inside(x, y, left, top) {
            continue;
        }
        let mut item = structure(kind, x, y, village, true);
        set(&mut item, "variant", scalar(variant));
        set(&mut item, "label", scalar(label));
        if variant == "barn" {
            let animals = match village.biome {
                b'b' => serde_json::json!({"chicken":3,"pig":1}),
                b'd' => serde_json::json!({"horse":2,"chicken":2}),
                b'i' => serde_json::json!({"sheep":3,"chicken":1}),
                _ => unreachable!(),
            };
            set(
                &mut item,
                "animals",
                Node::parse(&serde_json::to_vec(&animals).unwrap(), PATH)?,
            );
        }
        additions.push(item);
    }
    for (kind, dx, dy) in PLANTS {
        let (x, y) = (village.x + dx, village.y + dy);
        if !inside(x, y, left, top) {
            continue;
        }
        rows[(y - top) as usize][(x - left) as usize] =
            if kind == "water" || (kind == "tree" && village.biome == b'b') {
                village.biome
            } else {
                b'g'
            };
        let mut item = structure(kind, x, y, village, false);
        if kind == "tree" && village.biome == b'd' {
            set(&mut item, "species", scalar("palm"));
        }
        additions.push(item);
    }
    let occupied = additions
        .iter()
        .map(|n| position(n, PATH).unwrap())
        .collect::<BTreeSet<_>>();
    filter_resources(record, |x, y| !occupied.contains(&(x, y)))?;
    field_mut(record, "map_resources").extend(additions);
    Ok(())
}
// Preserve arbitrary-size integer and floating scalar spelling; never roundtrip
// retained source fields through serde_json::Value or a numeric representation.
fn write_node(node: &Node, output: &mut Vec<u8>) {
    match node {
        Node::Scalar(raw) => output.extend_from_slice(raw.get().as_bytes()),
        Node::Array(items) => {
            output.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i != 0 {
                    output.push(b',');
                }
                write_node(item, output);
            }
            output.push(b']');
        }
        Node::Object(fields) => {
            output.push(b'{');
            for (i, (key, item)) in fields.iter().enumerate() {
                if i != 0 {
                    output.push(b',');
                }
                output.extend_from_slice(serde_json::to_string(key).unwrap().as_bytes());
                output.push(b':');
                write_node(item, output);
            }
            output.push(b'}');
        }
    }
}

/// Independent private `swamp_village.apply` phase for version 1 only.
/// Metadata must already have been selected and migrated. Version 2 is rejected,
/// including completed records, rather than silently skipping organic geometry.
/// This never edits the imported archive, active records or input metadata.
/// ```compile_fail
/// use lantern_host::legacy::LegacySwampOverlay;
/// fn leak(view: &LegacySwampOverlay) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct LegacySwampOverlay {
    source: Vec<u8>,
    record: Vec<u8>,
}
impl LegacySwampOverlay {
    pub fn materialize(
        source: &[u8],
        villages: &[u8],
        key: ChunkKey,
        world_size: u32,
    ) -> Result<Self, BoundaryError> {
        const ROOT: &str = "$.legacy_swamp_overlay";
        if ![480, 1920].contains(&world_size)
            || key.x >= world_size / 40
            || key.y >= world_size / 40
        {
            return Err(BoundaryError::invalid(
                ROOT,
                "unsupported world bounds or chunk key",
            ));
        }
        let mut record = Node::parse(source, ROOT)?;
        let previous = version(&record, "swamp_version", 1, ROOT)?.unwrap_or(0);
        version(&record, "village_version", 3, ROOT)?;
        version(&record, "terrain_version", 7, ROOT)?;
        let (left, top) = (key.x as i32 * 40, key.y as i32 * 40);
        let mut terrain = rows(&record, ROOT)?;
        validate_resources(&record, left, top, ROOT)?;
        if let Some(clock) = record.optional("clock", ROOT)? {
            clock.nonnegative_number(&format!("{ROOT}.clock"))?;
        }
        let nodes = Node::parse(villages, &format!("{ROOT}.villages"))?;
        let mut selected = Vec::new();
        for (index, node) in nodes.array(&format!("{ROOT}.villages"))?.iter().enumerate() {
            let path = format!("{ROOT}.villages[{index}]");
            let village = Village::parse(node, index, world_size, ROOT)?;
            let swamp = version(node, "swamp_version", 1, &path)?;
            if village.expanded && swamp == Some(1) {
                selected.push(village);
            }
        }
        // Python's wrapper calls apply_legacy only for an expanded v1 village
        // when the record is older, passing only that first village and returning
        // immediately even if its square misses this chunk.
        if previous == 0 {
            let mut touched = false;
            for village in selected.into_iter().take(1) {
                let (vx, vy) = (village.x, village.y);
                if left > vx + 18 || left + 40 <= vx - 19 || top > vy + 18 || top + 40 <= vy - 19 {
                    continue;
                }
                touched = true;
                let mut cleared = BTreeSet::new();
                for y in top.max(vy - 19)..(top + 40).min(vy + 19) {
                    for x in left.max(vx - 19)..(left + 40).min(vx + 19) {
                        if (x - vx).abs().max((y - vy).abs()) > 9 {
                            terrain[(y - top) as usize][(x - left) as usize] = b'g';
                            cleared.insert((x, y));
                        }
                    }
                }
                for field in ["map_resources", "tree_stumps"] {
                    field_mut(&mut record, field).retain(|item| {
                        let point = position(item, ROOT).unwrap();
                        !cleared.contains(&point)
                            || item.optional("village", ROOT).unwrap().is_some_and(|n| {
                                n.string(ROOT).is_ok_and(|name| name == village.name)
                            })
                    });
                }
                for (index, (x, y)) in legacy_swamp_sites().into_iter().enumerate() {
                    let (kind, variant, label) = if (x, y) == (12, 4) {
                        ("village_hut", "tavern", "The Crooked Candle")
                    } else if index % 7 == 0 {
                        ("village_market", "stall", "Marsh market stall")
                    } else if index % 5 == 0 {
                        ("village_hut", "workshop", "Reedworker’s shed")
                    } else {
                        (
                            "village_hut",
                            ["reed_house", "net_house", "moss_house"][index % 3],
                            "Stilt cottage",
                        )
                    };
                    let mut items = vec![(kind, x, y, variant, label)];
                    let plant = ["ferns", "cattails", "mushrooms", "reeds"][index % 4];
                    let plant_label = ["Ferns", "Cattails", "Mushrooms", "Reeds"][index % 4];
                    items.extend([
                        ("village_decor", x + 1, y, "bench", "Bench"),
                        (
                            "village_decor",
                            x - 1,
                            y + 1,
                            "candle_lantern",
                            "Candle lantern",
                        ),
                        ("village_decor", x, y - 1, plant, plant_label),
                    ]);
                    for (kind, x, y, variant, label) in items {
                        append_swamp_item(
                            &mut record,
                            &village,
                            left,
                            top,
                            (kind, x, y, variant, label),
                        );
                    }
                }
                for (x, y) in [(10, 5), (14, 4), (14, 8), (-10, 8)] {
                    append_swamp_item(
                        &mut record,
                        &village,
                        left,
                        top,
                        ("village_decor", x, y, "barrels", "Ale barrels"),
                    );
                }
            }
            if touched {
                commit_rows(&mut record, &terrain);
                set(&mut record, "swamp_version", scalar(1));
            }
        }
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
fn legacy_swamp_sites() -> Vec<(i32, i32)> {
    let mut sites = Vec::new();
    for y in [-16, -12, 12, 16] {
        for x in [-16, -12, -8, -4, 4, 8, 12, 16] {
            sites.push((x, y));
        }
    }
    for x in [-12, 12] {
        for y in [-8, -4, 4, 8] {
            sites.push((x, y));
        }
    }
    sites.extend([(-16, -4), (16, -4)]);
    sites
}
fn append_swamp_item(
    record: &mut Node,
    village: &Village,
    left: i32,
    top: i32,
    (kind, x, y, variant, label): (&str, i32, i32, &str, &str),
) {
    let (x, y) = (village.x + x, village.y + y);
    if inside(x, y, left, top) {
        let mut item = structure(kind, x, y, village, true);
        set(&mut item, "variant", scalar(variant));
        set(&mut item, "label", scalar(label));
        append(record, item);
    }
}
