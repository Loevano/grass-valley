//! Frozen agricultural facts are checked against both agricultural and site owners.
use super::*;
use crate::{
    building::{ChildState, Rules},
    farming::Fields,
    orchards::Orchards,
    persistence::TownState,
};
use lantern_core::model::Town;
use std::collections::BTreeMap;

pub(super) fn check(
    records: &[Node],
    town: &Town,
    rules: &Rules,
    state: &TownState,
) -> Result<(), BoundaryError> {
    let values: Vec<serde_json::Value> = records
        .iter()
        .map(|n| serde_json::from_slice(&raw(n)).map_err(|_| invalid("agricultural JSON coverage")))
        .collect::<Result<_, _>>()?;
    let ids: Vec<_> = town.buildings().iter().map(|b| b.id.clone()).collect();
    // Admission constructs only expected frozen facts. Comparing complete owner
    // encodings also refuses assignments, claims, jobs, rounds and prior work.
    let expected_fields = Fields::admit_legacy(town, &state.ledger, &values, &ids, BTreeMap::new())
        .map_err(|_| invalid("agricultural field public coverage"))?;
    let expected_trees =
        Orchards::admit_legacy(town, &state.ledger, &values, &ids, BTreeMap::new())
            .map_err(|_| invalid("agricultural tree public coverage"))?;
    if encoded(expected_fields.encode(town, &state.ledger))?
        != encoded(state.fields.encode(town, &state.ledger))?
        || encoded(expected_trees.encode(town, &state.ledger))?
            != encoded(state.orchards.encode(town, &state.ledger))?
    {
        return Err(invalid("native/public agricultural state mismatch"));
    }
    for ((record, value), building) in records.iter().zip(&values).zip(town.buildings()) {
        let site = &rules.sites[&building.id];
        let (parent, child) = match site.kind.as_str() {
            "farm" => {
                if record.optional("farm_index", PATH)?.is_some()
                    || record.optional("orchard_index", PATH)?.is_some()
                    || record.optional("tree", PATH)?.is_some()
                {
                    return Err(invalid("unexpected agricultural public field"));
                }
                (None, None)
            }
            "acre" | "orchard_tree" => {
                let field = site.kind == "acre";
                let key = if field { "farm_index" } else { "orchard_index" };
                for forbidden in if field {
                    &["orchard_index", "tree"][..]
                } else {
                    &["farm_index", "crop", "field"][..]
                } {
                    if record.optional(forbidden, PATH)?.is_some() {
                        return Err(invalid("unexpected agricultural public field"));
                    }
                }
                let index = record.required(key, PATH)?.unsigned(PATH)?;
                let parent = usize::try_from(index)
                    .ok()
                    .and_then(|i| town.buildings().get(i))
                    .ok_or_else(|| invalid("agricultural parent index coverage"))?;
                let child = if field {
                    let f = expected_fields
                        .field(&building.id)
                        .map_err(|_| invalid("missing native field coverage"))?;
                    // The importer has already validated the stage; its wire name
                    // is preserved here solely to compare the R12 duplicate mirror.
                    ChildState::Field {
                        stage: value["field"]["stage"]
                            .as_str()
                            .unwrap_or("unplowed")
                            .into(),
                        elapsed: f.state.elapsed,
                        progress: lantern_core::units::Work::new(f.state.progress.get()).unwrap(),
                        cycle: f.state.cycle,
                    }
                } else {
                    let t = expected_trees
                        .tree(&building.id)
                        .ok_or_else(|| invalid("missing native tree coverage"))?;
                    ChildState::Tree {
                        stage: value["tree"]["stage"].as_str().unwrap_or("sapling").into(),
                        elapsed: t.elapsed,
                        fruit: t.fruit,
                    }
                };
                (Some(parent.id.clone()), Some(child))
            }
            _ => {
                for key in ["farm_index", "orchard_index", "crop", "field", "tree"] {
                    if record.optional(key, PATH)?.is_some() {
                        return Err(invalid("unexpected agricultural public field"));
                    }
                }
                (None, None)
            }
        };
        if site.parent != parent || site.child != child {
            return Err(invalid("native/public agricultural site mismatch"));
        }
    }
    Ok(())
}
