//! Explicit finite schemas for public presentation. A private archive is never
//! serialized on the strength of preservation alone. Unknown nested fields fail.
use super::*;
use crate::persistence::{RestoredModel, RestoredWorld};
mod buildings;
mod ledger;
mod people;

use lantern_core::{
    model::{ConstructionState, Task, TravelIntent},
    units::Tile,
};

fn fields(node: &Node, allowed: &[&str]) -> Result<(), BoundaryError> {
    for (key, _) in node.object(PATH)? {
        if !allowed.contains(&key.as_str()) {
            return Err(invalid("missing public metadata field coverage"));
        }
    }
    Ok(())
}
trait Number {
    fn number(&self, path: &str) -> Result<f64, BoundaryError>;
}
impl Number for Node {
    fn number(&self, path: &str) -> Result<f64, BoundaryError> {
        let Node::Scalar(raw) = self else {
            return Err(BoundaryError::invalid(path, "expected finite number"));
        };
        raw.get()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| BoundaryError::invalid(path, "expected finite number"))
    }
}
fn encoded<E>(result: Result<String, E>) -> Result<String, BoundaryError> {
    result.map_err(|_| invalid("native owner encoding coverage"))
}
fn point_pair(node: &Node) -> Result<(), BoundaryError> {
    let pair = node.array(PATH)?;
    if pair.len() != 2 {
        return Err(invalid("coordinate pair"));
    }
    for number in pair {
        number.number(PATH)?;
    }
    Ok(())
}
fn numbers(node: &Node, keys: &[&str]) -> Result<(), BoundaryError> {
    for key in keys {
        if let Some(value) = node.optional(key, PATH)? {
            value.number(PATH)?;
        }
    }
    Ok(())
}
fn strings(node: &Node, keys: &[&str]) -> Result<(), BoundaryError> {
    for key in keys {
        if let Some(value) = node.optional(key, PATH)? {
            value.string(PATH)?;
        }
    }
    Ok(())
}
fn booleans(node: &Node, keys: &[&str]) -> Result<(), BoundaryError> {
    for key in keys {
        if let Some(value) = node.optional(key, PATH)? {
            value.boolean(PATH)?;
        }
    }
    Ok(())
}
pub(super) fn npc(node: &Node) -> Result<(), BoundaryError> {
    fields(
        node,
        &[
            "name",
            "role",
            "x",
            "y",
            "dialogue",
            "biome",
            "village",
            "routine",
            "ambient_stop",
        ],
    )?;
    strings(node, &["name", "role", "dialogue", "biome", "village"])?;
    node.required("name", PATH)?.string(PATH)?;
    node.required("role", PATH)?.string(PATH)?;
    node.required("x", PATH)?.tile(PATH)?;
    node.required("y", PATH)?.tile(PATH)?;
    if let Some(stop) = node.optional("ambient_stop", PATH)? {
        point_pair(stop)?;
    }
    if let Some(routine) = node.optional("routine", PATH)? {
        fields(
            routine,
            &[
                "x", "y", "path", "stop", "wait", "elapsed", "phase", "job", "facing", "origin",
                "paused", "goal", "source",
            ],
        )?;
        numbers(routine, &["x", "y", "stop", "wait", "elapsed", "facing"])?;
        strings(routine, &["phase", "job"])?;
        booleans(routine, &["paused"])?;
        for point in routine.required("path", PATH)?.array(PATH)? {
            point_pair(point)?;
        }
        for key in ["origin", "goal"] {
            if let Some(point) = routine.optional(key, PATH)?.filter(|p| !p.is_null()) {
                point_pair(point)?;
            }
        }
        if let Some(source) = routine.optional("source", PATH)?.filter(|p| !p.is_null()) {
            check_point(source)?;
        }
    }
    Ok(())
}
pub(super) fn village(node: &Node) -> Result<(), BoundaryError> {
    fields(
        node,
        &[
            "name",
            "biome",
            "x",
            "y",
            "layout_version",
            "life_version",
            "swamp_version",
        ],
    )?;
    strings(node, &["name", "biome"])?;
    node.required("name", PATH)?.string(PATH)?;
    for key in ["x", "y"] {
        node.required(key, PATH)?.tile(PATH)?;
    }
    for (key, max) in [
        ("layout_version", 2),
        ("life_version", 1),
        ("swamp_version", 2),
    ] {
        if node
            .optional(key, PATH)?
            .map(|n| n.unsigned(PATH))
            .transpose()?
            .is_some_and(|v| v > max)
        {
            return Err(invalid("unsupported village presentation version"));
        }
    }
    Ok(())
}
pub(super) fn adventure(node: &Node, lanterns: bool, towns: &Node) -> Result<(), BoundaryError> {
    let allowed: &[&str] = if lanterns {
        &["members", "elapsed", "completed", "message"]
    } else {
        &[
            "members",
            "started",
            "completed",
            "gate_latched",
            "sequence",
            "runes_solved",
            "message",
        ]
    };
    fields(node, allowed)?;
    node.required("message", PATH)?.string(PATH)?;
    node.required("completed", PATH)?.boolean(PATH)?;
    if lanterns {
        node.required("elapsed", PATH)?.nonnegative_number(PATH)?;
    } else {
        for key in ["started", "gate_latched", "runes_solved"] {
            node.required(key, PATH)?.boolean(PATH)?;
        }
        let sequence = node.required("sequence", PATH)?.array(PATH)?;
        if sequence.len() > 3
            || sequence
                .iter()
                .zip(["sun", "leaf", "moon"])
                .any(|(n, expected)| n.string(PATH).ok().as_deref() != Some(expected))
        {
            return Err(invalid("unsupported quarry sequence"));
        }
    }
    for (town, member) in node.required("members", PATH)?.object(PATH)? {
        if towns.optional(town, PATH)?.is_none() {
            return Err(invalid("adventure member owner"));
        }
        fields(
            member,
            if lanterns {
                &["x", "y", "inside", "checkpoint", "hits", "last_step"]
            } else {
                &["x", "y", "inside"]
            },
        )?;
        let x = member.required("x", PATH)?.unsigned(PATH)?;
        let y = member.required("y", PATH)?.unsigned(PATH)?;
        if x == 0 || x > if lanterns { 11 } else { 9 } || y == 0 || y > if lanterns { 3 } else { 5 }
        {
            return Err(invalid("adventure position"));
        }
        member.required("inside", PATH)?.boolean(PATH)?;
        if !lanterns {
            let layout = crate::http::catalogs()["quarry_catalog"]["layout"]
                .as_array()
                .unwrap();
            if layout[y as usize].as_str().unwrap().as_bytes()[x as usize] == b'#' {
                return Err(invalid("quarry member inside wall"));
            }
        }
        if lanterns {
            let checkpoint = member.required("checkpoint", PATH)?.unsigned(PATH)?;
            if ![1, 6].contains(&checkpoint) {
                return Err(invalid("lantern checkpoint"));
            }
            member.required("hits", PATH)?.unsigned(PATH)?;
            if let Some(step) = member.optional("last_step", PATH)? {
                step.nonnegative_number(PATH)?;
            }
        }
    }
    Ok(())
}
pub(super) fn lantern_snapshot(world: &Node) -> Result<Node, BoundaryError> {
    let Some(quest) = world.optional("lanterns", PATH)? else {
        return Ok(scalar(serde_json::Value::Null));
    };
    let mut result = quest.clone();
    let completed = quest.required("completed", PATH)?.boolean(PATH)?;
    let inside = quest
        .required("members", PATH)?
        .object(PATH)?
        .iter()
        .filter(|(_, m)| m.required("inside", PATH).unwrap().boolean(PATH).unwrap())
        .count();
    set(&mut result, "playing", scalar(!completed && inside >= 2));
    let elapsed = quest.required("elapsed", PATH)?.nonnegative_number(PATH)?;
    let mut hazards = Vec::new();
    for (start, end, offset) in [(3, 5, 0.), (7, 10, 1.2)] {
        let phase = elapsed + offset;
        let wave = (phase / 3.4).floor();
        let beat = phase % 3.4;
        let state = if beat < 2. {
            "warning"
        } else if beat < 2.6 {
            "strike"
        } else {
            "clear"
        };
        let safe = wave % 3. + 1.;
        if state != "clear" {
            for x in start..=end {
                for y in 1..4 {
                    if y as f64 != safe {
                        hazards.push(scalar(serde_json::json!({"x":x,"y":y,"state":state})));
                    }
                }
            }
        }
    }
    set(&mut result, "hazards", Node::Array(hazards));
    Ok(result)
}
fn same_number(node: &Node, key: &str, expected: f64) -> Result<(), BoundaryError> {
    if node.required(key, PATH)?.number(PATH)? != expected {
        return Err(invalid("native/public state mismatch"));
    }
    Ok(())
}
fn stock(node: &Node) -> Result<(), BoundaryError> {
    for (key, value) in node.object(PATH)? {
        crate::resources::ResourceKey::parse(key)
            .map_err(|_| invalid("unknown public resource"))?;
        value.nonnegative_number(PATH)?;
    }
    Ok(())
}
/// Native construction, population and ledger owners establish membership and
/// values. Only absent presentation facts (names, wire field presence) come from
/// the explicitly checked archive. Active work lacking a wire adapter refuses.
pub(super) fn towns(world: &RestoredWorld, payload: &Node) -> Result<(), BoundaryError> {
    let entries = payload.required("towns", PATH)?.object(PATH)?;
    let model = world.model.model();
    if entries.len() != model.towns().len() || entries.len() != world.towns.len() {
        return Err(invalid("native/public town membership mismatch"));
    }
    if entries.is_empty() {
        return Ok(());
    }
    let RestoredModel::Building(building_world) = &world.model else {
        return Err(invalid("missing native town presentation coverage"));
    };
    for (key, town) in entries {
        let native = model
            .towns()
            .iter()
            .find(|t| t.owner().town.as_str() == key)
            .ok_or_else(|| invalid("native/public town owner mismatch"))?;
        let state = world
            .towns
            .iter()
            .find(|s| s.ledger.owner() == native.owner())
            .ok_or_else(|| invalid("native/public ledger owner mismatch"))?;
        let rules = &building_world.rules()[&native.owner().town];
        if !building_world.builders().is_empty() {
            return Err(invalid("builder playback public coverage"));
        }
        people::progression(state, rules)?;
        // Every offline owner must have an explicit adapter before it can enter
        // the read host. Compare canonical owner saves, not a guessed empty list.
        let empty = crate::persistence::TownState::empty(state.ledger.clone());
        if encoded(state.trips.encode(native, &state.ledger))?
            != encoded(empty.trips.encode(native, &state.ledger))?
            || encoded(state.woodcutters.encode(native))?
                != encoded(empty.woodcutters.encode(native))?
            || encoded(state.processors.encode(native))?
                != encoded(empty.processors.encode(native))?
            || encoded(state.fields.encode(native, &state.ledger))?
                != encoded(empty.fields.encode(native, &state.ledger))?
            || encoded(state.orchards.encode(native, &state.ledger))?
                != encoded(empty.orchards.encode(native, &state.ledger))?
            || encoded(state.fishers.encode(native))? != encoded(empty.fishers.encode(native))?
            || encoded(state.boats.encode(native))? != encoded(empty.boats.encode(native))?
            || state.livestock.is_some()
            || !state.ledger.held().is_empty()
        {
            return Err(invalid("missing active-work public coverage"));
        }
        let mut allowed = vec![
            "name",
            "level",
            "town_origin",
            "player",
            "buildings",
            "villagers",
            "sandbox",
            "wood_chain_version",
            "worker_capacity_version",
            "grain_chain_version",
            "acre_upgrade_version",
            "harvested",
            "achievements",
            "roads",
            "upgrade_remaining",
            "produced",
            "market_sales",
            "rules",
            "resource_activity",
            "goods",
            "inventory_version",
            "food_balance_version",
            "market_storage_version",
            "local_visits",
            "food_shortage_seconds",
            "housing_shortage_seconds",
            "arrival_delay_seconds",
            "departure_count",
            "last_departure",
        ];
        let resources: Vec<String> = crate::resources::catalog()["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .chain(["food".into()])
            .collect();
        allowed.extend(resources.iter().map(String::as_str));
        fields(town, &allowed)?;
        town.required("name", PATH)?.string(PATH)?;
        same_number(town, "level", rules.level as f64)?;
        let origin = town.required("town_origin", PATH)?;
        check_point(origin)?;
        same_number(origin, "x", rules.origin.x as f64)?;
        same_number(origin, "y", rules.origin.y as f64)?;
        if town
            .optional("sandbox", PATH)?
            .map(|n| n.boolean(PATH))
            .transpose()?
            .unwrap_or(false)
            != rules.sandbox
        {
            return Err(invalid("native/public mode mismatch"));
        }
        for version in [
            "wood_chain_version",
            "worker_capacity_version",
            "grain_chain_version",
            "acre_upgrade_version",
        ] {
            if town.required(version, PATH)?.unsigned(PATH)? != 1 {
                return Err(invalid("town migration coverage"));
            }
        }
        if town.required("harvested", PATH)?.unsigned(PATH)? != 0 {
            return Err(invalid("harvest history coverage"));
        }
        if !town.required("achievements", PATH)?.array(PATH)?.is_empty() {
            return Err(invalid("achievement coverage"));
        }
        let overview = state
            .ledger
            .overview()
            .map_err(|_| invalid("native ledger coverage"))?;
        ledger::inventory(town, &overview)?;
        ledger::history(town, &state.ledger)?;
        let player = town.required("player", PATH)?;
        people::founder(town, state.social.as_ref())?;
        people::boat(player, state.exploration.as_ref())?;
        same_number(player, "x", rules.player.x.get())?;
        same_number(player, "y", rules.player.y.get())?;
        if ![-1., 1.].contains(&player.required("facing", PATH)?.number(PATH)?) {
            return Err(invalid("founder facing"));
        }
        let buildings = town.required("buildings", PATH)?.array(PATH)?;
        if buildings.len() != native.buildings().len() {
            return Err(invalid("native/public building membership"));
        }
        for (record, building) in buildings.iter().zip(native.buildings()) {
            buildings::check(record, building, &rules.sites[&building.id])?;
            let site = &rules.sites[&building.id];
            if record.required("kind", PATH)?.string(PATH)? != site.kind {
                return Err(invalid("native/public building kind"));
            }
            same_number(record, "x", building.location.x as f64)?;
            same_number(record, "y", building.location.y as f64)?;
            let footprint = record.required("footprint", PATH)?.array(PATH)?;
            if footprint.len() != 2
                || footprint[0].unsigned(PATH)? != site.dimensions.0 as u64
                || footprint[1].unsigned(PATH)? != site.dimensions.1 as u64
            {
                return Err(invalid("native/public footprint"));
            }
            if record
                .optional("orientation", PATH)?
                .map(|n| n.unsigned(PATH))
                .transpose()?
                .unwrap_or(0)
                != site.orientation as u64
            {
                return Err(invalid("native/public orientation"));
            }
            ledger::building(record, building, site, &state.ledger)?;
        }
        buildings::roads(town, &rules.roads)?;
        buildings::hall_remaining(town, native, rules)?;
        let villagers = town.required("villagers", PATH)?.array(PATH)?;
        if villagers.len() != native.actors().len() {
            return Err(invalid("native/public actor membership"));
        }
        let population = state
            .population
            .as_ref()
            .ok_or_else(|| invalid("missing native population presentation"))?;
        people::population(town, population)?;
        if population.residents().len() != villagers.len() {
            return Err(invalid("population history public coverage"));
        }
        for ((record, actor), resident) in villagers
            .iter()
            .zip(native.actors())
            .zip(population.residents())
        {
            let mut allowed = vec![
                "name",
                "job",
                "x",
                "y",
                "facing",
                "talents",
                "sandbox",
                "worker_tool",
                "pending_output",
                "pending_workplace",
            ];
            let jobs: Vec<_> = crate::resources::catalog()["rules"]["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|j| j.as_str().unwrap().to_owned())
                .collect();
            allowed.extend(jobs.iter().map(String::as_str));
            fields(record, &allowed)?;
            ledger::worker(record, &resident.actor, &state.ledger, native, rules)?;
            let actor = actor.state();
            if actor.id != resident.actor
                || actor.task != Task::Idle
                || actor.travel != TravelIntent::None
                || !actor.cargo.goods().is_empty()
                || resident.assignment.is_some()
                || resident.worker.job() != "idle"
            {
                return Err(invalid("active actor public coverage"));
            }
            let tool: Option<String> = record
                .optional("worker_tool", PATH)?
                .map(|n| n.string(PATH))
                .transpose()?;
            if tool != resident.worker.tool {
                return Err(invalid("native/public resident tool mismatch"));
            }
            if record.required("name", PATH)?.string(PATH)? != resident.name
                || record.required("job", PATH)?.string(PATH)? != resident.worker.job()
            {
                return Err(invalid("native/public resident mismatch"));
            }
            if let Some(worker) = state.workers.get(&actor.id)
                && worker != &resident.worker
            {
                return Err(invalid("worker owner mismatch"));
            }
            for job in jobs {
                let amount = resident.worker.skills().get(&job).map_or(0., |n| n.get());
                if record
                    .optional(&job, PATH)?
                    .map(|n| n.nonnegative_number(PATH))
                    .transpose()?
                    .unwrap_or(0.)
                    != amount
                {
                    return Err(invalid("native/public practice mismatch"));
                }
            }
            for (key, amount) in [
                ("x", actor.position.x.get()),
                ("y", actor.position.y.get()),
                ("facing", actor.facing.get() as f64),
            ] {
                if record.optional(key, PATH)?.is_some() {
                    same_number(record, key, amount)?;
                } else if amount != if key == "facing" { 1. } else { 0. } {
                    return Err(invalid("missing authoritative actor pose presentation"));
                }
            }
            if record
                .optional("sandbox", PATH)?
                .map(|n| n.boolean(PATH))
                .transpose()?
                .unwrap_or(false)
                != resident.worker.sandbox
            {
                return Err(invalid("native/public worker mode"));
            }
            match (record.optional("talents", PATH)?, resident.worker.talents()) {
                (Some(talents), Some(native)) => {
                    if talents.object(PATH)?.len() != native.len() {
                        return Err(invalid("talent membership"));
                    }
                    for (key, amount) in talents.object(PATH)? {
                        if native.get(key).copied() != Some(amount.number(PATH)?) {
                            return Err(invalid("talent mismatch"));
                        }
                    }
                }
                (None, None) => (),
                _ => return Err(invalid("talent presence mismatch")),
            }
        }
    }
    Ok(())
}
pub(super) fn hall_ready(world: &Node) -> Result<Node, BoundaryError> {
    let mut result = Vec::new();
    let map = world.required("shared_map", PATH)?;
    for (key, town) in world.required("towns", PATH)?.object(PATH)? {
        let level = town.required("level", PATH)?.unsigned(PATH)? as u8;
        let buildings = town
            .required("buildings", PATH)?
            .array(PATH)?
            .iter()
            .map(|b| {
                Ok(crate::progression::HallBuilding {
                    kind: b.required("kind", PATH)?.string(PATH)?,
                    tile: Tile {
                        x: b.required("x", PATH)?.tile(PATH)?,
                        y: b.required("y", PATH)?.tile(PATH)?,
                    },
                    status: if let Some(construction) = b.optional("construction", PATH)? {
                        if construction.optional("upgrade", PATH)?.is_some() {
                            crate::progression::BuildingStatus::Upgrade
                        } else {
                            crate::progression::BuildingStatus::InitialConstruction
                        }
                    } else {
                        crate::progression::BuildingStatus::Complete
                    },
                })
            })
            .collect::<Result<Vec<_>, BoundaryError>>()?;
        let water = map
            .optional("map_resources", PATH)?
            .map(|n| n.array(PATH))
            .transpose()?
            .unwrap_or(&[])
            .iter()
            .filter(|r| {
                r.required("kind", PATH)
                    .and_then(|n| n.string(PATH))
                    .ok()
                    .as_deref()
                    == Some("water")
            })
            .map(|r| {
                Ok(Tile {
                    x: r.required("x", PATH)?.tile(PATH)?,
                    y: r.required("y", PATH)?.tile(PATH)?,
                })
            })
            .collect::<Result<Vec<_>, BoundaryError>>()?;
        let evidence = crate::progression::HallEvidence {
            food: lantern_core::units::Quantity::new(town.required("food", PATH)?.number(PATH)?)
                .map_err(|_| invalid("food"))?,
            produced: ledger::produced(town)?,
            completed_trades: 0,
            market_sales: lantern_core::units::Quantity::new(
                town.optional("market_sales", PATH)?
                    .map(|n| n.nonnegative_number(PATH))
                    .transpose()?
                    .unwrap_or(0.),
            )
            .unwrap(),
        };
        result.push((
            key.clone(),
            scalar(crate::progression::hall_milestone(
                level + 1,
                &evidence,
                &crate::progression::HallContext {
                    buildings,
                    natural_water: water,
                },
            )),
        ));
    }
    Ok(Node::Object(result))
}
