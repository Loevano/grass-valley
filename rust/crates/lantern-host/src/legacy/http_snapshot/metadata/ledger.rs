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
