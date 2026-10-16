//! Explicit terrain-only hydration of nonchunked copies, never full gameplay load.
use super::{
    ImportedRegistry, LandscapeLayout, LoadedResource, LoadedStump, json::Node,
    loaded::decode_resource_arrays,
};
use crate::BoundaryError;
use lantern_core::{
    ids::{Owner, WorldId},
    legacy_map::{MapSeed, generate_map},
};

/// Owned decoded terrain and source quantities. Unknown metadata stays in the
/// containing HydratedLandscapes' exact private source bytes. No Debug/Serialize.
pub struct HydratedLandscape {
    size: u32,
    version: u32,
    rows: Vec<String>,
    resources: Vec<LoadedResource>,
    stumps: Vec<LoadedStump>,
    regenerated: bool,
}
impl HydratedLandscape {
    pub fn map_size(&self) -> u32 {
        self.size
    }
    pub fn generator_version(&self) -> u32 {
        self.version
    }
    pub fn terrain(&self) -> &[String] {
        &self.rows
    }
    pub fn resources(&self) -> &[LoadedResource] {
        &self.resources
    }
    pub fn stumps(&self) -> &[LoadedStump] {
        &self.stumps
    }
    pub fn regenerated_ground(&self) -> bool {
        self.regenerated
    }
}

/// One map per independent town, or exactly one shared map bound to every town.
/// There is no implicit independent-to-shared conversion or mutable view cache.
///
/// ```compile_fail
/// use lantern_host::legacy::HydratedLandscapes;
/// fn leak(view: &HydratedLandscapes<'_>) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct HydratedLandscapes<'a> {
    source: &'a [u8],
    maps: Vec<HydratedLandscape>,
    bindings: Vec<(Owner, usize)>,
    layout: LandscapeLayout,
}
impl HydratedLandscapes<'_> {
    pub fn layout(&self) -> LandscapeLayout {
        self.layout
    }
    pub fn landscapes(&self) -> &[HydratedLandscape] {
        &self.maps
    }
    pub fn for_town(&self, town: &Owner) -> Option<&HydratedLandscape> {
        self.bindings
            .iter()
            .find(|(id, _)| id == town)
            .map(|(_, index)| &self.maps[*index])
    }
    /// Contains the whole private registry, potentially including credentials.
    pub fn private_source_bytes(&self) -> &[u8] {
        self.source
    }
}

impl ImportedRegistry {
    /// Mirrors only deterministic terrain.ensure_terrain(new=False): valid-height
    /// saved rows win; absent/wrong-height rows regenerate with void replaced by
    /// grass. Saved resources/stumps remain untouched. No ensure_resources,
    /// industry/NPC migration, supply defaults, production, clocks or writes.
    pub fn hydrate_nonchunked(
        &self,
        world: &WorldId,
    ) -> Result<HydratedLandscapes<'_>, BoundaryError> {
        let path = "$.terrain_hydration";
        let index = self
            .worlds
            .iter()
            .position(|w| w.id() == world)
            .ok_or_else(|| BoundaryError::invalid(path, "unknown world owner"))?;
        let imported = &self.worlds[index];
        if imported.layout() == LandscapeLayout::Chunked {
            return Err(BoundaryError::invalid(
                path,
                "chunked world requires chunk hydration",
            ));
        }
        let source = self
            .private_source_bytes()
            .ok_or_else(|| BoundaryError::invalid(path, "source unavailable"))?;
        let root = Node::parse(source, path)?;
        let payload = if let Some(entries) = root.optional("worlds", path)? {
            entries.object(path)?[index].1.required("world", path)?
        } else {
            root.required("world", path)?
        };
        let mut maps = Vec::new();
        let mut bindings = Vec::new();
        if let Some(shared) = payload.optional("shared_map", path)? {
            maps.push(hydrate(shared, "$.terrain_hydration.shared_map")?);
            bindings.extend(imported.towns().iter().map(|t| (t.owner().clone(), 0)));
        } else {
            for (index, (_, town)) in payload
                .required("towns", path)?
                .object(path)?
                .iter()
                .enumerate()
            {
                maps.push(hydrate(
                    town,
                    &format!("$.terrain_hydration.towns[{index}]"),
                )?);
                bindings.push((imported.towns()[index].owner().clone(), index));
            }
        }
        Ok(HydratedLandscapes {
            source,
            maps,
            bindings,
            layout: imported.layout(),
        })
    }
}

fn hydrate(node: &Node, path: &str) -> Result<HydratedLandscape, BoundaryError> {
    let size = node
        .optional("map_size", path)?
        .map(|n| n.unsigned(&format!("{path}.map_size")))
        .transpose()?
        .unwrap_or(12);
    if !(1..=1920).contains(&size) {
        return Err(BoundaryError::invalid(
            format!("{path}.map_size"),
            "unsupported map dimensions",
        ));
    }
    let size = size as u32;
    let version = node
        .optional("terrain_version", path)?
        .map(|n| n.unsigned(&format!("{path}.terrain_version")))
        .transpose()?
        .unwrap_or(1);
    // Immutable import validates every world/shared/town version. Keep the check
    // here too, before narrowing, so this terrain boundary remains explicit.
    if !(1..=7).contains(&version) {
        return Err(BoundaryError::invalid(
            format!("{path}.terrain_version"),
            "unsupported terrain version",
        ));
    }
    let version = version as u32;
    let saved = node
        .optional("terrain", path)?
        .map(|n| n.array(&format!("{path}.terrain")))
        .transpose()?;
    let regenerated = saved.is_none_or(|rows| rows.len() != size as usize);
    let rows = if !regenerated {
        saved
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let rp = format!("{path}.terrain[{index}]");
                let row = row.string(&rp)?;
                if row.len() != size as usize || !row.bytes().all(|b| b"gfmsrbdiv".contains(&b)) {
                    return Err(BoundaryError::invalid(rp, "unsupported terrain row"));
                }
                Ok(row)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let sp = format!("{path}.terrain_seed");
        let seed = node.required("terrain_seed", path)?;
        let Node::Scalar(raw) = seed else {
            return Err(BoundaryError::invalid(sp, "unsupported map seed type"));
        };
        let text;
        let seed = if raw.get().starts_with('"') {
            text = seed.string(&sp)?;
            MapSeed::Text(&text)
        } else {
            MapSeed::Integer(raw.get())
        };
        generate_map(size, seed)
            .map_err(|_| BoundaryError::invalid(sp, "unsupported deterministic map generation"))?
            .terrain
            .into_iter()
            .map(|row| row.replace('v', "g"))
            .collect()
    };
    // Old inline maps can omit stumps. Required resources must not be silently
    // invented; their old gameplay bootstrap is outside this terrain-only API.
    let empty = Node::Array(Vec::new());
    let (resources, stumps) = decode_resource_arrays(
        node.required("map_resources", path)?,
        node.optional("tree_stumps", path)?.unwrap_or(&empty),
        path,
    )?;
    for tile in resources
        .iter()
        .map(|r| r.tile())
        .chain(stumps.iter().map(|s| s.tile()))
    {
        if tile.x < 0 || tile.y < 0 || tile.x >= size as i32 || tile.y >= size as i32 {
            return Err(BoundaryError::invalid(
                path,
                "resource or stump outside landscape",
            ));
        }
    }
    Ok(HydratedLandscape {
        size,
        version,
        rows,
        resources,
        stumps,
        regenerated,
    })
}
