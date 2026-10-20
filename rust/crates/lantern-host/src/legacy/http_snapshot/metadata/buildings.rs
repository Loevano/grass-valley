//! Checked frozen construction presentation; no builder playback is inferred.
use super::*;
use crate::building::{Road, Rules, Site, Upgrade};
use lantern_core::model::{Building, Town};

fn default_number(
    node: &Node,
    key: &str,
    expected: f64,
    default: f64,
) -> Result<(), BoundaryError> {
    if node
        .optional(key, PATH)?
        .map(|n| n.number(PATH))
        .transpose()?
        .unwrap_or(default)
        != expected
    {
        return Err(invalid("native/public building state mismatch"));
    }
    Ok(())
}
fn equipment(node: Option<&Node>, expected: &BTreeSet<String>) -> Result<(), BoundaryError> {
    let values = node.map(|n| n.array(PATH)).transpose()?.unwrap_or(&[]);
    let mut found = BTreeSet::new();
    for value in values {
        if !found.insert(value.string(PATH)?) {
            return Err(invalid("duplicate public equipment"));
        }
    }
    if &found != expected {
        return Err(invalid("native/public equipment mismatch"));
    }
    Ok(())
}
fn construction(
    node: Option<&Node>,
    state: &ConstructionState,
    upgrade: Option<&Upgrade>,
    equipment_owned: &BTreeSet<String>,
) -> Result<(), BoundaryError> {
    match (node, state) {
        (None, ConstructionState::Complete) => Ok(()),
        (Some(node), ConstructionState::InProgress { remaining }) => {
            fields(node, &["progress", "total", "upgrade"])?;
            let progress = node.required("progress", PATH)?.nonnegative_number(PATH)?;
            let total = node.required("total", PATH)?.nonnegative_number(PATH)?;
            if progress > total || total - progress != remaining.get() {
                return Err(invalid("native/public construction work mismatch"));
            }
            match (node.optional("upgrade", PATH)?, upgrade) {
                (None, None | Some(Upgrade::Hall { .. })) => (),
                (Some(value), Some(Upgrade::Structure(level))) => {
                    fields(value, &["level"])?;
                    same_number(value, "level", *level as f64)?;
                }
                (Some(value), Some(Upgrade::Capacity(level))) => {
                    fields(value, &["settler_level"])?;
                    same_number(value, "settler_level", *level as f64)?;
                }
                (Some(value), Some(Upgrade::Equipment(key))) => {
                    fields(value, &["profession_upgrades"])?;
                    let mut expected = equipment_owned.clone();
                    expected.insert(key.clone());
                    equipment(value.optional("profession_upgrades", PATH)?, &expected)?;
                }
                _ => return Err(invalid("native/public upgrade mismatch")),
            }
            Ok(())
        }
        _ => Err(invalid("native/public construction presence mismatch")),
    }
}
pub(super) fn check(record: &Node, building: &Building, site: &Site) -> Result<(), BoundaryError> {
    fields(
        record,
        &[
            "kind",
            "x",
            "y",
            "footprint",
            "stored_goods",
            "output_buffer",
            "input_buffer",
            "pending_output",
            "orphaned_output",
            "storage_enabled",
            "storage_allocations",
            "orientation",
            "fence_orientation",
            "construction",
            "level",
            "settler_level",
            "profession_upgrades",
            "farm_index",
            "orchard_index",
            "crop",
            "field",
            "tree",
        ],
    )?;
    default_number(record, "level", site.level as f64, 1.)?;
    default_number(record, "settler_level", site.capacity_level as f64, 1.)?;
    default_number(
        record,
        "fence_orientation",
        site.fence_orientation as f64,
        0.,
    )?;
    equipment(
        record.optional("profession_upgrades", PATH)?,
        &site.equipment,
    )?;
    construction(
        record.optional("construction", PATH)?,
        &building.construction,
        site.upgrade.as_ref(),
        &site.equipment,
    )
}
pub(super) fn roads(town: &Node, roads: &[Road]) -> Result<(), BoundaryError> {
    let records = town
        .optional("roads", PATH)?
        .map(|n| n.array(PATH))
        .transpose()?
        .unwrap_or(&[]);
    if records.len() != roads.len() {
        return Err(invalid("native/public road membership mismatch"));
    }
    for (record, road) in records.iter().zip(roads) {
        fields(record, &["x", "y", "construction"])?;
        same_number(record, "x", road.tile.x as f64)?;
        same_number(record, "y", road.tile.y as f64)?;
        let state = road
            .remaining
            .map_or(ConstructionState::Complete, |remaining| {
                ConstructionState::InProgress { remaining }
            });
        construction(
            record.optional("construction", PATH)?,
            &state,
            None,
            &BTreeSet::new(),
        )?;
    }
    Ok(())
}
pub(super) fn hall_remaining(
    record: &Node,
    town: &Town,
    rules: &Rules,
) -> Result<(), BoundaryError> {
    let mut remaining = 0.;
    for building in town.buildings() {
        if let Some(Upgrade::Hall { .. }) = rules.sites[&building.id].upgrade
            && let ConstructionState::InProgress { remaining: work } = building.construction
        {
            remaining = work.get();
        }
    }
    default_number(record, "upgrade_remaining", remaining, 0.)
}
