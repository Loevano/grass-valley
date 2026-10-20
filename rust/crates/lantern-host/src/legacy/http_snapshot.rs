//! Explicit public terrain projection. Private ordered JSON never implements Serialize.
//! Unknown payload/resource fields refuse coverage; private chunk envelope fields
//! (clock, migration markers and extensions) are deliberately outside the payload.
use super::{
    ChunkKey, ChunkLifecycle, ImportedChunk, ImportedRegistry, TerrainArchive, json::Node,
};
use crate::{BoundaryError, http::SnapshotQuery};
use blake2::{
    Blake2bVar,
    digest::{Update, VariableOutput},
};
use lantern_core::ids::WorldId;
use serde::Serialize;
use serde_json::value::RawValue;
use std::collections::BTreeSet;

mod metadata;

const PATH: &str = "$.http_terrain";
fn invalid(reason: &'static str) -> BoundaryError {
    BoundaryError::invalid(PATH, reason)
}
fn raw(node: &Node) -> Vec<u8> {
    let mut out = Vec::new();
    super::village_overlay::write_node(node, &mut out);
    out
}
fn scalar<T: Serialize>(value: T) -> Node {
    Node::parse(&serde_json::to_vec(&value).unwrap(), PATH).unwrap()
}
fn set(node: &mut Node, key: &str, value: Node) {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    if let Some((_, existing)) = fields.iter_mut().find(|(k, _)| k == key) {
        *existing = value;
    } else {
        fields.push((key.into(), value));
    }
}

/// Serialize-only, already projected public bytes. There is no deserialize/from
/// arbitrary JSON constructor, and no private credentials or archive extensions.
#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct PublicFrame(Box<RawValue>);

