//! Explicit world-metadata phase of swamp_village.ensure; never a playable load.
use super::{
    metadata::{Rect, Theme, array_mut, field_mut, validate_village},
    position, scalar, set,
    swamp::layout,
    version, write_node,
};
use crate::{BoundaryError, legacy::json::Node};

const ROOT: &str = "$.swamp_village_metadata";
const NAMES: &[&str] = &[
    "Aster", "Burdock", "Cress", "Dill", "Elva", "Fennel", "Greta", "Hollis", "Ivo", "Juniper",
    "Kel", "Lark", "Mallow", "Nettle", "Odo", "Perrin", "Quill", "Rowan", "Sorrel", "Thistle",
];
const ROLES: &[&str] = &[
    "basket weaver",
    "reed cutter",
    "marsh herbalist",
    "marsh fisher",
    "candle maker",
];
const SLEEPERS: &[(&str, i32, i32)] = &[
    ("Bramble", 13, 5),
    ("Dewey", 11, 6),
    ("Gus", 13, 9),
    ("Hob", -11, 9),
];

/// Deferred authoritative effects, in Python village iteration order. For EACH
/// ordinal, apply SwampOverlay with only that upgraded village to every active
/// and camera-cached record; rebind landscape map_resources/tree_stumps from
/// active records, then invalidate membership and resource_index. Merely merging
/// all villages into one apply call changes mixed-version behavior. This plan
/// performs no cache writes, installs no records and carries no private world data.
#[derive(Debug, PartialEq, Eq)]
pub struct SwampCachePlan {
    pub village_ordinals: Vec<usize>,
}

