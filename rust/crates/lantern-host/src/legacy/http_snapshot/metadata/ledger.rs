//! Public lifetime production, caps and minute activity agree with the ledger.
use super::*;
use crate::resources::{Ledger, ResourceKey, Stock};
use lantern_core::units::Quantity;

pub(super) fn produced(town: &Node) -> Result<Stock, BoundaryError> {
    let Some(node) = town.optional("produced", PATH)? else {
        return Ok(Stock::default());
    };
    stock(node)?;
    Stock::from_entries(
        node.object(PATH)?
            .iter()
            .map(|(key, value)| {
                Ok((
                    ResourceKey::parse(key).map_err(|_| invalid("production resource coverage"))?,
                    Quantity::new(value.nonnegative_number(PATH)?)
                        .map_err(|_| invalid("production amount"))?,
                ))
            })
            .collect::<Result<Vec<_>, BoundaryError>>()?,
    )
    .map_err(|_| invalid("production stock coverage"))
}
fn amounts(
    node: &Node,
    expected: &std::collections::BTreeMap<String, f64>,
) -> Result<(), BoundaryError> {
    if node.object(PATH)?.len() != expected.len() {
        return Err(invalid("native/public history membership mismatch"));
    }
    for (key, amount) in node.object(PATH)? {
        if expected.get(key).copied() != Some(amount.nonnegative_number(PATH)?) {
            return Err(invalid("native/public history mismatch"));
        }
    }
    Ok(())
}
pub(super) fn history(town: &Node, ledger: &Ledger) -> Result<(), BoundaryError> {
    if produced(town)?.entries() != ledger.produced().entries() {
        return Err(invalid("native/public production mismatch"));
    }
    let sales = town
        .optional("market_sales", PATH)?
        .map(|n| n.nonnegative_number(PATH))
        .transpose()?
        .unwrap_or(0.);
    if sales != ledger.market_sales().get() {
        return Err(invalid("native/public market sales mismatch"));
    }
    let caps: std::collections::BTreeMap<_, _> = ledger
        .caps()
        .iter()
        .map(|(k, n)| (k.as_str().to_owned(), n.get()))
        .collect();
    if let Some(rules) = town.optional("rules", PATH)? {
        fields(rules, &["production_caps"])?;
        if let Some(node) = rules.optional("production_caps", PATH)? {
            amounts(node, &caps)?;
        } else if !caps.is_empty() {
            return Err(invalid("missing public caps"));
        }
    } else if !caps.is_empty() {
        return Err(invalid("missing public rules"));
    }
    let activity = ledger.activity();
    if let Some(node) = town.optional("resource_activity", PATH)? {
        fields(node, &["seconds", "buckets"])?;
        same_number(node, "seconds", activity.seconds())?;
        let buckets = node.required("buckets", PATH)?.array(PATH)?;
        if buckets.len() != activity.buckets().len() {
            return Err(invalid("native/public activity membership"));
        }
        for (node, bucket) in buckets.iter().zip(activity.buckets()) {
            fields(node, &["at", "in", "out"])?;
            same_number(node, "at", bucket.at)?;
            amounts(node.required("in", PATH)?, &bucket.incoming)?;
            amounts(node.required("out", PATH)?, &bucket.outgoing)?;
        }
    } else if activity.seconds() != 0. || !activity.buckets().is_empty() {
        return Err(invalid("missing public resource activity"));
    }
    Ok(())
}

