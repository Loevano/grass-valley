use super::{
    BoundReference, BuildingLink, ImportedTown, ImportedWorld, LandscapeLayout, PreservedField,
    json::Node, opaque_town, preserved_fields,
};
use crate::{
    BoundaryError, ErrorKind,
    state::{PrivateAccount, PrivateCredentials},
};
use lantern_core::ids::{BuildingId, IdAllocator, Owner, TownId, WorldId};
use std::collections::BTreeSet;

fn versions(node: &Node, path: &str) -> Result<(), BoundaryError> {
    if let Some(chunked) = node.optional("chunked", path)?
        && chunked.boolean(&format!("{path}.chunked"))?
    {
        return Err(BoundaryError::invalid(
            format!("{path}.chunked"),
            "external chunks require R06 slice 2",
        ));
    }
    for (field, minimum, maximum) in [
        ("terrain_version", 1, 7),
        ("forage_version", 0, 1),
        ("industry_version", 0, 2),
        ("mineral_version", 0, 1),
        ("npc_roster_version", 0, 3),
        ("npc_placement_version", 0, 1),
        ("wood_chain_version", 0, 1),
        ("grain_chain_version", 0, 1),
        ("worker_capacity_version", 0, 1),
        ("acre_upgrade_version", 0, 1),
        ("inventory_version", 0, 1),
        ("market_storage_version", 0, 1),
        ("food_balance_version", 0, 2),
    ] {
        if let Some(value) = node.optional(field, path)? {
            let location = format!("{path}.{field}");
            let version = value.unsigned(&location)?;
            if !(minimum..=maximum).contains(&version) {
                return Err(BoundaryError::invalid(
                    location,
                    "unsupported explicit version",
                ));
            }
        }
    }
    Ok(())
}

fn reference_index(
    node: &Node,
    buildings: &[BuildingId],
    path: &str,
) -> Result<BuildingId, BoundaryError> {
    let index = node.unsigned(path)?;
    usize::try_from(index)
        .ok()
        .and_then(|i| buildings.get(i))
        .cloned()
        .ok_or_else(|| BoundaryError::invalid(path, "dangling building index"))
}

pub(super) fn import_world(
    world: &WorldId,
    payload: &Node,
    path: &str,
    preserved: &mut Vec<PreservedField>,
) -> Result<(LandscapeLayout, Vec<ImportedTown>, IdAllocator), BoundaryError> {
    versions(payload, path)?;
    let layout = if let Some(shared) = payload.optional("shared_map", path)? {
        versions(shared, &format!("{path}.shared_map"))?;
        LandscapeLayout::Shared
    } else {
        LandscapeLayout::Independent
    };
    let mut allocator = IdAllocator::new(world.clone());
    let mut towns = Vec::new();
    let entries = payload
        .required("towns", path)?
        .object(&format!("{path}.towns"))?;
    preserved.push(PreservedField {
        path: path.into(),
        reason: "gameplay payload retained privately; not converted or playable",
    });
    for (index, (key, town)) in entries.iter().enumerate() {
        let tp = format!("{path}.towns[{index}]");
        versions(town, &tp)?;
        let owner = Owner {
            world: world.clone(),
            town: opaque_town(key, &tp)?,
        };
        let building_records = town
            .required("buildings", &tp)?
            .array(&format!("{tp}.buildings"))?;
        let villager_records = town
            .required("villagers", &tp)?
            .array(&format!("{tp}.villagers"))?;
        let mut buildings = Vec::new();
        let mut kinds = Vec::new();
        for (bi, building) in building_records.iter().enumerate() {
            let bp = format!("{tp}.buildings[{bi}]");
            let kind = building
                .required("kind", &bp)?
                .string(&format!("{bp}.kind"))?;
            if kind.is_empty() {
                return Err(BoundaryError::invalid(
                    format!("{bp}.kind"),
                    "empty building kind",
                ));
            }
            for axis in ["x", "y"] {
                building
                    .required(axis, &bp)?
                    .tile(&format!("{bp}.{axis}"))?;
            }
            kinds.push(kind);
            buildings.push(
                allocator
                    .allocate(&owner)
                    .map_err(|e| BoundaryError::at(&bp, ErrorKind::Identity(e)))?,
            );
            preserved_fields(
                building,
                &["kind", "x", "y", "farm_index", "orchard_index"],
                &bp,
                preserved,
            )?;
        }
        let mut references = Vec::new();
        for (bi, building) in building_records.iter().enumerate() {
            let bp = format!("{tp}.buildings[{bi}]");
            for (field, link, expected) in [
                ("farm_index", BuildingLink::Farm, "farm"),
                ("orchard_index", BuildingLink::Orchard, "orchard"),
            ] {
                if let Some(value) = building.optional(field, &bp)? {
                    if value.is_null() {
                        continue;
                    }
                    let location = format!("{bp}.{field}");
                    let target = reference_index(value, &buildings, &location)?;
                    let target_index =
                        usize::try_from(value.unsigned(&location)?).expect("validated index");
                    if kinds[target_index] != expected {
                        return Err(BoundaryError::invalid(
                            location,
                            "wrong linked building kind",
                        ));
                    }
                    references.push(BoundReference::Building {
                        source: buildings[bi].clone(),
                        field: link,
                        target,
                    });
                }
            }
        }
        let mut villagers = Vec::new();
        for (vi, villager) in villager_records.iter().enumerate() {
            let vp = format!("{tp}.villagers[{vi}]");
            villager.object(&vp)?;
            let actor = allocator
                .allocate(&owner)
                .map_err(|e| BoundaryError::at(&vp, ErrorKind::Identity(e)))?;
            if let Some(value) = villager.optional("building_index", &vp)?
                && !value.is_null()
            {
                references.push(BoundReference::Assignment {
                    actor: actor.clone(),
                    building: reference_index(value, &buildings, &format!("{vp}.building_index"))?,
                });
            }
            villagers.push(actor);
            preserved_fields(villager, &["building_index"], &vp, preserved)?;
        }
        let founder =
            if let Some(player) = town.optional("player", &tp)? {
                player.object(&format!("{tp}.player"))?;
                Some(allocator.allocate(&owner).map_err(|e| {
                    BoundaryError::at(format!("{tp}.player"), ErrorKind::Identity(e))
                })?)
            } else {
                None
            };
        preserved_fields(town, &["buildings", "villagers"], &tp, preserved)?;
        towns.push(ImportedTown {
            owner,
            buildings,
            villagers,
            founder,
            references,
        });
    }
    Ok((layout, towns, allocator))
}

