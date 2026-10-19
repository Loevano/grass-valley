//! Explicit ledger DTO; never serialize the authoritative ledger implicitly.
use super::*;
use crate::dto::wire::UniqueMap;
use lantern_core::{ids::EntityIdentity, model::Town};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    world: String,
    town: String,
    coins: f64,
    market_sales: f64,
    stores: Vec<SavedStore>,
    held: Vec<(u64, u64, u8, Goods)>,
    caps: Goods,
    produced: Goods,
    seconds: f64,
    buckets: Vec<(f64, UniqueMap<f64>, UniqueMap<f64>)>,
}
// An ordered pair list preserves withdrawal/order rules and rejects duplicates
// through Stock::from_entries rather than silently overwriting a map key.
type Goods = Vec<(String, f64)>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedStore {
    id: u64,
    kind: String,
    level: u8,
    construction: bool,
    equipment: Vec<String>,
    enabled: Option<UniqueMap<bool>>,
    allocations: Goods,
    stored: Goods,
    output: Goods,
    inputs: Goods,
    pending: Vec<(Option<u64>, Goods)>,
    provisions: f64,
}
fn goods(stock: &Stock) -> Goods {
    stock
        .ordered_entries()
        .map(|(k, n)| (k.as_str().into(), n.get()))
        .collect()
}
fn stock(entries: Goods) -> Result<Stock, Error> {
    Stock::from_entries(
        entries
            .into_iter()
            .map(|(k, n)| {
                Ok((
                    ResourceKey::parse(&k)?,
                    Quantity::new(n).map_err(|_| Error::Overflow)?,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?,
    )
}
fn quantity(n: f64) -> Result<Quantity, Error> {
    Quantity::new(n).map_err(|_| Error::Overflow)
}
fn identity<K>(town: &Town, serial: u64) -> Result<lantern_core::ids::PersistentId<K>, Error> {
    Ok(lantern_core::ids::PersistentId::from_saved(
        EntityIdentity::from_saved(town.owner().clone(), serial).map_err(|_| Error::Ownership)?,
    ))
}
impl Ledger {
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        if town.owner() != self.owner() {
            return Err(Error::Ownership);
        }
        let saved = Saved {
            version: 1,
            world: self.owner.world.as_str().into(),
            town: self.owner.town.as_str().into(),
            coins: self.coins.get(),
            market_sales: self.market_sales.get(),
            stores: self
                .stores
                .iter()
                .map(|s| SavedStore {
                    id: s.id.identity().serial(),
                    kind: s.policy.kind.clone(),
                    level: s.policy.level,
                    construction: s.policy.construction,
                    equipment: s.policy.equipment.iter().cloned().collect(),
                    enabled: s
                        .policy
                        .enabled
                        .as_ref()
                        .map(|m| UniqueMap(m.iter().map(|(k, v)| (k.clone(), *v)).collect())),
                    allocations: s
                        .policy
                        .legacy_allocations
                        .iter()
                        .map(|(k, v)| (k.clone(), v.get()))
                        .collect(),
                    stored: goods(&s.stored),
                    output: goods(&s.output),
                    inputs: goods(&s.inputs),
                    pending: s
                        .pending
                        .iter()
                        .map(|(p, g)| {
                            (
                                match p {
                                    PendingOwner::Worker(a) => Some(a.identity().serial()),
                                    PendingOwner::Orphan => None,
                                },
                                goods(g),
                            )
                        })
                        .collect(),
                    provisions: s.provisions.get(),
                })
                .collect(),
            held: self
                .held
                .values()
                .map(|c| {
                    (
                        c.actor.identity().serial(),
                        c.source.identity().serial(),
                        match c.kind {
                            CargoKind::Collect => 0,
                            CargoKind::Supply => 1,
                            CargoKind::Sell => 2,
                        },
                        goods(&c.goods),
                    )
                })
                .collect(),
            caps: self
                .caps
                .iter()
                .map(|(k, v)| (k.as_str().into(), v.get()))
                .collect(),
            produced: goods(&self.produced),
            seconds: self.activity.seconds(),
            buckets: self
                .activity
                .buckets()
                .iter()
                .map(|b| {
                    (
                        b.at,
                        UniqueMap(b.incoming.iter().map(|(k, v)| (k.clone(), *v)).collect()),
                        UniqueMap(b.outgoing.iter().map(|(k, v)| (k.clone(), *v)).collect()),
                    )
                })
                .collect(),
        };
        let json = serde_json::to_string(&saved).map_err(|_| Error::Overflow)?;
        // Verify cross-owner links even for caller-assembled admission facts.
        Self::decode(&json, town)?;
        Ok(json)
    }
    pub fn decode(json: &str, town: &Town) -> Result<Self, Error> {
        let saved: Saved = serde_json::from_str(json).map_err(|_| Error::InvalidStorage)?;
        if saved.version != 1
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::Ownership);
        }
        let mut stores = Vec::new();
        for s in saved.stores {
            let id = identity(town, s.id)?;
            town.building(&id).map_err(|_| Error::MissingBuilding)?;
            let mut allocations = BTreeMap::new();
            for (k, v) in s.allocations {
                if allocations.insert(k, quantity(v)?).is_some() {
                    return Err(Error::DuplicateResource);
                }
            }
            let equipment: BTreeSet<_> = s.equipment.iter().cloned().collect();
            if equipment.len() != s.equipment.len() {
                return Err(Error::InvalidStorage);
            }
            let policy = StoragePolicy {
                kind: s.kind,
                level: s.level,
                construction: s.construction,
                equipment,
                enabled: s.enabled.map(|m| m.0.into_iter().collect()),
                legacy_allocations: allocations,
            };
            let mut pending = BTreeMap::new();
            for (actor, g) in s.pending {
                let owner = if let Some(a) = actor {
                    let id = identity(town, a)?;
                    town.actor(&id).map_err(|_| Error::Ownership)?;
                    PendingOwner::Worker(id)
                } else {
                    PendingOwner::Orphan
                };
                if pending.insert(owner, stock(g)?).is_some() {
                    return Err(Error::DuplicateIdentity);
                }
            }
            stores.push(Store {
                id,
                policy,
                stored: stock(s.stored)?,
                output: stock(s.output)?,
                inputs: stock(s.inputs)?,
                pending,
                provisions: quantity(s.provisions)?,
            });
        }
        let mut held = Vec::new();
        for (a, s, k, g) in saved.held {
            let actor = identity(town, a)?;
            let source = identity(town, s)?;
            town.actor(&actor).map_err(|_| Error::Ownership)?;
            town.building(&source).map_err(|_| Error::MissingBuilding)?;
            let kind = match k {
                0 => CargoKind::Collect,
                1 => CargoKind::Supply,
                2 => CargoKind::Sell,
                _ => return Err(Error::MissingCargo),
            };
            held.push(HeldCargo {
                actor,
                source,
                kind,
                goods: stock(g)?,
            });
        }
        let mut result = Self::new(town.owner().clone(), quantity(saved.coins)?, stores, held)?;
        result.market_sales = quantity(saved.market_sales)?;
        result.caps = stock(saved.caps)?.entries().clone();
        result.produced = stock(saved.produced)?;
        result.activity = Activity::restore(
            saved.seconds,
            saved
                .buckets
                .into_iter()
                .map(|(at, i, o)| super::super::Bucket {
                    at,
                    incoming: i.0.into_iter().collect(),
                    outgoing: o.0.into_iter().collect(),
                })
                .collect(),
        )?;
        result.overview()?;
        Ok(result)
    }
}
