//! Frozen R12 controller facts, including checked R18 profession helpers.
use super::*;
use crate::building::{Builder, Claim, Role, Rules};
use lantern_core::{
    ids::{ActorId, BuildingId},
    model::{ActorState, ConstructionPhase, Town},
};
use std::collections::BTreeMap;

fn point(node: &Node, position: lantern_core::units::Position) -> Result<(), BoundaryError> {
    fields(node, &["x", "y"])?;
    same_number(node, "x", position.x.get())?;
    same_number(node, "y", position.y.get())
}
fn target(node: &Node, claim: &Claim, town: &Town, rules: &Rules) -> Result<(), BoundaryError> {
    fields(node, &["kind", "x", "y", "footprint"])?;
    let (kind, tile, dimensions) = match claim {
        Claim::Building(id) => {
            let b = town
                .building(id)
                .map_err(|_| invalid("construction target owner coverage"))?;
            let site = &rules.sites[id];
            (site.kind.as_str(), b.location, site.dimensions)
        }
        Claim::Road(tile) => ("road", *tile, (1, 1)),
    };
    if node.required("kind", PATH)?.string(PATH)? != kind {
        return Err(invalid("native/public construction target kind mismatch"));
    }
    same_number(node, "x", tile.x as f64)?;
    same_number(node, "y", tile.y as f64)?;
    if let Some(dim) = node.optional("footprint", PATH)? {
        let pair = dim.array(PATH)?;
        if pair.len() != 2
            || pair[0].unsigned(PATH)? != dimensions.0 as u64
            || pair[1].unsigned(PATH)? != dimensions.1 as u64
        {
            return Err(invalid(
                "native/public construction target footprint mismatch",
            ));
        }
    }
    Ok(())
}
/// Returns whether the explicit construction position supplies the actor pose.
/// A retained position after cancellation is still checked against the actor.
pub(super) fn check(
    record: &Node,
    actor: &ActorState,
    job: &str,
    town: &Town,
    rules: &Rules,
    builders: &BTreeMap<ActorId, Builder>,
    assignment: Option<&BuildingId>,
) -> Result<bool, BoundaryError> {
    let positioned = if let Some(p) = record.optional("construction_position", PATH)? {
        point(p, actor.position)?;
        true
    } else {
        false
    };
    let builder = builders.get(&actor.id);
    if actor.travel != TravelIntent::None || !actor.cargo.goods().is_empty() {
        return Err(invalid("workplace actor playback coverage"));
    }
    let expected_role = if job == "building" {
        Role::Dedicated
    } else if job == "idle" {
        Role::Idle
    } else {
        Role::Busy
    };
    if let Some(b) = builder {
        if b.role != expected_role
            || b.helper.as_ref()
                != if expected_role == Role::Busy {
                    assignment
                } else {
                    None
                }
        {
            return Err(invalid("construction role/helper coverage"));
        }
        // Movement/activity retained under a profession suspension needs the
        // separate 1h.b.2 work adapter. Never replace it with an idle placeholder.
        if let Some(saved) = &b.suspended
            && (saved.task != Task::Idle || saved.travel != TravelIntent::None)
        {
            return Err(invalid("suspended workplace playback coverage"));
        }
    } else if job == "building" {
        return Err(invalid("missing native construction controller coverage"));
    }
    let claim = builder.and_then(|b| b.claim.as_ref());
    let phase = match (&actor.task, claim) {
        (Task::Idle, None) => None,
        (
            Task::Construction {
                site,
                phase,
                elapsed,
            },
            Some(Claim::Building(id)),
        ) if site == id && elapsed.get() == 0. => Some(*phase),
        (Task::RoadConstruction { tile, phase }, Some(Claim::Road(expected)))
            if tile == expected =>
        {
            Some(*phase)
        }
        _ => return Err(invalid("construction task/claim public coverage")),
    };
    match (record.optional("construction_target", PATH)?, claim) {
        (Some(node), Some(c)) => target(node, c, town, rules)?,
        (None, None) => (),
        _ => return Err(invalid("construction target presence coverage")),
    }
    let arrived = record
        .optional("construction_arrived", PATH)?
        .map(|n| n.boolean(PATH))
        .transpose()?
        .unwrap_or(false);
    if arrived != (phase == Some(ConstructionPhase::Building)) {
        return Err(invalid("native/public construction phase mismatch"));
    }
    match (
        record.optional("construction_walk", PATH)?,
        builder.and_then(|b| b.walk.as_ref()),
    ) {
        (None, None) if !arrived => (),
        (Some(node), Some(walk)) => {
            fields(node, &["target", "job", "path", "step"])?;
            target(node.required("target", PATH)?, &walk.claim, town, rules)?;
            let tile = match &walk.claim {
                Claim::Building(id) => {
                    town.building(id)
                        .map_err(|_| invalid("construction walk owner coverage"))?
                        .location
                }
                Claim::Road(t) => *t,
            };
            let dimensions = match &walk.claim {
                Claim::Building(id) => rules.sites[id].dimensions,
                Claim::Road(_) => (1, 1),
            };
            if walk.footprint.origin() != tile
                || walk.footprint.dimensions() != dimensions
                || node.required("job", PATH)?.string(PATH)? != job
                || node.required("step", PATH)?.unsigned(PATH)? != walk.step as u64
            {
                return Err(invalid("native/public construction walk mismatch"));
            }
            let path = node.required("path", PATH)?.array(PATH)?;
            if path.len() != walk.path.len() {
                return Err(invalid(
                    "native/public construction path membership mismatch",
                ));
            }
            for (p, expected) in path.iter().zip(&walk.path) {
                let pair = p.array(PATH)?;
                if pair.len() != 2
                    || pair[0].number(PATH)? != expected.x.get()
                    || pair[1].number(PATH)? != expected.y.get()
                {
                    return Err(invalid("native/public construction path mismatch"));
                }
            }
            if arrived && walk.step + 1 != walk.path.len() {
                return Err(invalid("construction arrived path coverage"));
            }
        }
        _ => return Err(invalid("construction walk presence coverage")),
    }
    Ok(positioned)
}