fn account(
    node: &Node,
    owners: &BTreeSet<TownId>,
    path: &str,
    preserved: &mut Vec<PreservedField>,
) -> Result<PrivateAccount, BoundaryError> {
    let town_id = opaque_town(
        &node
            .required("town_id", path)?
            .string(&format!("{path}.town_id"))?,
        &format!("{path}.town_id"),
    )?;
    if !owners.contains(&town_id) {
        return Err(BoundaryError::invalid(
            format!("{path}.town_id"),
            "dangling credential owner",
        ));
    }
    preserved_fields(
        node,
        &["name", "town_id", "salt", "pin_hash"],
        path,
        preserved,
    )?;
    Ok(PrivateAccount {
        town_id,
        name: node
            .required("name", path)?
            .string(&format!("{path}.name"))?,
        salt: node
            .required("salt", path)?
            .string(&format!("{path}.salt"))?,
        pin_hash: node
            .required("pin_hash", path)?
            .string(&format!("{path}.pin_hash"))?,
    })
}

pub(super) fn credentials(
    tokens: &Node,
    players: Option<&Node>,
    owners: &BTreeSet<TownId>,
    path: &str,
    preserved: &mut Vec<PreservedField>,
) -> Result<PrivateCredentials, BoundaryError> {
    let mut result = PrivateCredentials::default();
    for (index, (key, value)) in tokens.object(&format!("{path}.tokens"))?.iter().enumerate() {
        let location = format!("{path}.tokens[{index}]");
        if key.is_empty() {
            return Err(BoundaryError::invalid(location, "empty credential key"));
        }
        let town = opaque_town(&value.string(&location)?, &location)?;
        if !owners.contains(&town) {
            return Err(BoundaryError::invalid(
                location,
                "dangling credential owner",
            ));
        }
        result.tokens.insert(key.clone(), town);
    }
    if let Some(players) = players {
        for (index, (key, value)) in players
            .object(&format!("{path}.players"))?
            .iter()
            .enumerate()
        {
            let location = format!("{path}.players[{index}]");
            if key.is_empty() {
                return Err(BoundaryError::invalid(location, "empty credential key"));
            }
            result
                .players
                .insert(key.clone(), account(value, owners, &location, preserved)?);
        }
    }
    Ok(result)
}

pub(super) fn provision(
    world: &mut ImportedWorld,
    players: &Node,
    path: &str,
    preserved: &mut Vec<PreservedField>,
    ignored: &mut usize,
) -> Result<(), BoundaryError> {
    let owners = world
        .towns
        .iter()
        .map(|town| town.owner.town.clone())
        .collect::<BTreeSet<_>>();
    for (index, (key, value)) in players.object(path)?.iter().enumerate() {
        let location = format!("{path}.players[{index}]");
        let town = value
            .required("town_id", &location)?
            .string(&format!("{location}.town_id"))?;
        if !owners.iter().any(|id| id.as_str() == town)
            || world.credentials.players.contains_key(key)
        {
            *ignored += 1;
            continue;
        }
        if key.is_empty() {
            return Err(BoundaryError::invalid(location, "empty credential key"));
        }
        world
            .credentials
            .players
            .insert(key.clone(), account(value, &owners, &location, preserved)?);
    }
    Ok(())
}