pub(crate) struct TerrainView {
    imported: ImportedRegistry,
    world: WorldId,
    base: Node,
    chunked: bool,
    size: u32,
    sandbox: bool,
}
impl TerrainView {
    /// Admit a frozen shared landscape with an explicit public schema. Populated
    /// presentation must agree with native construction/population/ledger owners;
    /// feature state without a checked adapter refuses admission.
    pub fn new(
        native: &crate::persistence::RestoredWorld,
        archive: &TerrainArchive,
    ) -> Result<Self, BoundaryError> {
        let world = native.model.model().id();
        let mut payload = Node::parse(archive.private_world_bytes(), PATH)?;
        if let Some(village) = &native.village {
            let mut live = Node::parse(&village.private_world_bytes(), PATH)?;
            check_world(&live)?;
            let original_map = payload.required("shared_map", PATH)?.clone();
            let live_map = live.required("shared_map", PATH)?.clone();
            let mut merged_map = original_map.clone();
            let mut unchanged_map = live_map.clone();
            for key in ["npcs", "villages"] {
                let original = original_map.required(key, PATH)?.clone();
                set(&mut merged_map, key, live_map.required(key, PATH)?.clone());
                set(&mut unchanged_map, key, original);
            }
            set(&mut live, "shared_map", unchanged_map);
            if raw(&live) != raw(&payload) {
                return Err(invalid("village owner has unprojected state"));
            }
            set(&mut payload, "shared_map", merged_map);
        }
        // Validate saved routine semantics through the existing read-only owner;
        // its opaque extensions still require the separate public allowlist.
        super::VillageLife::restore(world.clone(), &raw(&payload))?;
        check_world(&payload)?;
        let chunked = super::chunks::metadata(&payload, PATH)?.is_some();
        check_map(payload.required("shared_map", PATH)?, chunked)?;
        // Even current rosters repair two historical clothing dialogues on GET.
        let roster =
            super::NpcRoster::materialize(&raw(payload.required("shared_map", PATH)?), &[], &[])?;
        let map = Node::parse(roster.private_town_bytes(), PATH)?;
        set(&mut payload, "shared_map", map);
        let source = format!(
            "{{\"worlds\":{{{}:{{\"name\":\"read view\",\"world\":{},\"tokens\":{{}}}}}},\"active_world\":{}}}",
            serde_json::to_string(world.as_str()).unwrap(),
            std::str::from_utf8(&raw(&payload)).map_err(|_| invalid("world encoding"))?,
            serde_json::to_string(world.as_str()).unwrap()
        );
        // Both archive records and registry ownership were validated by native
        // restore. This in-memory bridge has no paths, discovery or file writes.
        let mut imported = super::parse_registry(Some(source.as_bytes()), None)?;
        imported.worlds[0].chunks = archive
            .records()
            .iter()
            .map(|(k, v)| ImportedChunk::from_validated_archive(*k, v.clone()))
            .collect();
        let mut sources = Vec::new();
        let mut work_terrain = std::collections::BTreeMap::new();
        let base = if chunked {
            let mut owner = ChunkLifecycle::prepare(&imported, world, &[], &[])?;
            owner.activate()?;
            for (key, _) in payload.required("towns", PATH)?.object(PATH)? {
                let scene = owner.founder_scene(key)?;
                if !scene.walkable(scene.state()?.position)? {
                    return Err(invalid("founder repair requires missing coverage"));
                }
            }
            for key in owner.active_keys() {
                let record = Node::parse(owner.private_active_bytes(key).unwrap(), PATH)?;
                sources.extend(metadata::work_sources(&record)?);
                work_terrain.extend(metadata::work_terrain(
                    &record,
                    lantern_core::units::Tile {
                        x: key.x as i32 * 40,
                        y: key.y as i32 * 40,
                    },
                )?);
            }
            owner.private_world_bytes()
        } else {
            sources.extend(metadata::work_sources(
                payload.required("shared_map", PATH)?,
            )?);
            work_terrain.extend(metadata::work_terrain(
                payload.required("shared_map", PATH)?,
                lantern_core::units::Tile { x: 0, y: 0 },
            )?);
            for (key, _) in payload.required("towns", PATH)?.object(PATH)? {
                let scene = super::PrivateFounderWorldCopy::decode(&raw(&payload), key)?;
                if !scene.walkable(scene.state()?.position)? {
                    return Err(invalid("founder repair requires missing coverage"));
                }
            }
            raw(&payload)
        };
        let base = Node::parse(&base, PATH)?;
        check_world(&base)?;
        metadata::towns(native, &payload, &sources, &work_terrain)?;
        metadata::towns(native, &base, &sources, &work_terrain)?;
        let map = base.required("shared_map", PATH)?;
        check_map(map, chunked)?;
        let size = map.required("map_size", PATH)?.unsigned(PATH)? as u32;
        // The frozen view has a complete public schema, including owner overlays.
        for npc in map.required("npcs", PATH)?.array(PATH)? {
            for axis in ["x", "y"] {
                let point = npc.required(axis, PATH)?.tile(PATH)?;
                if point < 0 || point as u32 >= size {
                    return Err(invalid("NPC outside world"));
                }
            }
        }
        let sandbox = base
            .optional("sandbox", PATH)?
            .map(|n| n.boolean(PATH))
            .transpose()?
            .unwrap_or(false);
        // Refuse unsupported sleeping payloads before publishing any endpoint.
        // Chunk envelope extensions remain private and do not affect versions.
        for (key, bytes) in archive.records() {
            project_record(bytes, *key)?;
        }
        Ok(Self {
            imported,
            world: world.clone(),
            base,
            chunked,
            size,
            sandbox,
        })
    }
    pub fn town_count(&self) -> usize {
        self.base
            .required("towns", PATH)
            .unwrap()
            .object(PATH)
            .unwrap()
            .len()
    }
    pub fn players(&self) -> serde_json::Value {
        let entries = self
            .base
            .required("towns", PATH)
            .unwrap()
            .object(PATH)
            .unwrap();
        let players = Node::Object(
            entries
                .iter()
                .filter_map(|(key, town)| {
                    town.optional("player", PATH)
                        .unwrap()
                        .map(|p| (key.clone(), p.clone()))
                })
                .collect(),
        );
        serde_json::from_slice(&raw(&players)).unwrap()
    }
    pub fn lanterns(&self) -> serde_json::Value {
        serde_json::from_slice(&raw(
            &metadata::lantern_snapshot(&self.base).expect("admitted lantern snapshot")
        ))
        .unwrap()
    }
    pub fn sandbox(&self) -> bool {
        self.sandbox
    }
    pub fn validate_query(&self, query: &SnapshotQuery) -> Result<(), &'static str> {
        if self.chunked
            && query.chunks.iter().any(|key| {
                let key = key_from_text(key);
                key.x >= self.size / 40 || key.y >= self.size / 40
            })
        {
            return Err("Map chunk outside the world.");
        }
        Ok(())
    }
    pub fn frame(
        &self,
        query: &SnapshotQuery,
        name: &str,
        sampled_at: f64,
    ) -> Result<PublicFrame, BoundaryError> {
        let mut world = self.base.clone();
        let ready = metadata::hall_ready(&world)?;
        if self.chunked {
            let keys: Vec<_> = query.chunks.iter().map(|k| key_from_text(k)).collect();
            let mut owner = ChunkLifecycle::prepare(&self.imported, &self.world, &[], &[])?;
            owner.activate()?;
            let copy = owner.snapshot(&keys)?;
            let matching = query
                .world_id
                .as_deref()
                .is_none_or(|id| id == self.world.as_str());
            let known: BTreeSet<_> = if matching {
                query.known.iter().cloned().collect()
            } else {
                BTreeSet::new()
            };
            let mut terrain = Vec::new();
            let mut resources = Vec::new();
            let mut visible: Vec<_> = copy.keys().map(key_text).collect();
            visible.sort(); // Python sorts textual keys, not numeric coordinates.
            for key in &visible {
                let record = copy.private_record_bytes(key_from_text(key)).unwrap();
                let (rows, mut payload) = project_record(record, key_from_text(key))?;
                if !known.contains(key) {
                    terrain.push((key.clone(), rows));
                }
                let version = resource_version(&payload)?;
                if !matching || query.resources.get(key) != Some(&version) {
                    set(&mut payload, "version", scalar(version));
                    resources.push((key.clone(), payload));
                }
            }
            let map = world.required("shared_map", PATH)?;
            let mut map = map.clone();
            let Node::Object(fields) = &mut map else {
                unreachable!()
            };
            fields.retain(|(k, _)| {
                !["terrain", "map_resources", "tree_stumps"].contains(&k.as_str())
            });
            set(&mut map, "terrain_chunks", Node::Object(terrain));
            set(&mut map, "resource_chunks", Node::Object(resources));
            set(&mut map, "visible_chunks", scalar(visible));
            set(&mut world, "shared_map", map);
        }
        set(&mut world, "world_id", scalar(self.world.as_str()));
        set(&mut world, "name", scalar(name));
        set(&mut world, "players_sampled_at", scalar(sampled_at));
        set(
            &mut world,
            "simulation_clock",
            scalar(serde_json::json!({"tick_seconds":3,"next_tick_in":3})),
        );
        set(&mut world, "hall_ready", ready);
        for key in ["hall_upgrades", "local_stories", "quarry_catalog"] {
            set(&mut world, key, scalar(&crate::http::catalogs()[key]));
        }
        set(
            &mut world,
            "progression",
            scalar(crate::resources::public_catalog()),
        );
        Ok(PublicFrame(
            RawValue::from_string(String::from_utf8(raw(&world)).unwrap())
                .map_err(|_| invalid("public frame encoding"))?,
        ))
    }
}
fn key_from_text(text: &str) -> ChunkKey {
    let (x, y) = text.split_once(',').expect("validated query key");
    ChunkKey {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
    }
}
fn key_text(key: ChunkKey) -> String {
    format!("{},{}", key.x, key.y)
}
fn check_world(world: &Node) -> Result<(), BoundaryError> {
    for (key, value) in world.object(PATH)? {
        match key.as_str() {
            "towns" => {
                value.object(PATH)?;
            }
            "lanterns" | "quarry" => {
                metadata::adventure(value, key == "lanterns", world.required("towns", PATH)?)?
            }
            "shared_map" => {}
            "sandbox" => {
                value.boolean(PATH)?;
            }
            "town_sites" => {
                for point in value.array(PATH)? {
                    check_point(point)?;
                }
            }
            _ => return Err(invalid("missing world field coverage")),
        }
    }
    let map = world.required("shared_map", PATH)?;
    map.object(PATH)?;
    let size = map.required("map_size", PATH)?.unsigned(PATH)?;
    let sites = world.required("town_sites", PATH)?.array(PATH)?;
    if !(1..=4).contains(&sites.len()) {
        return Err(invalid("missing starter-window coverage"));
    }
    for site in sites {
        for axis in ["x", "y"] {
            let coordinate = site.required(axis, PATH)?.tile(PATH)?;
            if coordinate < 0 || coordinate as u64 >= size {
                return Err(invalid("starter site outside world"));
            }
        }
    }
    Ok(())
}
fn check_point(point: &Node) -> Result<(), BoundaryError> {
    for (k, v) in point.object(PATH)? {
        if !["x", "y"].contains(&k.as_str()) {
            return Err(invalid("missing point field coverage"));
        }
        v.tile(PATH)?;
    }
    point.required("x", PATH)?.tile(PATH)?;
    point.required("y", PATH)?.tile(PATH)?;
    Ok(())
}
fn check_map(map: &Node, chunked: bool) -> Result<(), BoundaryError> {
    for (key, value) in map.object(PATH)? {
        match key.as_str() {
            "npcs" => {
                for npc in value.array(PATH)? {
                    metadata::npc(npc)?;
                }
            }
            "villages" => {
                for village in value.array(PATH)? {
                    metadata::village(village)?;
                }
            }
            "forage_halls" => {
                for point in value.array(PATH)? {
                    let pair = point.array(PATH)?;
                    if pair.len() != 2 {
                        return Err(invalid("forage hall pair"));
                    }
                    for n in pair {
                        n.tile(PATH)?;
                    }
                }
            }
            "terrain_seed" => {
                if value.string(PATH).is_err() {
                    value.unsigned(PATH)?;
                }
            }
            "chunked" => {
                if value.boolean(PATH).is_err() {
                    value.unsigned(PATH)?;
                }
            }
            "map_size"
            | "chunk_size"
            | "starter_offset"
            | "terrain_version"
            | "npc_roster_version"
            | "npc_placement_version"
            | "forage_version"
            | "industry_version"
            | "mineral_version" => {
                value.unsigned(PATH)?;
            }
            "chunk_clock" => {
                super::clock::Clock::parse(value, PATH)?;
            }
            "terrain" | "map_resources" | "tree_stumps" => {
                value.array(PATH)?;
            }
            _ => return Err(invalid("missing landscape field coverage")),
        }
    }
    if !chunked {
        let size = map.required("map_size", PATH)?.unsigned(PATH)?;
        if size == 0 || size > 1920 {
            return Err(invalid("ordinary map bounds"));
        }
        let rows = map.required("terrain", PATH)?.array(PATH)?;
        if rows.len() as u64 != size {
            return Err(invalid("ordinary terrain height"));
        }
        for row in rows {
            let row = row.string(PATH)?;
            if row.len() as u64 != size || !row.bytes().all(|c| b"gfmsrbdivpwc".contains(&c)) {
                return Err(invalid("ordinary terrain row"));
            }
        }
        for field in ["map_resources", "tree_stumps"] {
            for item in map.required(field, PATH)?.array(PATH)? {
                check_resource_in(item, field == "tree_stumps", (0, 0), size as i32)?;
            }
        }
        for (key, version) in [("industry_version", 2), ("mineral_version", 1)] {
            if map.required(key, PATH)?.unsigned(PATH)? != version {
                return Err(invalid("ordinary landscape migration coverage"));
            }
        }
    }
    // Never infer that an unversioned save has completed unsupported migrations.
    for (field, version) in [
        ("npc_roster_version", 3),
        ("npc_placement_version", 1),
        ("forage_version", 1),
    ] {
        if map.required(field, PATH)?.unsigned(PATH)? != version {
            return Err(invalid("missing landscape migration coverage"));
        }
    }
    map.required("npcs", PATH)?;
    if chunked {
        map.required("villages", PATH)?;
    }
    Ok(())
}
fn project_record(bytes: &[u8], key: ChunkKey) -> Result<(Node, Node), BoundaryError> {
    let record = Node::parse(bytes, PATH)?;
    let terrain = record.required("terrain", PATH)?;
    if terrain.array(PATH)?.len() != 40 {
        return Err(invalid("terrain height"));
    }
    for row in terrain.array(PATH)? {
        let row = row.string(PATH)?;
        if row.len() != 40 || !row.bytes().all(|c| b"gfmsrbdivpwc".contains(&c)) {
            return Err(invalid("terrain row"));
        }
    }
    for field in ["map_resources", "tree_stumps"] {
        for item in record.required(field, PATH)?.array(PATH)? {
            check_resource(item, field == "tree_stumps", key)?;
        }
    }
    let mut resources = record.required("map_resources", PATH)?.clone();
    let Node::Array(items) = &mut resources else {
        unreachable!()
    };
    for item in items {
        let kind = item.required("kind", PATH)?.string(PATH)?;
        if ["tree", "water", "rock", "berry_bush", "herb_patch"].contains(&kind.as_str()) {
            // Native quantities are binary64; Python's max constants are ints.
            // min(maximum, computed_float) returns its first (integer) argument
            // on equality. The distinction is part of the resource version hash.
            if let Some(maximum) = item
                .optional("supply_max", PATH)?
                .map(|n| n.nonnegative_number(PATH))
                .transpose()?
            {
                let maximum = maximum as u64;
                set(item, "supply_max", scalar(maximum));
                if item
                    .optional("supply", PATH)?
                    .map(|n| n.nonnegative_number(PATH))
                    .transpose()?
                    == Some(maximum as f64)
                {
                    set(item, "supply", scalar(maximum));
                }
            }
            if kind == "tree" && item.optional("regeneration", PATH)?.is_some() {
                set(item, "regeneration", scalar(0));
            }
        }
    }
    Ok((
        terrain.clone(),
        Node::Object(vec![
            ("map_resources".into(), resources),
            (
                "tree_stumps".into(),
                record.required("tree_stumps", PATH)?.clone(),
            ),
        ]),
    ))
}
fn check_resource(item: &Node, stump: bool, key: ChunkKey) -> Result<(), BoundaryError> {
    check_resource_in(item, stump, (key.x as i32 * 40, key.y as i32 * 40), 40)
}
fn check_resource_in(
    item: &Node,
    stump: bool,
    origin: (i32, i32),
    size: i32,
) -> Result<(), BoundaryError> {
    let kind = item.required("kind", PATH)?.string(PATH)?;
    if (stump && kind != "tree")
        || ![
            "tree",
            "water",
            "rock",
            "berry_bush",
            "herb_patch",
            "chest",
            "wild_horse",
            "village_hut",
            "village_market",
            "village_decor",
        ]
        .contains(&kind.as_str())
    {
        return Err(invalid("missing resource kind coverage"));
    }
    for axis in ["x", "y"] {
        let value = item.required(axis, PATH)?.tile(PATH)?;
        let origin = if axis == "x" { origin.0 } else { origin.1 };
        if !(origin..origin + size).contains(&value) {
            return Err(invalid("resource outside landscape window"));
        }
    }
    for (field, value) in item.object(PATH)? {
        match field.as_str() {
            "kind" | "species" | "ore" | "biome" | "village" | "variant" | "label" => {
                value.string(PATH)?;
            }
            "x" | "y" => {
                value.tile(PATH)?;
            }
            "supply" | "supply_max" | "regeneration" | "remaining" => {
                value.nonnegative_number(PATH)?;
            }
            "animals" if !stump => {
                for (animal, count) in value.object(PATH)? {
                    if !["chicken", "pig", "horse", "sheep", "cow", "goat"]
                        .contains(&animal.as_str())
                    {
                        return Err(invalid("missing animal metadata coverage"));
                    }
                    count.unsigned(PATH)?;
                }
            }
            _ => return Err(invalid("missing resource field coverage")),
        }
    }
    if stump {
        item.required("remaining", PATH)?.nonnegative_number(PATH)?;
    }
    Ok(())
}
fn resource_version(payload: &Node) -> Result<String, BoundaryError> {
    let mut body = String::new();
    python_json(payload, &mut body)?;
    let mut hash = Blake2bVar::new(8).unwrap();
    hash.update(body.as_bytes());
    let mut bytes = [0; 8];
    hash.finalize_variable(&mut bytes).unwrap();
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// json.dumps(separators=(',', ':'), ensure_ascii=True), retaining object order
/// and Python's int/float distinction. Hashing serde_json::Value would sort keys,
/// lose wide integers and change Unicode/exponent spellings, producing wrong IDs.
fn python_json(node: &Node, out: &mut String) -> Result<(), BoundaryError> {
    match node {
        Node::Object(fields) => {
            out.push('{');
            for (i, (key, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                python_string(key, out);
                out.push(':');
                python_json(value, out)?;
            }
            out.push('}');
        }
        Node::Array(items) => {
            out.push('[');
            for (i, value) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                python_json(value, out)?;
            }
            out.push(']');
        }
        Node::Scalar(value) => {
            let text = value.get();
            if text.starts_with('"') {
                python_string(&node.string(PATH)?, out);
            } else if matches!(text, "null" | "true" | "false") {
                out.push_str(text);
            } else if text.contains(['.', 'e', 'E']) {
                let value: f64 = text.parse().map_err(|_| invalid("invalid public float"))?;
                if !value.is_finite() {
                    return Err(invalid("nonfinite public float"));
                }
                let spelling = format!("{value:?}");
                if let Some((mantissa, exponent)) = spelling.split_once('e') {
                    let exponent: i32 = exponent.parse().unwrap();
                    out.push_str(&format!("{mantissa}e{exponent:+03}"));
                } else {
                    out.push_str(&spelling);
                }
            } else {
                out.push_str(if text == "-0" { "0" } else { text });
            }
        }
    }
    Ok(())
}
fn python_string(value: &str, out: &mut String) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if !(' '..'\u{7f}').contains(&c) => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_json_bytes_and_blake2_versions_match_independent_oracle() {
        #[derive(serde::Deserialize)]
        struct Case {
            input: String,
            canonical: String,
            version: String,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../../tests/fixtures/http/python_json.json"))
                .unwrap();
        for (index, case) in cases.into_iter().enumerate() {
            let node = Node::parse(case.input.as_bytes(), PATH).unwrap();
            let mut encoded = String::new();
            python_json(&node, &mut encoded).unwrap();
            assert_eq!(encoded, case.canonical, "case {index}");
            assert_eq!(
                resource_version(&node).unwrap(),
                case.version,
                "case {index}"
            );
        }
    }
    #[test]
    fn projection_refuses_unreviewed_fields_and_keeps_envelope_private() {
        let key = ChunkKey { x: 0, y: 0 };
        let record = serde_json::json!({"terrain":vec!["g".repeat(40);40],
            "map_resources":[{"kind":"tree","x":1,"y":2,"supply":2.5}],"tree_stumps":[],
            "PRIVATE_ENVELOPE":{"tokens":"SYNTHETIC_SECRET"},"clock":17});
        let bytes = serde_json::to_vec(&record).unwrap();
        let (_, payload) = project_record(&bytes, key).unwrap();
        assert!(
            !String::from_utf8(raw(&payload))
                .unwrap()
                .contains("SYNTHETIC_SECRET")
        );
        let mut unknown = record.clone();
        unknown["map_resources"][0]["private"] = serde_json::json!("SYNTHETIC_SECRET");
        let error = project_record(&serde_json::to_vec(&unknown).unwrap(), key)
            .err()
            .unwrap();
        assert!(!format!("{error:?}").contains("SYNTHETIC_SECRET"));
        assert_eq!(bytes, serde_json::to_vec(&record).unwrap());
    }
}
