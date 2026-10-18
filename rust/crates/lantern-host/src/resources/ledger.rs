//! Single-owner experimental town ledger. All views are derived from physical owners.
//! No town-level mirrored stock, worker pending mirror, routing, or live save adapter.
use super::{
    Activity, Direction, ResourceError as Error, ResourceKey, Stock, StoragePolicy, catalog,
};
use lantern_core::{
    ids::{ActorId, BuildingId, Owner},
    units::{Quantity, Seconds},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PendingOwner {
    Worker(ActorId),
    Orphan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CargoKind {
    Collect,
    Supply,
    Sell,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HeldCargo {
    pub actor: ActorId,
    pub source: BuildingId,
    pub kind: CargoKind,
    pub goods: Stock,
}
/// Explicit offline facts; after admission only the ledger may mutate inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    pub id: BuildingId,
    pub policy: StoragePolicy,
    pub stored: Stock,
    pub output: Stock,
    pub inputs: Stock,
    /// Completed worker output belongs here once, including abandoned output.
    pub pending: BTreeMap<PendingOwner, Stock>,
    /// Actual untyped provisions from old worlds, not an edible-value mirror.
    pub provisions: Quantity,
}
impl Store {
    pub fn new(id: BuildingId, policy: StoragePolicy) -> Self {
        Self {
            id,
            policy,
            stored: Stock::default(),
            output: Stock::default(),
            inputs: Stock::default(),
            pending: BTreeMap::new(),
            provisions: q(0.0),
        }
    }
    pub fn inventory(&self) -> Result<Stock, Error> {
        combined([&self.stored, &self.output])
    }
    pub fn pending_stock(&self) -> Result<Stock, Error> {
        combined(self.pending.values())
    }
    pub fn local_room(&self, key: &str) -> Result<f64, Error> {
        if key != "food" {
            ResourceKey::parse(key)?;
        }
        Ok(self.policy.room(
            key,
            &self.inventory()?,
            &self.pending_stock()?,
            self.provisions.get(),
        ))
    }
    fn complete(&self) -> Result<(), Error> {
        if self.policy.construction {
            Err(Error::Unfinished)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResourceRow {
    pub free: f64,
    pub reserved: f64,
    pub pending: f64,
    pub total: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Ledger {
    owner: Owner,
    coins: Quantity,
    market_sales: Quantity,
    /// Vec retains Python withdrawal/delivery order; IDs provide stable references.
    stores: Vec<Store>,
    held: BTreeMap<ActorId, HeldCargo>,
    caps: BTreeMap<ResourceKey, Quantity>,
    produced: Stock,
    activity: Activity,
}
impl Ledger {
    pub fn new(
        owner: Owner,
        coins: Quantity,
        stores: Vec<Store>,
        cargo: Vec<HeldCargo>,
    ) -> Result<Self, Error> {
        let mut ids = BTreeSet::new();
        let mut workers = BTreeSet::new();
        for store in &stores {
            store
                .id
                .identity()
                .validate_owner(&owner)
                .map_err(|_| Error::Ownership)?;
            if !ids.insert(store.id.identity().serial()) {
                return Err(Error::DuplicateIdentity);
            }
            store.policy.validate()?;
            for goods in [&store.stored, &store.output, &store.inputs]
                .into_iter()
                .chain(store.pending.values())
            {
                items_only(goods)?;
            }
            store.inventory()?;
            store.pending_stock()?;
            for pending in store.pending.keys() {
                if let PendingOwner::Worker(actor) = pending {
                    actor
                        .identity()
                        .validate_owner(&owner)
                        .map_err(|_| Error::Ownership)?;
                    if !workers.insert(actor.clone()) {
                        return Err(Error::DuplicateIdentity);
                    }
                }
            }
        }
        let mut held = BTreeMap::new();
        for cargo in cargo {
            cargo
                .actor
                .identity()
                .validate_owner(&owner)
                .map_err(|_| Error::Ownership)?;
            cargo
                .source
                .identity()
                .validate_owner(&owner)
                .map_err(|_| Error::Ownership)?;
            items_only(&cargo.goods)?;
            if held.insert(cargo.actor.clone(), cargo).is_some() {
                return Err(Error::DuplicateIdentity);
            }
        }
        for actor in workers.iter().chain(held.keys()) {
            if ids.contains(&actor.identity().serial()) {
                return Err(Error::DuplicateIdentity);
            }
        }
        let ledger = Self {
            owner,
            coins,
            market_sales: q(0.0),
            stores,
            held,
            caps: BTreeMap::new(),
            produced: Stock::default(),
            activity: Activity::default(),
        };
        ledger.overview()?;
        Ok(ledger)
    }
    pub fn market_sales(&self) -> Quantity {
        self.market_sales
    }
    /// Explicit offline admission of the saved cumulative sale count. Durable
    /// ledger/history restoration belongs to the R19 owner, not trip JSON.
    pub fn restore_market_sales(&mut self, count: Quantity) {
        self.market_sales = count;
    }
    fn record_sale(&mut self, key: &ResourceKey, amount: Quantity) -> Result<(), Error> {
        let value = Quantity::new(amount.get() * key.sale_price()).map_err(|_| Error::Overflow)?;
        self.coins = Quantity::new(self.coins.get() + value.get()).map_err(|_| Error::Overflow)?;
        self.market_sales =
            Quantity::new(self.market_sales.get() + amount.get()).map_err(|_| Error::Overflow)?;
        self.activity.record(key, amount, Direction::Out)?;
        self.activity
            .record(&ResourceKey::parse("coins")?, value, Direction::In)
    }
    /// Market arrival consumes only physically held sale goods; the trip owner
    /// returns the remainder to its captured source without capacity loss.
    pub fn sell_held(&mut self, actor: &ActorId, sold: &Stock) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            let mut cargo = l.held.remove(actor).ok_or(Error::MissingCargo)?;
            if cargo.kind != CargoKind::Sell {
                return Err(Error::InvalidStorage);
            }
            strict_afford(&cargo.goods, sold)?;
            cargo.goods.spend(sold)?;
            for (key, n) in sold.ordered_entries() {
                l.record_sale(key, *n)?;
            }
            if sum(&cargo.goods) > 0.0 {
                l.held.insert(actor.clone(), cargo);
            }
            Ok(())
        })
    }
    /// Manual quick sales have no worker practice and use free stock only.
    pub fn sell_free(&mut self, key: &ResourceKey, amount: Quantity) -> Result<(), Error> {
        self.apply(|l| {
            if !super::public_catalog()["manual_sellable_goods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some(key.as_str()))
            {
                return Err(Error::InvalidStorage);
            }
            let goods = Stock::from_entries([(key.clone(), amount)])?;
            if !l.can_afford(&goods)? {
                return Err(Error::InsufficientStock);
            }
            l.withdraw(key, amount, true)?;
            l.record_sale(key, amount)
        })
    }
    /// Recheck capacity and coins atomically, charging only actually admitted
    /// imports. Purchase inflow never advances production milestones/caps.
    pub fn purchase(&mut self, key: &ResourceKey, amount: Quantity) -> Result<Quantity, Error> {
        self.apply(|l| {
            let price = 2.0 * key.sale_price();
            let affordable = Quantity::new(amount.get().min(l.coins.get() / price))
                .map_err(|_| Error::Overflow)?;
            let received = l.deliver(key, affordable, false)?;
            l.spend(&Stock::from_entries([(
                ResourceKey::parse("coins")?,
                Quantity::new(received.get() * price).map_err(|_| Error::Overflow)?,
            )])?)?;
            Ok(received)
        })
    }
    pub fn stores(&self) -> &[Store] {
        &self.stores
    }
    pub fn held(&self) -> &BTreeMap<ActorId, HeldCargo> {
        &self.held
    }
    pub fn activity(&self) -> &Activity {
        &self.activity
    }
    pub fn produced(&self) -> &Stock {
        &self.produced
    }
    pub fn caps(&self) -> &BTreeMap<ResourceKey, Quantity> {
        &self.caps
    }
    pub fn store(&self, id: &BuildingId) -> Result<&Store, Error> {
        self.stores
            .iter()
            .find(|s| &s.id == id)
            .ok_or(Error::MissingBuilding)
    }
    fn store_mut(&mut self, id: &BuildingId) -> Result<&mut Store, Error> {
        self.stores
            .iter_mut()
            .find(|s| &s.id == id)
            .ok_or(Error::MissingBuilding)
    }
    fn actor(&self, actor: &ActorId) -> Result<(), Error> {
        actor
            .identity()
            .validate_owner(&self.owner)
            .map_err(|_| Error::Ownership)?;
        if self
            .stores
            .iter()
            .any(|s| s.id.identity().serial() == actor.identity().serial())
        {
            return Err(Error::DuplicateIdentity);
        }
        Ok(())
    }
    /// Mutations are committed only after every quantity/derived total is valid.
    fn apply<T>(&mut self, edit: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        let mut next = self.clone();
        let result = edit(&mut next)?;
        next.overview()?;
        *self = next;
        Ok(result)
    }
    pub fn overview(&self) -> Result<BTreeMap<String, ResourceRow>, Error> {
        let mut rows = BTreeMap::new();
        for name in catalog()["resources"].as_array().unwrap() {
            rows.insert(name.as_str().unwrap().into(), ResourceRow::default());
        }
        rows.insert("food".into(), ResourceRow::default());
        rows.get_mut("coins").unwrap().free = self.coins.get();
        for store in &self.stores {
            add_rows(&mut rows, &store.inventory()?, 0)?;
            add_rows(&mut rows, &store.inputs, 1)?;
            add_rows(&mut rows, &store.pending_stock()?, 2)?;
            rows.get_mut("food").unwrap().free += store.provisions.get();
        }
        for cargo in self.held.values() {
            add_rows(
                &mut rows,
                &cargo.goods,
                if cargo.kind == CargoKind::Collect {
                    2
                } else {
                    1
                },
            )?;
        }
        for row in rows.values_mut() {
            row.total = row.free + row.reserved + row.pending;
            if !row.total.is_finite() {
                return Err(Error::Overflow);
            }
        }
        Ok(rows)
    }
    pub fn free(&self, key: &ResourceKey) -> Result<Quantity, Error> {
        Ok(q(self.overview()?[key.as_str()].free))
    }
    pub fn food(&self) -> Result<Quantity, Error> {
        Ok(q(self.overview()?["food"].free))
    }
    pub fn storage_room(&self, key: &str) -> Result<f64, Error> {
        if key != "food" {
            ResourceKey::parse(key)?;
        }
        self.stores
            .iter()
            .filter(|s| !s.policy.construction)
            .try_fold(0.0, |total, s| finite(total + s.local_room(key)?))
    }
    pub fn storage_limit(&self, key: &str) -> Result<f64, Error> {
        if key != "food" {
            ResourceKey::parse(key)?;
        }
        self.stores
            .iter()
            .filter(|s| !s.policy.construction)
            .try_fold(0.0, |total, s| {
                let allocations = s.policy.allocations();
                let n = if key == "food" {
                    allocations
                        .iter()
                        .filter(|(k, _)| {
                            k.as_str() == "food" || super::public_catalog()["edible"][k].is_number()
                        })
                        .map(|(k, n)| {
                            n * super::public_catalog()["edible"][k].as_f64().unwrap_or(1.0)
                        })
                        .sum()
                } else if s.policy.accepts(key) {
                    allocations.get(key).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                finite(total + n)
            })
    }
    pub fn configure_storage(
        &mut self,
        id: &BuildingId,
        enabled: BTreeMap<String, bool>,
    ) -> Result<(), Error> {
        self.apply(|l| {
            let s = l.store_mut(id)?;
            s.complete()?;
            if s.policy.kind != "storehouse" || enabled.keys().any(|k| !s.policy.accepts(k)) {
                return Err(Error::InvalidStorage);
            }
            s.policy.enabled = Some(
                super::public_catalog()["storage_keys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|v| {
                        let key = v.as_str().unwrap();
                        s.policy
                            .accepts(key)
                            .then(|| (key.into(), enabled.get(key).copied().unwrap_or(false)))
                    })
                    .collect(),
            );
            s.policy.legacy_allocations.clear();
            Ok(())
        })
    }
    /// Structural/equipment changes arrive from the building owner; preserve all goods.
    pub fn update_storage(&mut self, id: &BuildingId, policy: StoragePolicy) -> Result<(), Error> {
        self.apply(|l| {
            policy.validate()?;
            let s = l.store_mut(id)?;
            if s.policy.kind != policy.kind {
                return Err(Error::InvalidStorage);
            }
            s.policy = policy;
            Ok(())
        })
    }
    pub fn configure_cap(&mut self, key: ResourceKey, cap: Option<Quantity>) -> Result<(), Error> {
        if !catalog()["rules"]["production_goods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some(key.as_str()))
        {
            return Err(Error::InvalidCap);
        }
        if let Some(cap) = cap {
            self.caps.insert(key, cap);
        } else {
            self.caps.remove(&key);
        }
        Ok(())
    }
    pub fn production_room(&self, key: &ResourceKey) -> Result<f64, Error> {
        let Some(cap) = self.caps.get(key) else {
            return Ok(f64::INFINITY);
        };
        let mut total = self.free(key)?.get();
        for s in &self.stores {
            total = finite(total + s.pending_stock()?.quantity(key).get())?;
        }
        for cargo in self.held.values().filter(|c| c.kind != CargoKind::Supply) {
            total = finite(total + cargo.goods.quantity(key).get())?;
        }
        Ok((cap.get() - total).max(0.0))
    }
    pub fn can_afford(&self, costs: &Stock) -> Result<bool, Error> {
        for (key, n) in costs.entries() {
            if self.free(key)?.get() + 1e-9 < n.get() {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn spend(&mut self, costs: &Stock) -> Result<(), Error> {
        self.apply(|l| {
            if !l.can_afford(costs)? {
                return Err(Error::InsufficientStock);
            }
            for (key, n) in costs.entries() {
                if key.as_str() == "coins" {
                    l.coins = q((l.coins.get() - n.get()).max(0.0));
                } else {
                    l.withdraw(key, *n, true)?;
                }
                l.activity.record(key, *n, Direction::Out)?;
            }
            Ok(())
        })
    }
    fn withdraw(
        &mut self,
        key: &ResourceKey,
        amount: Quantity,
        tolerant: bool,
    ) -> Result<(), Error> {
        let mut remaining = amount.get();
        for store in &mut self.stores {
            for stock in [&mut store.stored, &mut store.output] {
                let take = remaining.min(stock.quantity(key).get());
                stock.spend(&Stock::from_entries([(key.clone(), q(take))])?)?;
                remaining -= take;
            }
            if remaining <= 1e-9 {
                break;
            }
        }
        if remaining > if tolerant { 1e-9 } else { 0.0 } {
            return Err(Error::InsufficientStock);
        }
        Ok(())
    }
    /// Consume aggregate food in Python's saved catalog order; untyped provisions first.
    pub fn eat(&mut self, amount: Quantity) -> Result<(), Error> {
        self.apply(|l| {
            let mut remaining = l.food()?.get().min(amount.get());
            for s in &mut l.stores {
                let take = remaining.min(s.provisions.get());
                s.provisions = q(s.provisions.get() - take);
                remaining -= take;
                l.activity.record_food(take, Direction::Out)?;
            }
            for name in catalog()["rules"]["edible_order"].as_array().unwrap() {
                let key = ResourceKey::parse(name.as_str().unwrap())?;
                let take = l.free(&key)?.get().min(remaining / key.food_value());
                if take > 0.0 {
                    l.withdraw(&key, q(take), true)?;
                    l.activity.record(&key, q(take), Direction::Out)?;
                    remaining = (remaining - take * key.food_value()).max(0.0);
                }
            }
            Ok(())
        })
    }
    /// External arrival or direct production. Production is clipped to its cap;
    /// imported goods record inflow without milestones and may exceed that cap.
    pub fn deliver(
        &mut self,
        key: &ResourceKey,
        amount: Quantity,
        production: bool,
    ) -> Result<Quantity, Error> {
        self.apply(|l| {
            if key.as_str() == "coins" {
                return Err(Error::InvalidStorage);
            }
            let offered = if production {
                amount.get().min(l.production_room(key)?)
            } else {
                amount.get()
            };
            let mut remaining = offered;
            for s in &mut l.stores {
                if s.policy.construction {
                    continue;
                }
                let take = remaining.min(s.local_room(key.as_str())?);
                if take > 0.0 {
                    s.stored.credit(key.clone(), q(take))?;
                }
                remaining -= take;
            }
            let received = q(offered - remaining);
            l.activity.record(key, received, Direction::In)?;
            if production {
                l.produced.credit(key.clone(), received)?;
            }
            Ok(received)
        })
    }
    /// Reserve actual stock directly; transport arrivals use deposit instead.
    pub fn reserve(
        &mut self,
        source: &BuildingId,
        destination: &BuildingId,
        goods: &Stock,
    ) -> Result<(), Error> {
        self.apply(|l| {
            items_only(goods)?;
            l.store(source)?.complete()?;
            l.store(destination)?.complete()?;
            let inventory = l.store(source)?.inventory()?;
            strict_afford(&inventory, goods)?;
            let total: f64 = l
                .store(destination)?
                .inputs
                .entries()
                .values()
                .map(|v| v.get())
                .sum();
            if total + sum(goods) > l.store(destination)?.policy.input_size() + 1e-9 {
                return Err(Error::Capacity);
            }
            for (key, n) in goods.ordered_entries() {
                take_from(l.store_mut(source)?, key, *n, false)?;
                l.store_mut(destination)?.inputs.credit(key.clone(), *n)?;
            }
            Ok(())
        })
    }
    /// Release obsolete inputs into available output without production or activity.
    pub fn release(&mut self, id: &BuildingId, goods: &Stock) -> Result<(), Error> {
        self.apply(|l| {
            let s = l.store_mut(id)?;
            strict_afford(&s.inputs, goods)?;
            for (k, n) in goods.ordered_entries() {
                s.inputs.transfer_to(&mut s.output, k, *n)?;
            }
            Ok(())
        })
    }
    /// An existing barn grows during an R12 upgrade. Its ordinary storage is
    /// unavailable to hauling then, but Python still records clipped herd output.
    /// The livestock owner must reject initial construction before invoking this.
    pub(crate) fn produce_barn_growth(
        &mut self,
        id: &BuildingId,
        outputs: &Stock,
    ) -> Result<(), Error> {
        self.apply(|ledger| {
            if ledger.store(id)?.policy.kind != "barn" {
                return Err(Error::InvalidStorage);
            }
            let construction = ledger.store(id)?.policy.construction;
            ledger.store_mut(id)?.policy.construction = false;
            ledger.produce(id, None, &Stock::default(), outputs)?;
            ledger.store_mut(id)?.policy.construction = construction;
            Ok(())
        })
    }
    /// Low-level batch accounting. `progression::complete_batch` evaluates current
    /// recipes, personal qualification and practice before calling this operation.
    pub fn produce(
        &mut self,
        id: &BuildingId,
        worker: Option<&ActorId>,
        costs: &Stock,
        outputs: &Stock,
    ) -> Result<(), Error> {
        self.apply(|l| {
            items_only(costs)?;
            items_only(outputs)?;
            if let Some(actor) = worker {
                l.actor(actor)?;
                if l.stores.iter().any(|s| {
                    &s.id != id && s.pending.contains_key(&PendingOwner::Worker(actor.clone()))
                }) {
                    return Err(Error::DuplicateIdentity);
                }
            }
            l.store(id)?.complete()?;
            if !l.store(id)?.inputs.can_afford(costs) {
                return Err(Error::InsufficientStock);
            }
            let s = l.store(id)?;
            let inventory = s.inventory()?;
            let mut pending = s.pending_stock()?;
            for (k, n) in outputs.ordered_entries() {
                if n.get()
                    > s.policy
                        .room(k.as_str(), &inventory, &pending, s.provisions.get())
                        + 1e-9
                {
                    return Err(Error::Capacity);
                }
                if n.get() > l.production_room(k)? + 1e-9 {
                    return Err(Error::ProductionCap);
                }
                pending.credit(k.clone(), *n)?;
            }
            let before = l.store(id)?.inputs.clone();
            l.store_mut(id)?.inputs.spend(costs)?;
            for k in costs.entries().keys() {
                let consumed = before.quantity(k).get() - l.store(id)?.inputs.quantity(k).get();
                l.activity.record(k, q(consumed), Direction::Out)?;
            }
            let s = l.store_mut(id)?;
            let target = if let Some(actor) = worker {
                s.pending
                    .entry(PendingOwner::Worker(actor.clone()))
                    .or_default()
            } else {
                &mut s.output
            };
            for (k, n) in outputs.ordered_entries() {
                target.credit(k.clone(), *n)?;
            }
            for (k, n) in outputs.ordered_entries() {
                l.produced.credit(k.clone(), *n)?;
                l.activity.record(k, *n, Direction::In)?;
            }
            Ok(())
        })
    }
    pub fn drop_off(&mut self, id: &BuildingId, actor: &ActorId) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            let s = l.store_mut(id)?;
            let pending = s
                .pending
                .remove(&PendingOwner::Worker(actor.clone()))
                .ok_or(Error::MissingPending)?;
            for (k, n) in pending.ordered_entries() {
                s.output.credit(k.clone(), *n)?;
            }
            Ok(())
        })
    }
    /// A new gatherer takes responsibility for abandoned finished output. This
    /// changes only physical pending ownership, never production or activity.
    pub(crate) fn adopt_orphan_pending(
        &mut self,
        id: &BuildingId,
        actor: &ActorId,
    ) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            if l.stores.iter().any(|s| {
                &s.id != id && s.pending.contains_key(&PendingOwner::Worker(actor.clone()))
            }) {
                return Err(Error::DuplicateIdentity);
            }
            let s = l.store_mut(id)?;
            if let Some(orphan) = s.pending.remove(&PendingOwner::Orphan) {
                let target = s
                    .pending
                    .entry(PendingOwner::Worker(actor.clone()))
                    .or_default();
                for (k, n) in orphan.ordered_entries() {
                    target.credit(k.clone(), *n)?;
                }
            }
            Ok(())
        })
    }
    /// Adopt only the fish that fits a shared boat hold, preserving species and
    /// insertion order. This transfers ownership without production or activity.
    pub(crate) fn adopt_boat_orphans(
        &mut self,
        id: &BuildingId,
        actor: &ActorId,
        limit: Quantity,
    ) -> Result<(), Error> {
        self.apply(|ledger| {
            ledger.actor(actor)?;
            if ledger.stores.iter().any(|s| {
                &s.id != id && s.pending.contains_key(&PendingOwner::Worker(actor.clone()))
            }) {
                return Err(Error::DuplicateIdentity);
            }
            let store = ledger.store_mut(id)?;
            let Some(orphan) = store.pending.get(&PendingOwner::Orphan) else {
                return Ok(());
            };
            let mut room = limit.get();
            let mut adopted = Stock::default();
            for (key, n) in orphan.ordered_entries() {
                if super::public_catalog()["fish"][key.as_str()].is_object() {
                    let take = n.get().min(room);
                    if take > 0. {
                        adopted.credit(key.clone(), q(take))?;
                        room -= take;
                    }
                }
            }
            store
                .pending
                .get_mut(&PendingOwner::Orphan)
                .unwrap()
                .spend(&adopted)?;
            let remaining = store.pending[&PendingOwner::Orphan]
                .ordered_entries()
                .filter(|(_, n)| n.get() > 0.)
                .map(|(k, n)| (k.clone(), *n))
                .collect::<Vec<_>>();
            if remaining.is_empty() {
                store.pending.remove(&PendingOwner::Orphan);
            } else {
                store
                    .pending
                    .insert(PendingOwner::Orphan, Stock::from_entries(remaining)?);
            }
            let worker = store
                .pending
                .entry(PendingOwner::Worker(actor.clone()))
                .or_default();
            for (key, n) in adopted.ordered_entries() {
                worker.credit(key.clone(), *n)?;
            }
            Ok(())
        })
    }
    pub fn orphan_pending(&mut self, id: &BuildingId, actor: &ActorId) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            let s = l.store_mut(id)?;
            let pending = s
                .pending
                .remove(&PendingOwner::Worker(actor.clone()))
                .ok_or(Error::MissingPending)?;
            let orphan = s.pending.entry(PendingOwner::Orphan).or_default();
            for (k, n) in pending.ordered_entries() {
                orphan.credit(k.clone(), *n)?;
            }
            Ok(())
        })
    }
    /// Physical pickup only; the trip owner authorizes timing and current rules.
    pub fn pickup(
        &mut self,
        actor: &ActorId,
        source: &BuildingId,
        kind: CargoKind,
        goods: &Stock,
    ) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            items_only(goods)?;
            if l.held.contains_key(actor) {
                return Err(Error::CargoExists);
            }
            l.store(source)?.complete()?;
            for (k, n) in goods.ordered_entries() {
                take_from(l.store_mut(source)?, k, *n, kind == CargoKind::Collect)?;
            }
            l.held.insert(
                actor.clone(),
                HeldCargo {
                    actor: actor.clone(),
                    source: source.clone(),
                    kind,
                    goods: goods.clone(),
                },
            );
            Ok(())
        })
    }
    /// Keep undeliverable cargo held. No flow or milestones on physical arrival.
    pub fn deposit(&mut self, actor: &ActorId, destination: &BuildingId) -> Result<Stock, Error> {
        let order: Vec<_> = self
            .held
            .get(actor)
            .ok_or(Error::MissingCargo)?
            .goods
            .entries()
            .keys()
            .cloned()
            .collect();
        self.deposit_ordered(actor, destination, &order)
    }
    /// Supply cargo retains its pickup order when a destination only partly fits.
    pub fn deposit_ordered(
        &mut self,
        actor: &ActorId,
        destination: &BuildingId,
        order: &[ResourceKey],
    ) -> Result<Stock, Error> {
        self.apply(|l| {
            l.actor(actor)?;
            let mut cargo = l.held.remove(actor).ok_or(Error::MissingCargo)?;
            if cargo.kind == CargoKind::Sell {
                return Err(Error::InvalidStorage);
            }
            let s = l.store_mut(destination)?;
            s.complete()?;
            let mut moved = Stock::default();
            let unique: BTreeSet<_> = order.iter().collect();
            if unique.len() != order.len()
                || cargo.goods.entries().keys().any(|k| !unique.contains(k))
            {
                return Err(Error::InvalidStorage);
            }
            for k in order {
                let n = cargo.goods.quantity(k);
                let room = if cargo.kind == CargoKind::Collect {
                    s.local_room(k.as_str())?
                } else {
                    (s.policy.input_size() - sum(&s.inputs)).max(0.0)
                };
                let take = q(n.get().min(room));
                if take.get() == 0.0 {
                    continue;
                }
                let target = if cargo.kind == CargoKind::Collect {
                    &mut s.stored
                } else {
                    &mut s.inputs
                };
                cargo.goods.transfer_to(target, k, take)?;
                moved.credit(k.clone(), take)?;
            }
            if sum(&cargo.goods) > 0.0 {
                l.held.insert(actor.clone(), cargo);
            }
            Ok(moved)
        })
    }
    /// Cancellation restores source stock despite filters/capacity; caller supplies a
    /// completed hall fallback if the source was removed or became construction.
    pub fn return_cargo(&mut self, actor: &ActorId, fallback: &BuildingId) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            let cargo = l.held.remove(actor).ok_or(Error::MissingCargo)?;
            let destination = if l.store(&cargo.source).is_ok_and(|s| !s.policy.construction) {
                cargo.source
            } else {
                let s = l.store(fallback)?;
                s.complete()?;
                if s.policy.kind != "hall" {
                    return Err(Error::InvalidStorage);
                }
                fallback.clone()
            };
            let s = l.store_mut(&destination)?;
            for (k, n) in cargo.goods.ordered_entries().filter(|(_, n)| n.get() > 0.0) {
                s.stored.credit(k.clone(), *n)?;
            }
            Ok(())
        })
    }
    /// Explicit cancellation endpoint selected by the trip owner, including
    /// relocation-aware hall salvage. Capacity/filters never destroy actual cargo.
    pub fn restore_cargo(
        &mut self,
        actor: &ActorId,
        destination: &BuildingId,
    ) -> Result<(), Error> {
        self.apply(|l| {
            l.actor(actor)?;
            l.store(destination)?.complete()?;
            let cargo = l.held.remove(actor).ok_or(Error::MissingCargo)?;
            for (key, n) in cargo.goods.ordered_entries().filter(|(_, n)| n.get() > 0.0) {
                l.store_mut(destination)?.stored.credit(key.clone(), *n)?;
            }
            Ok(())
        })
    }
    pub fn advance(&mut self, elapsed: Seconds) -> Result<(), Error> {
        self.activity.advance(elapsed)
    }
}
fn q(v: f64) -> Quantity {
    Quantity::new(v).expect("validated ledger quantity")
}
fn finite(v: f64) -> Result<f64, Error> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Error::Overflow)
    }
}
fn sum(stock: &Stock) -> f64 {
    stock.entries().values().map(|v| v.get()).sum()
}
fn items_only(stock: &Stock) -> Result<(), Error> {
    if stock.entries().keys().any(|k| k.as_str() == "coins") {
        Err(Error::InvalidStorage)
    } else {
        Ok(())
    }
}
fn strict_afford(stock: &Stock, costs: &Stock) -> Result<(), Error> {
    if costs
        .entries()
        .iter()
        .any(|(k, n)| stock.quantity(k).get() < n.get())
    {
        Err(Error::InsufficientStock)
    } else {
        Ok(())
    }
}
fn combined<'a>(stocks: impl IntoIterator<Item = &'a Stock>) -> Result<Stock, Error> {
    let mut result = Stock::default();
    for stock in stocks {
        for (k, n) in stock.entries() {
            result.credit(k.clone(), *n)?;
        }
    }
    Ok(result)
}
fn take_from(
    store: &mut Store,
    key: &ResourceKey,
    amount: Quantity,
    orphan: bool,
) -> Result<(), Error> {
    let mut remaining = amount.get();
    for stock in [&mut store.output, &mut store.stored] {
        let take = remaining.min(stock.quantity(key).get());
        if take > 0.0 {
            stock.spend(&Stock::from_entries([(key.clone(), q(take))])?)?;
        }
        remaining -= take;
    }
    if orphan && let Some(stock) = store.pending.get_mut(&PendingOwner::Orphan) {
        let take = remaining.min(stock.quantity(key).get());
        if take > 0.0 {
            stock.spend(&Stock::from_entries([(key.clone(), q(take))])?)?;
        }
        remaining -= take;
    }
    if remaining > 0.0 {
        return Err(Error::InsufficientStock);
    }
    Ok(())
}
fn add_rows(
    rows: &mut BTreeMap<String, ResourceRow>,
    stock: &Stock,
    field: u8,
) -> Result<(), Error> {
    for (k, n) in stock.entries() {
        for (name, amount) in [(k.as_str(), n.get()), ("food", n.get() * k.food_value())] {
            let row = rows.get_mut(name).unwrap();
            let target = match field {
                0 => &mut row.free,
                1 => &mut row.reserved,
                _ => &mut row.pending,
            };
            *target = finite(*target + amount)?;
        }
    }
    Ok(())
}
