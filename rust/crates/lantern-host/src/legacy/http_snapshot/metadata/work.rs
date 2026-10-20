//! Frozen R11 task/source/route checks over the checked native observation.
use super::*;

pub(in crate::legacy::http_snapshot) fn sources(
    map: &Node,
) -> Result<Vec<lantern_core::spatial::Resource>, BoundaryError> {
    map.required("map_resources", PATH)?
        .array(PATH)?
        .iter()
        .map(|r| {
            Ok(lantern_core::spatial::Resource {
                kind: r.required("kind", PATH)?.string(PATH)?,
                footprint: lantern_core::spatial::Footprint::saved(
                    Tile {
                        x: r.required("x", PATH)?.tile(PATH)?,
                        y: r.required("y", PATH)?.tile(PATH)?,
                    },
                    None,
                )
                .map_err(|_| invalid("work source geometry coverage"))?,
            })
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
pub(super) fn target(
    record: &Node,
    task: &Task,
    job: &str,
    town: &lantern_core::model::Town,
    rules: &crate::building::Rules,
    sources: &[lantern_core::spatial::Resource],
    returning_tree: bool,
) -> Result<(), BoundaryError> {
    let Task::Work {
        workplace, target, ..
    } = task
    else {
        return Ok(());
    };
    let animation = record.required("work_animation", PATH)?;
    let spec = animation.required("task", PATH)?.array(PATH)?;
    if spec.len() != 7 {
        return Err(invalid("work task shape coverage"));
    }
    let site = &rules.sites[workplace];
    let building = town
        .building(workplace)
        .map_err(|_| invalid("workplace owner coverage"))?;
    if spec[0].string(PATH)? != job
        || spec[1].string(PATH)? != site.kind
        || spec[2].number(PATH)? != f64::from(building.location.x)
        || spec[3].number(PATH)? != f64::from(building.location.y)
    {
        return Err(invalid("work task owner coverage"));
    }
    let kind = spec[4].string(PATH)?;
    if job == "woodcutting" && (site.kind != "woodcutter" || kind != "tree") {
        return Err(invalid("gathering source kind coverage"));
    }
    let (x, y) = match target {
        lantern_core::model::Destination::Building(id) => {
            if rules.sites[id].kind != kind {
                return Err(invalid("work source kind coverage"));
            }
            let b = town
                .building(id)
                .map_err(|_| invalid("work source owner coverage"))?;
            (f64::from(b.location.x), f64::from(b.location.y))
        }
        lantern_core::model::Destination::Position(p) => {
            if !(returning_tree && kind == "tree")
                && !sources.iter().any(|r| {
                    r.kind == kind
                        && f64::from(r.footprint.origin().x) == p.x.get()
                        && f64::from(r.footprint.origin().y) == p.y.get()
                })
            {
                return Err(invalid("work source geometry coverage"));
            }
            (p.x.get(), p.y.get())
        }
    };
    if spec[5].number(PATH)? != x || spec[6].number(PATH)? != y {
        return Err(invalid("work source coordinates coverage"));
    }
    Ok(())
}
pub(in crate::legacy::http_snapshot) fn terrain(
    map: &Node,
    origin: Tile,
) -> Result<Vec<(Tile, char)>, BoundaryError> {
    let mut cells = Vec::new();
    for (y, row) in map
        .required("terrain", PATH)?
        .array(PATH)?
        .iter()
        .enumerate()
    {
        for (x, c) in row.string(PATH)?.chars().enumerate() {
            cells.push((
                Tile {
                    x: origin.x + x as i32,
                    y: origin.y + y as i32,
                },
                c,
            ));
        }
    }
    Ok(cells)
}
pub(super) fn path(
    record: &Node,
    world: &RestoredWorld,
    sources: &[lantern_core::spatial::Resource],
    map: &Node,
    terrain: &std::collections::BTreeMap<Tile, char>,
) -> Result<(), BoundaryError> {
    let Some(animation) = record.optional("work_animation", PATH)? else {
        return Ok(());
    };
    let size = map.required("map_size", PATH)?.unsigned(PATH)? as f64;
    let mut prior = (
        animation.required("x", PATH)?.number(PATH)?,
        animation.required("y", PATH)?.number(PATH)?,
    );
    let RestoredModel::Building(buildings) = &world.model else {
        return Err(invalid("worker geometry owner coverage"));
    };
    use lantern_core::spatial::{
        AccessProfile, Construction, Footprint, LandAccess, SpatialIndex, Structure,
    };
    let mut structures = Vec::new();
    for town in buildings.model().towns() {
        for building in town.buildings() {
            let site = &buildings.rules()[&town.owner().town].sites[&building.id];
            structures.push(Structure {
                footprint: Footprint::saved(building.location, Some(site.dimensions))
                    .map_err(|_| invalid("worker structure coverage"))?,
                kind: site.kind.clone(),
                construction: if building.construction == ConstructionState::Complete {
                    Construction::Complete
                } else if site.upgrade.is_some() {
                    Construction::Upgrade
                } else {
                    Construction::Site
                },
                fence_orientation: site.fence_orientation.into(),
            });
        }
    }
    let npcs = map
        .required("npcs", PATH)?
        .array(PATH)?
        .iter()
        .map(|n| {
            Footprint::saved(
                Tile {
                    x: n.required("x", PATH)?.tile(PATH)?,
                    y: n.required("y", PATH)?.tile(PATH)?,
                },
                None,
            )
            .map_err(|_| invalid("worker NPC geometry coverage"))
        })
        .collect::<Result<Vec<_>, BoundaryError>>()?;
    let spatial = SpatialIndex::derive(&structures, sources, &npcs, &[], size as u32)
        .map_err(|_| invalid("worker spatial owner coverage"))?;
    let exemptions = std::collections::BTreeSet::new();
    let access = AccessProfile {
        land: if record.required("job", PATH)?.string(PATH)? == "farming" {
            LandAccess::Farmer
        } else {
            LandAccess::Ordinary
        },
        exemptions: &exemptions,
        only: None,
    };
    let open = |tile: Tile| {
        tile.x >= 0
            && tile.y >= 0
            && f64::from(tile.x) < size
            && f64::from(tile.y) < size
            && !spatial.occupied_for(tile, &access)
            && terrain.get(&tile).is_some_and(|c| *c != 'v')
            && !buildings.landscape().unavailable.contains(&tile)
    };
    for (index, p) in animation
        .required("path", PATH)?
        .array(PATH)?
        .iter()
        .enumerate()
    {
        let x = p.required("x", PATH)?.number(PATH)?;
        let y = p.required("y", PATH)?.number(PATH)?;
        let limit = if index == 0 { 1.5 } else { 1. };
        if x < 0.
            || y < 0.
            || x >= size
            || y >= size
            || x.fract() != 0.
            || y.fract() != 0.
            || (x - prior.0).abs() > limit
            || (y - prior.1).abs() > limit
        {
            return Err(invalid("worker route geometry coverage"));
        }
        let tile = Tile {
            x: x as i32,
            y: y as i32,
        };
        let from = Tile {
            x: (prior.0 + 0.5).floor() as i32,
            y: (prior.1 + 0.5).floor() as i32,
        };
        let start = lantern_core::units::Position::new(prior.0, prior.1)
            .map_err(|_| invalid("worker segment start coverage"))?;
        let end = lantern_core::units::Position::new(x, y)
            .map_err(|_| invalid("worker segment end coverage"))?;
        if !(if from == tile {
            open(tile)
        } else {
            spatial.edge_open(from, tile, true, open)
        }) || lantern_core::spatial::crosses(start, end, spatial.fences())
        {
            return Err(invalid("worker route geometry coverage"));
        }
        prior = (x, y);
    }
    Ok(())
}
