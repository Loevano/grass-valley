//! Independent ordinary world-metadata phase; deliberately stops before swamp.ensure.
use super::{LIFE, PLANTS, position, scalar, set, version, write_node};
use crate::{BoundaryError, legacy::json::Node};
use serde::Deserialize;

const PATH: &str = "$.ordinary_village_metadata";
#[derive(Deserialize)]
struct Theme {
    name: String,
    biome: String,
    dx: i32,
    dy: i32,
    names: Vec<String>,
    dialogue: String,
    residents: Vec<(String, String, i32, i32, String)>,
}

/// Private independent copy, not an ImportedWorld migration or playable load.
/// Runs only Python's ordinary placement/dialogue/expand/life phases. Callers
/// must separately implement swamp.ensure before integrating chunk overlays.
/// ```compile_fail
/// use lantern_host::legacy::OrdinaryVillageMetadata;
/// fn leak(view: &OrdinaryVillageMetadata) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct OrdinaryVillageMetadata {
    source: Vec<u8>,
    world: Vec<u8>,
}
impl OrdinaryVillageMetadata {
    pub fn materialize(source: &[u8]) -> Result<Self, BoundaryError> {
        let mut world = Node::parse(source, PATH)?;
        if world.optional("_chunks", PATH)?.is_some() {
            return Err(BoundaryError::invalid(
                format!("{PATH}._chunks"),
                "runtime manager unsupported",
            ));
        }
        let themes: Vec<Theme> =
            serde_json::from_str(include_str!("metadata_catalog.json")).unwrap();
        let towns = world
            .required("towns", PATH)?
            .object(&format!("{PATH}.towns"))?;
        let mut player_obstacles = Vec::new();
        for (i, (_, town)) in towns.iter().enumerate() {
            let path = format!("{PATH}.towns[{i}]");
            town.object(&path)?;
            for field in ["buildings", "roads"] {
                if let Some(items) = town.optional(field, &path)? {
                    for (j, item) in items.array(&format!("{path}.{field}"))?.iter().enumerate() {
                        player_obstacles.push(Rect::parse(item, &format!("{path}.{field}[{j}]"))?);
                    }
                }
            }
        }
        let landscape = field_mut(&mut world, "shared_map", PATH)?;
        let lp = format!("{PATH}.shared_map");
        let size = landscape
            .required("map_size", &lp)?
            .unsigned(&format!("{lp}.map_size"))?;
        if ![120, 480, 1920].contains(&size) {
            return Err(BoundaryError::invalid(
                format!("{lp}.map_size"),
                "unsupported world bounds",
            ));
        }
        version(landscape, "terrain_version", 7, &lp)?;
        let offset = landscape
            .optional("starter_offset", &lp)?
            .map(|n| n.tile(&format!("{lp}.starter_offset")))
            .transpose()?
            .unwrap_or(880) as i64;
        // Validate all inputs, including already completed/no-op metadata, before edits.
        if let Some(villages) = landscape.optional("villages", &lp)? {
            for (i, village) in villages
                .array(&format!("{lp}.villages"))?
                .iter()
                .enumerate()
            {
                validate_village(village, i, size, &themes)?;
            }
        }
        let mut npc_obstacles = Vec::new();
        if let Some(npcs) = landscape.optional("npcs", &lp)? {
            for (i, npc) in npcs.array(&format!("{lp}.npcs"))?.iter().enumerate() {
                let path = format!("{lp}.npcs[{i}]");
                let rect = Rect::parse(npc, &path)?;
                npc.required("name", &path)?
                    .string(&format!("{path}.name"))?;
                let village = npc
                    .optional("village", &path)?
                    .map(|n| n.string(&format!("{path}.village")))
                    .transpose()?;
                npc_obstacles.push((village, rect));
            }
        }
        if landscape.optional("npcs", &lp)?.is_some() {
            for npc in array_mut(field_mut(landscape, "npcs", &lp)?) {
                let name = npc.required("name", &lp)?.string(&lp)?;
                let village = npc
                    .optional("village", &lp)?
                    .map(|n| n.string(&lp))
                    .transpose()?;
                if let Some(theme) = themes.iter().find(|t| {
                    Some(t.name.as_str()) == village.as_deref() && t.names.contains(&name)
                }) {
                    set(npc, "dialogue", scalar(&theme.dialogue));
                }
            }
        }
        if landscape.optional("villages", &lp)?.is_none() {
            let mut selected: Vec<(i64, i64)> = Vec::new();
            let mut villages = Vec::new();
            for theme in &themes {
                let preferred = (offset + theme.dx as i64, offset + theme.dy as i64);
                let grid = (40..size as i64 - 40)
                    .step_by(40)
                    .flat_map(|y| (40..size as i64 - 40).step_by(40).map(move |x| (x, y)));
                for (x, y) in std::iter::once(preferred).chain(grid) {
                    if !(24 < x && x < size as i64 - 24 && 24 < y && y < size as i64 - 24)
                        || (offset - 24 <= x
                            && x < offset + 144
                            && offset - 24 <= y
                            && y < offset + 144)
                        || player_obstacles
                            .iter()
                            .chain(npc_obstacles.iter().map(|(_, r)| r))
                            .any(|r| r.intersects(x, y, 26))
                        || selected
                            .iter()
                            .any(|&(vx, vy)| (x - vx).abs() < 52 && (y - vy).abs() < 52)
                    {
                        continue;
                    }
                    selected.push((x, y));
                    let mut village = Node::Object(Vec::new());
                    for (key, value) in [
                        ("name", scalar(&theme.name)),
                        ("biome", scalar(&theme.biome)),
                        ("x", scalar(x)),
                        ("y", scalar(y)),
                    ] {
                        set(&mut village, key, value);
                    }
                    villages.push(village);
                    for (name, dx) in theme.names.iter().zip([-1, 1]) {
                        let mut trader = Node::Object(Vec::new());
                        for (key, value) in [
                            ("name", scalar(name)),
                            ("role", scalar(format!("{} village trader", theme.name))),
                            ("dialogue", scalar(&theme.dialogue)),
                            ("biome", scalar(&theme.biome)),
                            ("village", scalar(&theme.name)),
                            ("x", scalar(x + dx)),
                            ("y", scalar(y + 1)),
                        ] {
                            set(&mut trader, key, value);
                        }
                        append_npc(landscape, trader);
                    }
                    break;
                }
            }
            set(landscape, "villages", Node::Array(villages));
        }
        // Temporarily own the array so appending NPCs cannot alias the metadata.
        let mut villages = std::mem::replace(
            field_mut(landscape, "villages", &lp)?,
            Node::Array(Vec::new()),
        );
        for village in array_mut(&mut villages) {
            let name = village.required("name", &lp)?.string(&lp)?;
            let theme = themes.iter().find(|t| t.name == name).unwrap();
            let (x, y) = position(village, &lp)?;
            let (x, y) = (x as i64, y as i64);
            let layout = version(village, "layout_version", 2, &lp)?;
            if !matches!(layout, Some(1 | 2)) {
                // Include NPCs appended by earlier expansions, in Python village order.
                let mut blocked = player_obstacles.iter().any(|r| r.intersects(x, y, 10));
                if let Some(npcs) = landscape.optional("npcs", &lp)? {
                    for npc in npcs.array(&lp)? {
                        let owner = npc
                            .optional("village", &lp)?
                            .map(|n| n.string(&lp))
                            .transpose()?;
                        if owner.as_deref() != Some(&name)
                            && Rect::parse(npc, &lp)?.intersects(x, y, 10)
                        {
                            blocked = true;
                        }
                    }
                }
                if blocked {
                    set(village, "layout_version", scalar(1));
                } else {
                    for (name, role, dx, dy, dialogue) in &theme.residents {
                        append_npc(
                            landscape,
                            npc(name, role, dialogue, theme, x + *dx as i64, y + *dy as i64),
                        );
                    }
                    set(village, "layout_version", scalar(2));
                }
            }
            if village.optional("life_version", &lp)?.is_none()
                && version(village, "layout_version", 2, &lp)? == Some(2)
            {
                let blocked = PLANTS
                    .iter()
                    .map(|&(_, dx, dy)| (dx, dy))
                    .chain(LIFE.iter().map(|&(_, dx, dy, _, _)| (dx, dy)))
                    .any(|(dx, dy)| {
                        player_obstacles
                            .iter()
                            .any(|r| r.contains(x + dx as i64, y + dy as i64))
                    });
                set(village, "life_version", scalar(if blocked { 0 } else { 1 }));
            }
        }
        set(landscape, "villages", villages);
        let mut output = Vec::new();
        write_node(&world, &mut output);
        Ok(Self {
            source: source.to_vec(),
            world: output,
        })
    }
    pub fn private_source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn private_world_bytes(&self) -> &[u8] {
        &self.world
    }
}
fn validate_village(
    node: &Node,
    i: usize,
    size: u64,
    themes: &[Theme],
) -> Result<(), BoundaryError> {
    let path = format!("{PATH}.shared_map.villages[{i}]");
    let name = node
        .required("name", &path)?
        .string(&format!("{path}.name"))?;
    let biome = node
        .required("biome", &path)?
        .string(&format!("{path}.biome"))?;
    if !themes.iter().any(|t| t.name == name && t.biome == biome) {
        return Err(BoundaryError::invalid(
            &path,
            "unsupported village theme or biome",
        ));
    }
    let (x, y) = position(node, &path)?;
    if x < 0 || y < 0 || x as u64 >= size || y as u64 >= size {
        return Err(BoundaryError::invalid(&path, "village outside world"));
    }
    version(node, "layout_version", 2, &path)?;
    version(node, "life_version", 1, &path)?;
    version(node, "swamp_version", 2, &path)?;
    Ok(())
}
fn npc(name: &str, role: &str, dialogue: &str, theme: &Theme, x: i64, y: i64) -> Node {
    let mut node = Node::Object(Vec::new());
    for (key, value) in [
        ("name", scalar(name)),
        ("role", scalar(role)),
        ("x", scalar(x)),
        ("y", scalar(y)),
        ("dialogue", scalar(dialogue)),
        ("biome", scalar(&theme.biome)),
        ("village", scalar(&theme.name)),
    ] {
        set(&mut node, key, value);
    }
    node
}
fn append_npc(landscape: &mut Node, npc: Node) {
    let Node::Object(fields) = landscape else {
        unreachable!()
    };
    if !fields.iter().any(|(key, _)| key == "npcs") {
        fields.push(("npcs".into(), Node::Array(Vec::new())));
    }
    let value = &mut fields.iter_mut().find(|(key, _)| key == "npcs").unwrap().1;
    array_mut(value).push(npc);
}
fn field_mut<'a>(node: &'a mut Node, key: &str, path: &str) -> Result<&'a mut Node, BoundaryError> {
    node.required(key, path)?;
    let Node::Object(fields) = node else {
        unreachable!()
    };
    Ok(&mut fields.iter_mut().find(|(k, _)| k == key).unwrap().1)
}
fn array_mut(node: &mut Node) -> &mut Vec<Node> {
    let Node::Array(items) = node else {
        unreachable!()
    };
    items
}
struct Rect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}
impl Rect {
    fn parse(node: &Node, path: &str) -> Result<Self, BoundaryError> {
        let (x, y) = position(node, path)?;
        let (w, h) = if let Some(fp) = node.optional("footprint", path)? {
            let fp = fp.array(&format!("{path}.footprint"))?;
            if fp.len() != 2 {
                return Err(BoundaryError::invalid(
                    format!("{path}.footprint"),
                    "expected two dimensions",
                ));
            }
            (
                fp[0].tile(&format!("{path}.footprint[0]"))?,
                fp[1].tile(&format!("{path}.footprint[1]"))?,
            )
        } else {
            (1, 1)
        };
        Ok(Self {
            x: x as i64,
            y: y as i64,
            w: w as i64,
            h: h as i64,
        })
    }
    fn contains(&self, x: i64, y: i64) -> bool {
        self.w > 0
            && self.h > 0
            && self.x <= x
            && x < self.x + self.w
            && self.y <= y
            && y < self.y + self.h
    }
    fn intersects(&self, x: i64, y: i64, r: i64) -> bool {
        self.w > 0
            && self.h > 0
            && self.x <= x + r
            && self.x + self.w > x - r
            && self.y <= y + r
            && self.y + self.h > y - r
    }
}
