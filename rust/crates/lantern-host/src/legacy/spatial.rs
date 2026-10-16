//! Private source-retaining geometry adapter. This decodes a supplied landscape
//! copy only; founder_world composes shared-world/terrain access and founder rules. Unknown fields/kinds remain in the immutable original bytes.
use super::json::Node;
use crate::BoundaryError;
use lantern_core::{
    spatial::{Construction, Footprint, Resource, Road, SpatialIndex, Structure},
    units::Tile,
};

pub struct PrivateSpatialCopy {
    source: Vec<u8>,
    structures: Vec<Structure>,
    resources: Vec<Resource>,
    npcs: Vec<Footprint>,
    roads: Vec<Road>,
    size: u32,
}
impl PrivateSpatialCopy {
    pub fn decode(source: &[u8]) -> Result<Self, BoundaryError> {
        let path = "$.spatial_copy";
        let root = Node::parse(source, path)?;
        let size = root.required("map_size", path)?.unsigned(path)?;
        if size == 0 || size > i32::MAX as u64 {
            return Err(invalid(path, "unsupported map size"));
        }
        let mut result = Self {
            source: source.to_vec(),
            structures: Vec::new(),
            resources: Vec::new(),
            npcs: Vec::new(),
            roads: Vec::new(),
            size: size as u32,
        };
        // Bound eager index expansion; rejecting unsupported geometry is explicit
        // and leaves the input untouched. No clipping or silent dimension loss.
        let mut budget = 1_000_000_u64;
        for (i, b) in root
            .required("buildings", path)?
            .array(path)?
            .iter()
            .enumerate()
        {
            let p = format!("{path}.buildings[{i}]");
            result.structures.push(Structure {
                footprint: footprint(b, &p, &mut budget)?,
                kind: b.required("kind", &p)?.string(&p)?,
                construction: construction(b, &p)?,
                fence_orientation: b
                    .optional("fence_orientation", &p)?
                    .map(|v| v.tile(&p))
                    .transpose()?
                    .unwrap_or(0),
            });
            if let Some(v) = b.optional("orientation", &p)? {
                v.tile(&format!("{p}.orientation"))?;
            }
        }
        for (i, r) in root
            .required("map_resources", path)?
            .array(path)?
            .iter()
            .enumerate()
        {
            let p = format!("{path}.map_resources[{i}]");
            result.resources.push(Resource {
                footprint: footprint(r, &p, &mut budget)?,
                kind: r.required("kind", &p)?.string(&p)?,
            });
        }
        if let Some(npcs) = root.optional("npcs", path)? {
            for (i, n) in npcs.array(path)?.iter().enumerate() {
                result
                    .npcs
                    .push(footprint(n, &format!("{path}.npcs[{i}]"), &mut budget)?);
            }
        }
        if let Some(roads) = root.optional("roads", path)? {
            for (i, r) in roads.array(path)?.iter().enumerate() {
                let p = format!("{path}.roads[{i}]");
                result.roads.push(Road {
                    tile: position(r, &p)?,
                    construction: construction(r, &p)?,
                });
            }
        }
        Ok(result)
    }
    pub fn private_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn structures(&self) -> &[Structure] {
        &self.structures
    }
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }
    pub fn npcs(&self) -> &[Footprint] {
        &self.npcs
    }
    pub fn roads(&self) -> &[Road] {
        &self.roads
    }
    pub fn derive_index(&self) -> Result<SpatialIndex, BoundaryError> {
        SpatialIndex::derive(
            &self.structures,
            &self.resources,
            &self.npcs,
            &self.roads,
            self.size,
        )
        .map_err(|_| invalid("$.spatial_copy", "unsupported geometry"))
    }
}
fn invalid(path: &str, reason: &'static str) -> BoundaryError {
    BoundaryError::invalid(path, reason)
}
fn position(n: &Node, p: &str) -> Result<Tile, BoundaryError> {
    Ok(Tile {
        x: n.required("x", p)?.tile(&format!("{p}.x"))?,
        y: n.required("y", p)?.tile(&format!("{p}.y"))?,
    })
}
fn footprint(n: &Node, p: &str, budget: &mut u64) -> Result<Footprint, BoundaryError> {
    let dimensions = n
        .optional("footprint", p)?
        .map(|v| {
            let fp = v.array(p)?;
            if fp.len() != 2 {
                return Err(invalid(p, "expected two footprint dimensions"));
            }
            let w = fp[0].unsigned(p)?;
            let d = fp[1].unsigned(p)?;
            if w == 0 || d == 0 || w > u32::MAX as u64 || d > u32::MAX as u64 {
                return Err(invalid(p, "unsupported footprint dimensions"));
            }
            Ok((w as u32, d as u32))
        })
        .transpose()?;
    let (w, d) = dimensions.unwrap_or((1, 1));
    *budget = budget
        .checked_sub(u64::from(w) * u64::from(d))
        .ok_or_else(|| invalid(p, "spatial copy exceeds tile expansion limit"))?;
    Footprint::saved(position(n, p)?, dimensions)
        .map_err(|_| invalid(p, "unsupported footprint coordinates"))
}
fn construction(n: &Node, p: &str) -> Result<Construction, BoundaryError> {
    match n.optional("construction", p)? {
        None => Ok(Construction::Complete),
        Some(v) => {
            let fields = v.object(&format!("{p}.construction"))?;
            Ok(if fields.is_empty() {
                Construction::EmptySite
            } else if v.optional("upgrade", p)?.is_some() {
                Construction::Upgrade
            } else {
                Construction::Site
            })
        }
    }
}