/// Public aggregate balances are mirrors of spendable stock, never inputs or
/// pending deliveries. Python keeps non-wood/food/coin quantities under `goods`.
pub(super) fn inventory(
    town: &Node,
    overview: &std::collections::BTreeMap<String, crate::resources::ResourceRow>,
) -> Result<(), BoundaryError> {
    for (key, expected) in [
        ("inventory_version", 1),
        ("food_balance_version", 2),
        ("market_storage_version", 1),
    ] {
        if let Some(value) = town.optional(key, PATH)?
            && value.unsigned(PATH)? != expected
        {
            return Err(invalid("inventory migration coverage"));
        }
    }
    let goods = town.optional("goods", PATH)?;
    if let Some(goods) = goods {
        stock(goods)?;
        if goods
            .object(PATH)?
            .iter()
            .any(|(k, _)| ["wood", "coins"].contains(&k.as_str()))
        {
            return Err(invalid("noncanonical inventory mirror"));
        }
    }
    for (key, row) in overview {
        let value = if ["wood", "food", "coins"].contains(&key.as_str()) {
            town.optional(key, PATH)?
        } else {
            goods.map(|n| n.optional(key, PATH)).transpose()?.flatten()
        };
        if value
            .map(|n| n.nonnegative_number(PATH))
            .transpose()?
            .unwrap_or(0.)
            != row.free
        {
            return Err(invalid("native/public inventory mismatch"));
        }
        // Retain the previously admitted explicit top-level resource form only
        // when it also agrees with the canonical Python balance.
        if let Some(value) = town.optional(key, PATH)?
            && value.nonnegative_number(PATH)? != row.free
        {
            return Err(invalid("native/public inventory mismatch"));
        }
    }
    Ok(())
}
fn values(stock: &Stock) -> std::collections::BTreeMap<String, f64> {
    stock
        .entries()
        .iter()
        .map(|(k, n)| (k.as_str().into(), n.get()))
        .collect()
}
fn optional_amounts(
    node: Option<&Node>,
    expected: &std::collections::BTreeMap<String, f64>,
) -> Result<(), BoundaryError> {
    let mut found = std::collections::BTreeMap::new();
    if let Some(node) = node {
        for (key, amount) in node.object(PATH)? {
            if key != "food" {
                ResourceKey::parse(key).map_err(|_| invalid("unknown storage resource"))?;
            }
            let n = amount.nonnegative_number(PATH)?;
            if n != 0. {
                found.insert(key.clone(), n);
            }
        }
    }
    let expected: std::collections::BTreeMap<_, _> = expected
        .iter()
        .filter(|(_, n)| **n != 0.)
        .map(|(k, n)| (k.clone(), *n))
        .collect();
    if found != expected {
        return Err(invalid("native/public storage mismatch"));
    }
    Ok(())
}
pub(super) fn building(
    record: &Node,
    building: &lantern_core::model::Building,
    site: &crate::building::Site,
    ledger: &Ledger,
) -> Result<(), BoundaryError> {
    let store = ledger
        .store(&building.id)
        .map_err(|_| invalid("missing native storage owner"))?;
    let mut policy = crate::resources::StoragePolicy::new(&site.kind);
    policy.level = site.level;
    policy.equipment = site.equipment.clone();
    policy.construction = building.construction != ConstructionState::Complete;
    policy.enabled = record
        .optional("storage_enabled", PATH)?
        .filter(|n| !n.is_null())
        .map(|n| {
            n.object(PATH)?
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.boolean(PATH)?)))
                .collect::<Result<_, BoundaryError>>()
        })
        .transpose()?;
    if let Some(node) = record.optional("storage_allocations", PATH)? {
        for (key, value) in node.object(PATH)? {
            policy.legacy_allocations.insert(
                key.clone(),
                Quantity::new(value.nonnegative_number(PATH)?)
                    .map_err(|_| invalid("storage allocation"))?,
            );
        }
    }
    policy
        .validate()
        .map_err(|_| invalid("storage filter coverage"))?;
    if policy != store.policy {
        return Err(invalid("native/public storage policy mismatch"));
    }
    let mut stored = values(&store.stored);
    if store.provisions.get() != 0. {
        stored.insert("food".into(), store.provisions.get());
    }
    // Legacy hall stock may be implicit only for the original wood/provision form.
    if record.optional("stored_goods", PATH)?.is_some() || site.kind != "hall" {
        optional_amounts(record.optional("stored_goods", PATH)?, &stored)?;
    } else if stored.keys().any(|k| k != "wood" && k != "food") {
        return Err(invalid("missing hall inventory presentation"));
    }
    optional_amounts(
        record.optional("output_buffer", PATH)?,
        &values(&store.output),
    )?;
    optional_amounts(
        record.optional("input_buffer", PATH)?,
        &values(&store.inputs),
    )?;
    let pending = store
        .pending_stock()
        .map_err(|_| invalid("pending inventory coverage"))?;
    optional_amounts(record.optional("pending_output", PATH)?, &values(&pending))?;
    let orphan = store
        .pending
        .get(&crate::resources::PendingOwner::Orphan)
        .cloned()
        .unwrap_or_default();
    optional_amounts(record.optional("orphaned_output", PATH)?, &values(&orphan))?;
    // R12 mirrors are optional native facts, not a second physical owner. When
    // retained, demolition stock includes free buffers and reserved ingredients.
    if !site.stored.is_empty() {
        let mut physical = values(&store.inventory().map_err(|_| invalid("store inventory"))?);
        for (k, n) in values(&store.inputs) {
            *physical.entry(k).or_default() += n;
        }
        if store.provisions.get() != 0. {
            physical.insert("food".into(), store.provisions.get());
        }
        let mirror = site
            .stored
            .iter()
            .map(|(k, n)| (k.clone(), n.get()))
            .collect();
        if physical != mirror {
            return Err(invalid("stale native building inventory mirror"));
        }
    }
    if !site.pending.is_empty() {
        let mirror = site
            .pending
            .iter()
            .map(|(k, n)| (k.clone(), n.get()))
            .collect();
        if values(&pending) != mirror {
            return Err(invalid("stale native building pending mirror"));
        }
    }
    Ok(())
}
pub(super) fn worker(
    record: &Node,
    actor: &lantern_core::ids::ActorId,
    ledger: &Ledger,
    town: &lantern_core::model::Town,
    rules: &crate::building::Rules,
) -> Result<(), BoundaryError> {
    let owners: Vec<_> = ledger
        .stores()
        .iter()
        .filter_map(|s| {
            s.pending
                .get(&crate::resources::PendingOwner::Worker(actor.clone()))
                .map(|stock| (s, stock))
        })
        .collect();
    let expected = owners.first().map(|(_, s)| values(s)).unwrap_or_default();
    optional_amounts(record.optional("pending_output", PATH)?, &expected)?;
    match (record.optional("pending_workplace", PATH)?, owners.first()) {
        (None, None) => (),
        (Some(node), Some((store, _))) => {
            fields(node, &["kind", "x", "y"])?;
            let building = town
                .building(&store.id)
                .map_err(|_| invalid("pending workplace owner"))?;
            if node.required("kind", PATH)?.string(PATH)? != rules.sites[&store.id].kind {
                return Err(invalid("pending workplace kind"));
            }
            same_number(node, "x", building.location.x as f64)?;
            same_number(node, "y", building.location.y as f64)?;
        }
        _ => return Err(invalid("missing/stale pending workplace owner")),
    }
    Ok(())
}
