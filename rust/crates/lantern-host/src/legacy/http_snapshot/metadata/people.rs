//! Frozen personal and population presentation, checked against native owners.
//! Native batch IDs and pending tool receipts are never public inventory.
use super::*;
use crate::{persistence::TownState, population::Population, social::Social};
use std::collections::BTreeMap;

fn decoded<T: serde::de::DeserializeOwned>(node: &Node) -> Result<T, BoundaryError> {
    serde_json::from_slice(&raw(node)).map_err(|_| invalid("personal field type coverage"))
}
fn optional_number(node: &Node, key: &str, expected: f64) -> Result<(), BoundaryError> {
    if node
        .optional(key, PATH)?
        .map(|n| n.nonnegative_number(PATH))
        .transpose()?
        .unwrap_or(0.)
        != expected
    {
        return Err(invalid("native/public personal number mismatch"));
    }
    Ok(())
}
fn count_map(node: Option<&Node>, expected: &BTreeMap<String, u64>) -> Result<(), BoundaryError> {
    let values = node.map(|n| n.object(PATH)).transpose()?.unwrap_or(&[]);
    if values.len() != expected.len() {
        return Err(invalid("native/public item membership mismatch"));
    }
    for (key, value) in values {
        if expected.get(key).copied() != Some(value.unsigned(PATH)?) {
            return Err(invalid("native/public item count mismatch"));
        }
    }
    Ok(())
}
pub(super) fn population(town: &Node, population: &Population) -> Result<(), BoundaryError> {
    // A pending return is still owned by Population. It cannot also appear in
    // the founder's bag until the transaction drains the native receipt.
    if !population.returned_tools().is_empty() {
        return Err(invalid("undrained population tool receipt coverage"));
    }
    let history = population.history();
    for (key, number) in [
        ("food_shortage_seconds", history.food_shortage_seconds),
        ("housing_shortage_seconds", history.housing_shortage_seconds),
        ("arrival_delay_seconds", history.arrival_delay_seconds),
    ] {
        optional_number(town, key, number)?;
    }
    if town
        .optional("departure_count", PATH)?
        .map(|n| n.unsigned(PATH))
        .transpose()?
        .unwrap_or(0)
        != history.departure_count
    {
        return Err(invalid("native/public departure count mismatch"));
    }
    match (
        town.optional("last_departure", PATH)?,
        &history.last_departure,
    ) {
        (None, None) => (),
        (Some(node), Some(expected))
            if decoded::<crate::population::Departure>(node)? == *expected => {}
        _ => return Err(invalid("native/public departure history mismatch")),
    }
    Ok(())
}
pub(super) fn founder(town: &Node, social: Option<&Social>) -> Result<(), BoundaryError> {
    let player = town.required("player", PATH)?;
    let jobs: Vec<_> = crate::resources::catalog()["rules"]["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let mut allowed = vec!["x", "y", "facing", "boat", "boat_level"];
    if social.is_some() {
        allowed.extend([
            "items",
            "worker_tools",
            "equipment",
            "keepsakes",
            "activity",
            "activity_status",
        ]);
        allowed.extend(jobs.iter().map(String::as_str));
    }
    fields(player, &allowed)?;
    let Some(social) = social else {
        if town.optional("local_visits", PATH)?.is_some() {
            return Err(invalid("missing native visit owner coverage"));
        }
        return Ok(());
    };
    let native = social.player();
    same_number(player, "facing", native.facing as f64)?;
    count_map(player.optional("items", PATH)?, &native.items)?;
    count_map(player.optional("worker_tools", PATH)?, &native.worker_tools)?;
    let equipment: BTreeMap<String, String> = player
        .optional("equipment", PATH)?
        .map(decoded)
        .transpose()?
        .unwrap_or_default();
    if equipment != native.equipment {
        return Err(invalid("native/public equipped item mismatch"));
    }
    let keepsakes: Vec<String> = player
        .optional("keepsakes", PATH)?
        .map(decoded)
        .transpose()?
        .unwrap_or_default();
    if keepsakes != native.keepsakes {
        return Err(invalid("native/public keepsake mismatch"));
    }
    if native.practice.keys().any(|key| !jobs.contains(key)) {
        return Err(invalid("native founder practice coverage"));
    }
    for job in jobs {
        optional_number(
            player,
            &job,
            native.practice.get(&job).copied().unwrap_or(0.),
        )?;
    }
    let status: Option<String> = player
        .optional("activity_status", PATH)?
        .map(decoded)
        .transpose()?
        .flatten();
    if status != native.activity_status {
        return Err(invalid("native/public activity status mismatch"));
    }
    match (
        player.optional("activity", PATH)?.filter(|n| !n.is_null()),
        &native.activity,
    ) {
        (None, None) => (),
        (Some(node), Some(activity)) => {
            fields(node, &["job", "kind", "x", "y"])?;
            if node.required("job", PATH)?.string(PATH)? != activity.job
                || node.required("kind", PATH)?.string(PATH)? != activity.kind
            {
                return Err(invalid("native/public founder activity mismatch"));
            }
            for (axis, coordinate) in ["x", "y"].into_iter().zip(activity.tile) {
                if node.required(axis, PATH)?.tile(PATH)? != coordinate {
                    return Err(invalid("native/public founder target mismatch"));
                }
            }
        }
        _ => return Err(invalid("native/public founder activity presence mismatch")),
    }
    let visits = town
        .optional("local_visits", PATH)?
        .map(|n| n.object(PATH))
        .transpose()?
        .unwrap_or(&[]);
    if visits.len() != social.visits().len() {
        return Err(invalid("native/public visit membership mismatch"));
    }
    for (key, node) in visits {
        fields(
            node,
            &[
                "met",
                "gift_claimed",
                "story_stage",
                "story_rocks",
                "trade_remaining",
            ],
        )?;
        let native = social
            .visits()
            .get(key)
            .ok_or_else(|| invalid("native/public visit owner mismatch"))?;
        if !native.extra.is_empty() || decoded::<crate::social::Visit>(node)? != *native {
            return Err(invalid("native/public visit state coverage"));
        }
    }
    Ok(())
}
pub(super) fn boat(
    player: &Node,
    native: Option<&crate::fishing::ExplorationBoat>,
) -> Result<(), BoundaryError> {
    if native.is_none()
        && (player.optional("boat", PATH)?.is_some()
            || player.optional("boat_level", PATH)?.is_some())
    {
        return Err(invalid("missing native exploration boat coverage"));
    }
    let owned = player
        .optional("boat", PATH)?
        .map(|n| n.boolean(PATH))
        .transpose()?
        .unwrap_or(false);
    let tier = player
        .optional("boat_level", PATH)?
        .map(|n| n.unsigned(PATH))
        .transpose()?
        .unwrap_or(u64::from(owned));
    let expected = native.map_or(0, |boat| boat.tier());
    if owned != (expected > 0) || tier != u64::from(expected) {
        return Err(invalid("native/public exploration boat mismatch"));
    }
    Ok(())
}
pub(super) fn progression(
    state: &TownState,
    rules: &crate::building::Rules,
) -> Result<(), BoundaryError> {
    if rules.progression == crate::building::Progression::default() {
        return Ok(());
    }
    let population = state
        .population
        .as_ref()
        .ok_or_else(|| invalid("native progression population coverage"))?;
    let workers: Vec<_> = population
        .residents()
        .iter()
        .map(|r| r.worker.clone())
        .collect();
    let founder = state
        .social
        .as_ref()
        .map(|social| {
            let mut worker = crate::progression::Worker::new("idle").unwrap();
            for (job, n) in &social.player().practice {
                worker
                    .set_practice(
                        job,
                        lantern_core::units::Practice::new(*n)
                            .map_err(|_| invalid("founder practice coverage"))?,
                    )
                    .map_err(|_| invalid("founder job coverage"))?;
            }
            Ok::<_, BoundaryError>(worker)
        })
        .transpose()?;
    let expected = crate::progression::construction_evidence(
        &state.ledger,
        &workers,
        founder.as_ref(),
        0,
        state.ledger.market_sales(),
    )
    .map_err(|_| invalid("native progression evidence coverage"))?;
    if rules.progression != expected {
        return Err(invalid("native construction progression owner mismatch"));
    }
    Ok(())
}