/// Private, independent copy of the swamp ensure phase AFTER ordinary metadata.
/// Runtime _chunks managers reject; callers must consume the deferred cache plan
/// at the later authoritative lifecycle boundary. No automatic ImportedWorld use.
/// ```compile_fail
/// use lantern_host::legacy::SwampVillageMetadata;
/// fn leak(view: &SwampVillageMetadata) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct SwampVillageMetadata {
    source: Vec<u8>,
    world: Vec<u8>,
    plan: SwampCachePlan,
}
impl SwampVillageMetadata {
    pub fn materialize(source: &[u8]) -> Result<Self, BoundaryError> {
        let mut world = Node::parse(source, ROOT)?;
        if world.optional("_chunks", ROOT)?.is_some() {
            return Err(BoundaryError::invalid(
                format!("{ROOT}._chunks"),
                "runtime manager unsupported",
            ));
        }
        let tp = format!("{ROOT}.towns");
        let mut obstacles = Vec::new();
        for (i, (_, town)) in world
            .required("towns", ROOT)?
            .object(&tp)?
            .iter()
            .enumerate()
        {
            let path = format!("{tp}[{i}]");
            town.object(&path)?;
            for field in ["buildings", "roads"] {
                if let Some(items) = town.optional(field, &path)? {
                    for (j, item) in items.array(&format!("{path}.{field}"))?.iter().enumerate() {
                        obstacles.push(Rect::parse(item, &format!("{path}.{field}[{j}]"))?);
                    }
                }
            }
            if let Some(visits) = town.optional("local_visits", &path)? {
                visits.object(&format!("{path}.local_visits"))?;
            }
        }
        let landscape = field_mut(&mut world, "shared_map", ROOT)?;
        let lp = format!("{ROOT}.shared_map");
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
        let themes: Vec<Theme> =
            serde_json::from_str(include_str!("metadata_catalog.json")).unwrap();
        for (i, village) in landscape
            .required("villages", &lp)?
            .array(&format!("{lp}.villages"))?
            .iter()
            .enumerate()
        {
            validate_village(village, &format!("{lp}.villages[{i}]"), size, &themes)?;
        }
        if let Some(npcs) = landscape.optional("npcs", &lp)? {
            for (i, npc) in npcs.array(&format!("{lp}.npcs"))?.iter().enumerate() {
                validate_npc(npc, &format!("{lp}.npcs[{i}]"))?;
            }
        }
        // Validation completes before mutation, including organic/no-op worlds.
        let mut villages = std::mem::replace(
            field_mut(landscape, "villages", &lp)?,
            Node::Array(Vec::new()),
        );
        let mut plan = SwampCachePlan {
            village_ordinals: Vec::new(),
        };
        let mut transfers = Vec::new();
        for (ordinal, village) in array_mut(&mut villages).iter_mut().enumerate() {
            let path = format!("{lp}.villages[{ordinal}]");
            if village.required("name", &path)?.string(&path)? != "Reedhaven"
                || version(village, "layout_version", 2, &path)? != Some(2)
                || version(village, "life_version", 1, &path)? != Some(1)
                || version(village, "swamp_version", 2, &path)? == Some(2)
            {
                continue;
            }
            let (vx, vy) = position(village, &path)?;
            let mut touched = layout()
                .footprint
                .union(&layout().water)
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            if version(village, "swamp_version", 2, &path)? == Some(1) {
                touched.extend((-19..=18).flat_map(|y| (-19..=18).map(move |x| (x, y))));
            }
            let npcs = landscape
                .required("npcs", &lp)?
                .array(&format!("{lp}.npcs"))?;
            let mut foreign = Vec::new();
            for npc in npcs {
                if npc
                    .optional("village", &lp)?
                    .map(|v| v.string(&lp))
                    .transpose()?
                    .as_deref()
                    != Some("Reedhaven")
                {
                    foreign.push(Rect::parse(npc, &lp)?);
                }
            }
            if touched.iter().any(|&(x, y)| {
                let (x, y) = (vx as i64 + x as i64, vy as i64 + y as i64);
                x < 0
                    || y < 0
                    || x >= size as i64
                    || y >= size as i64
                    || obstacles.iter().chain(&foreign).any(|r| r.contains(x, y))
            }) {
                continue;
            }
            // Python dict comprehension: last duplicate value, FIRST key order.
            let mut residents: Vec<(String, usize)> = Vec::new();
            for (i, npc) in npcs.iter().enumerate() {
                if npc
                    .optional("village", &lp)?
                    .map(|v| v.string(&lp))
                    .transpose()?
                    .as_deref()
                    == Some("Reedhaven")
                {
                    let name = npc.required("name", &lp)?.string(&lp)?;
                    if let Some(entry) = residents.iter_mut().find(|r| r.0 == name) {
                        entry.1 = i;
                    } else {
                        residents.push((name, i));
                    }
                }
            }
            let npcs = array_mut(field_mut(landscape, "npcs", &lp)?);
            let mut moved: Vec<(String, String)> = Vec::new();
            for (i, &name) in NAMES.iter().enumerate() {
                let existing = residents.iter().find(|r| r.0 == name).map(|r| r.1);
                let index = match existing {
                    Some(index) => index,
                    None => {
                        npcs.push(object(vec![
                            ("name", scalar(name)),
                            ("role", scalar(ROLES[i % ROLES.len()])),
                            ("biome", scalar("b")),
                            ("village", scalar("Reedhaven")),
                            (
                                "dialogue",
                                scalar(if i % 2 == 1 {
                                    "Crowded at the Crooked Candle!"
                                } else {
                                    "Reed roofs, willow boats, warm ale."
                                }),
                            ),
                        ]));
                        npcs.len() - 1
                    }
                };
                let npc = &mut npcs[index];
                let old = if existing.is_some() {
                    Some(local_key(npc)?)
                } else {
                    None
                };
                let (x, y) = layout().homes[i];
                set(npc, "x", scalar(vx + x));
                set(npc, "y", scalar(vy + y));
                set(npc, "ambient_stop", scalar([x, y]));
                remove(npc, "routine");
                if let Some(old) = old {
                    let new = local_key(npc)?;
                    if let Some(entry) = moved.iter_mut().find(|r| r.0 == old) {
                        entry.1 = new;
                    } else {
                        moved.push((old, new));
                    }
                }
            }
            for &(name, x, y) in SLEEPERS {
                if !residents.iter().any(|r| r.0 == name) {
                    npcs.push(object(vec![
                        ("name", scalar(name)),
                        ("role", scalar("sleeping tavern patron")),
                        ("x", scalar(vx + x)),
                        ("y", scalar(vy + y)),
                        ("biome", scalar("b")),
                        ("village", scalar("Reedhaven")),
                        ("pose", scalar("passed_out")),
                        ("dialogue", scalar("Zzz… too much marsh ale…")),
                    ]));
                }
            }
            // Town writes are independent of routine repair, and remain ordered
            // between village passes (including collisions and equal keys).
            transfers.push(moved);
            set(village, "swamp_version", scalar(2));
            for (_, index) in residents {
                let npc = &mut npcs[index];
                if let Some(state) = npc.optional("routine", &lp)? {
                    if state.is_null() || state.object(&lp)?.is_empty() {
                        continue;
                    }
                } else {
                    continue;
                }
                let state = field_mut(npc, "routine", &lp)?;
                let x =
                    number(state.required("x", &lp)?, &lp)?.round_ties_even() as i64 - vx as i64;
                let y =
                    number(state.required("y", &lp)?, &lp)?.round_ties_even() as i64 - vy as i64;
                if !layout()
                    .roads
                    .iter()
                    .any(|&((rx, ry), _)| (rx as i64, ry as i64) == (x, y))
                {
                    let ((rx, ry), _) = layout()
                        .roads
                        .iter()
                        .min_by_key(|&&((rx, ry), _)| (rx as i64 - x).abs() + (ry as i64 - y).abs())
                        .unwrap();
                    set(state, "x", scalar(vx + rx));
                    set(state, "y", scalar(vy + ry));
                }
                let origin = Node::Array(vec![
                    state.required("x", &lp)?.clone(),
                    state.required("y", &lp)?.clone(),
                ]);
                set(state, "path", Node::Array(Vec::new()));
                set(state, "phase", scalar("idle"));
                set(state, "wait", scalar(0));
                set(state, "origin", origin);
            }
            plan.village_ordinals.push(ordinal);
        }
        set(landscape, "villages", villages);
        // Transfers can be applied after landscape processing: no subsequent
        // village reads visits; applying each ordered map retains Python order.
        for moved in transfers {
            let Node::Object(towns) = field_mut(&mut world, "towns", ROOT)? else {
                unreachable!()
            };
            for (_, town) in towns {
                if town.optional("local_visits", ROOT)?.is_none() {
                    continue;
                }
                let Node::Object(visits) = field_mut(town, "local_visits", ROOT)? else {
                    unreachable!()
                };
                for (old, new) in &moved {
                    if let Some(index) = visits.iter().position(|r| &r.0 == old) {
                        let (_, value) = visits.remove(index);
                        if let Some(entry) = visits.iter_mut().find(|r| &r.0 == new) {
                            entry.1 = value;
                        } else {
                            visits.push((new.clone(), value));
                        }
                    }
                }
            }
        }
        let mut output = Vec::new();
        write_node(&world, &mut output);
        Ok(Self {
            source: source.to_vec(),
            world: output,
            plan,
        })
    }
    pub fn private_source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn private_world_bytes(&self) -> &[u8] {
        &self.world
    }
    pub fn cache_plan(&self) -> &SwampCachePlan {
        &self.plan
    }
}
fn object(fields: Vec<(&str, Node)>) -> Node {
    Node::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn remove(node: &mut Node, key: &str) {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    fields.retain(|(k, _)| k != key);
}
fn local_key(npc: &Node) -> Result<String, BoundaryError> {
    let (x, y) = position(npc, ROOT)?;
    Ok(format!(
        "{}:{x}:{y}",
        npc.required("name", ROOT)?.string(ROOT)?
    ))
}
fn number(node: &Node, path: &str) -> Result<f64, BoundaryError> {
    let Node::Scalar(raw) = node else {
        return Err(BoundaryError::invalid(
            path,
            "expected finite routine coordinate",
        ));
    };
    raw.get()
        .parse::<f64>()
        .ok()
        .filter(|v| {
            v.is_finite()
                && v.round_ties_even() >= i32::MIN as f64
                && v.round_ties_even() <= i32::MAX as f64
        })
        .ok_or_else(|| BoundaryError::invalid(path, "expected finite i32-range routine coordinate"))
}
fn pair(node: &Node, path: &str) -> Result<(), BoundaryError> {
    let pair = node.array(path)?;
    if pair.len() != 2 {
        return Err(BoundaryError::invalid(path, "expected coordinate pair"));
    }
    for (i, v) in pair.iter().enumerate() {
        number(v, &format!("{path}[{i}]"))?;
    }
    Ok(())
}
fn validate_npc(npc: &Node, path: &str) -> Result<(), BoundaryError> {
    Rect::parse(npc, path)?;
    npc.required("name", path)?
        .string(&format!("{path}.name"))?;
    if let Some(v) = npc.optional("village", path)? {
        v.string(&format!("{path}.village"))?;
    }
    if let Some(v) = npc.optional("ambient_stop", path)? {
        pair(v, &format!("{path}.ambient_stop"))?;
    }
    if let Some(state) = npc.optional("routine", path)? {
        let rp = format!("{path}.routine");
        if state.is_null() {
            return Ok(());
        }
        if !state.object(&rp)?.is_empty() {
            for field in ["x", "y"] {
                number(state.required(field, &rp)?, &format!("{rp}.{field}"))?;
            }
            if let Some(v) = state.optional("path", &rp)? {
                for (i, v) in v.array(&format!("{rp}.path"))?.iter().enumerate() {
                    pair(v, &format!("{rp}.path[{i}]"))?;
                }
            }
            if let Some(v) = state.optional("origin", &rp)? {
                pair(v, &format!("{rp}.origin"))?;
            }
            if let Some(v) = state.optional("phase", &rp)? {
                v.string(&format!("{rp}.phase"))?;
            }
            if let Some(v) = state.optional("wait", &rp)? {
                v.nonnegative_number(&format!("{rp}.wait"))?;
            }
        }
    }
    Ok(())
}
