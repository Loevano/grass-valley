//! Additional persisted references whose Python meaning is unambiguous. Unknown
//! gameplay remains raw, explicitly unvalidated; do not infer links from names.
use super::json::Node;
use crate::BoundaryError;
use std::collections::BTreeSet;

fn unique_ids(records: &[Node], path: &str) -> Result<(), BoundaryError> {
    let mut seen = BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        let rp = format!("{path}[{index}].id");
        let id = record
            .required("id", &format!("{path}[{index}]"))?
            .string(&rp)?;
        if id.is_empty() || !seen.insert(id) {
            return Err(BoundaryError::invalid(
                rp,
                "empty or duplicate persisted ID",
            ));
        }
    }
    Ok(())
}

pub(super) fn world(payload: &Node, path: &str) -> Result<usize, BoundaryError> {
    let towns = payload
        .required("towns", path)?
        .object(&format!("{path}.towns"))?;
    let owners = towns
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<BTreeSet<_>>();
    let mut count = 0;
    for (index, (id, town)) in towns.iter().enumerate() {
        let tp = format!("{path}.towns[{index}]");
        if let Some(offers) = town.optional("trade_offers", &tp)? {
            let op = format!("{tp}.trade_offers");
            let offers = offers.array(&op)?;
            unique_ids(offers, &op)?;
            for (index, offer) in offers.iter().enumerate() {
                let op = format!("{op}[{index}]");
                // Older/unknown offer shapes remain private extensions. Validate
                // the canonical Python target field when present, never a guessed alias.
                if let Some(target) = offer.optional("target_town", &op)? {
                    let rp = format!("{op}.target_town");
                    let target = target.string(&rp)?;
                    if &target == id || !owners.contains(target.as_str()) {
                        return Err(BoundaryError::invalid(rp, "invalid trade town owner"));
                    }
                    count += 1;
                }
            }
        }
        for (index, building) in town
            .required("buildings", &tp)?
            .array(&format!("{tp}.buildings"))?
            .iter()
            .enumerate()
        {
            let bp = format!("{tp}.buildings[{index}]");
            if let Some(queue) = building.optional("craft_queue", &bp)? {
                let qp = format!("{bp}.craft_queue");
                unique_ids(queue.array(&qp)?, &qp)?;
            }
        }
    }
    for name in ["quarry", "lanterns"] {
        let qp = format!("{path}.{name}");
        if let Some(quest) = payload.optional(name, path)? {
            let members = quest
                .required("members", &qp)?
                .object(&format!("{qp}.members"))?;
            for (index, (id, member)) in members.iter().enumerate() {
                let mp = format!("{qp}.members[{index}]");
                if !owners.contains(id.as_str()) {
                    return Err(BoundaryError::invalid(mp, "dangling adventure town owner"));
                }
                member.object(&mp)?;
                count += 1;
            }
        }
    }
    Ok(count)
}
