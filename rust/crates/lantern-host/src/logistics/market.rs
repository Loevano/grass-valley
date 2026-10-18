//! Ordered market rules and explicit simulation-update receipts. No wall clock,
//! navigation owner, offline production, or public command/ordinary-save adapter.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaleRule {
    pub resource: String,
    pub enabled: bool,
    pub cap: f64,
    pub amount: u8,
    pub strict: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rules {
    sales: Vec<SaleRule>,
    purchases: Vec<(String, f64)>,
}
fn buyable(key: &str) -> bool {
    key != "food"
        && resources::public_catalog()["storage_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some(key))
}
fn sellable(key: &str) -> bool {
    resources::public_catalog()["sellable_goods"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(key))
}
fn available(town: &Town, ledger: &Ledger, market: &BuildingId) -> bool {
    town.building(market)
        .is_ok_and(|b| b.construction == ConstructionState::Complete)
        && ledger
            .store(market)
            .is_ok_and(|s| s.policy.kind == "market" && !s.policy.construction)
}
fn require_market(town: &Town, ledger: &Ledger) -> Result<(), Error> {
    if ledger
        .stores()
        .iter()
        .any(|s| available(town, ledger, &s.id))
    {
        Ok(())
    } else {
        Err(Error::MarketRule)
    }
}
impl Rules {
    fn validate(&self) -> Result<(), Error> {
        let mut sales = BTreeSet::new();
        let mut purchases = BTreeSet::new();
        for (key, n) in &self.purchases {
            if !buyable(key) || !n.is_finite() || *n < 0. || !purchases.insert(key) {
                return Err(Error::InvalidSave);
            }
        }
        for rule in &self.sales {
            if !sellable(&rule.resource)
                || !rule.cap.is_finite()
                || rule.cap < 0.
                || (!rule.strict && rule.cap < 1.)
                || !(1..=10).contains(&rule.amount)
                || !sales.insert(&rule.resource)
                || (rule.enabled
                    && self
                        .purchases
                        .iter()
                        .any(|(k, n)| *k == rule.resource && *n >= rule.cap))
            {
                return Err(Error::InvalidSave);
            }
        }
        Ok(())
    }
    pub(super) fn sale_cargo(
        &self,
        ledger: &Ledger,
        source: &BuildingId,
        worker: &Worker,
    ) -> Result<Stock, Error> {
        let inventory = ledger.store(source)?.inventory()?;
        let capacity = carry_capacity(worker, &Workplace::new("market"))?;
        for rule in &self.sales {
            if !rule.enabled {
                continue;
            }
            let key = ResourceKey::parse(&rule.resource)?;
            let free = ledger.free(&key)?.get();
            if free + 1e-9 < rule.cap {
                continue;
            }
            let surplus = if rule.strict {
                (free - rule.cap).max(0.)
            } else {
                f64::INFINITY
            };
            let actual = inventory.quantity(&key).get();
            let amount = (f64::from(rule.amount)
                .min(surplus)
                .min(actual)
                .min(capacity)
                + 1e-9)
                .floor()
                .min(actual)
                .min(capacity);
            if amount > 0. {
                return Ok(Stock::from_entries([(key, q(amount))])?);
            }
        }
        Ok(Stock::default())
    }
    pub(super) fn sale_at_arrival(&self, ledger: &Ledger, cargo: &Stock) -> Result<Stock, Error> {
        let mut sold = Stock::default();
        for (key, n) in cargo.ordered_entries() {
            let Some(rule) = self
                .sales
                .iter()
                .find(|r| r.resource == key.as_str() && r.enabled)
            else {
                continue;
            };
            let surplus = if rule.strict {
                (ledger.free(key)?.get() + n.get() - rule.cap).max(0.)
            } else {
                f64::INFINITY
            };
            let amount = (n.get().min(f64::from(rule.amount)).min(surplus) + 1e-9)
                .floor()
                .min(n.get());
            if amount > 0. {
                sold.credit(key.clone(), q(amount))?;
            }
        }
        Ok(sold)
    }
    fn purchase_candidates(&self, ledger: &Ledger) -> Result<Vec<(ResourceKey, Quantity)>, Error> {
        let coins = ledger.free(&ResourceKey::parse("coins")?)?.get();
        let mut candidates = Vec::new();
        for (name, threshold) in &self.purchases {
            let key = ResourceKey::parse(name)?;
            let amount = (10_f64
                .min(*threshold - ledger.free(&key)?.get())
                .min(ledger.storage_room(name)?)
                .min(coins / (2. * key.sale_price()))
                + 1e-9)
                .floor();
            if amount > 0. {
                candidates.push((key, q(amount)));
            }
        }
        Ok(candidates)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Purchase {
    actor: u64,
    endpoint: Endpoint,
    work: f64,
    last: Option<String>,
    update: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MarketState {
    rules: Rules,
    purchases: Vec<Purchase>,
    manual_update: u64,
}
impl MarketState {
    pub(super) fn validate(&self, town: &Town) -> Result<(), Error> {
        self.rules.validate()?;
        let mut actors = BTreeSet::new();
        for state in &self.purchases {
            let actor = ActorId::from_saved(
                EntityIdentity::from_saved(town.owner().clone(), state.actor)
                    .map_err(|_| Error::InvalidSave)?,
            );
            town.actor(&actor)?;
            state.endpoint.id(town)?;
            if !actors.insert(state.actor)
                || !state.work.is_finite()
                || !(0.0..3.0).contains(&state.work)
                || state.last.as_ref().is_some_and(|k| !buyable(k))
                || state.update == 0
            {
                return Err(Error::InvalidSave);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum PurchaseOutcome {
    Idle,
    Waiting,
    Bought {
        resource: ResourceKey,
        amount: Quantity,
    },
    OnTrip,
    Unavailable,
    Replayed,
}
impl InputTrips {
    pub fn sale_rules(&self) -> &[SaleRule] {
        &self.market.rules.sales
    }
    pub fn purchase_rules(&self) -> &[(String, f64)] {
        &self.market.rules.purchases
    }
    /// Current paired thresholds. Disabling a sale retains its prior batch/order.
    pub fn configure_trade(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        key: ResourceKey,
        buy: Option<Quantity>,
        sell: Option<Quantity>,
    ) -> Result<(), Error> {
        require_market(town, ledger)?;
        if !buyable(key.as_str())
            || (sell.is_some() && !sellable(key.as_str()))
            || matches!((buy,sell),(Some(b),Some(s)) if b.get()>=s.get())
        {
            return Err(Error::MarketRule);
        }
        let mut rules = self.market.rules.clone();
        if let Some(buy) = buy {
            if let Some((_, n)) = rules.purchases.iter_mut().find(|(k, _)| k == key.as_str()) {
                *n = buy.get();
            } else {
                rules.purchases.push((key.as_str().into(), buy.get()));
            }
        } else {
            rules.purchases.retain(|(k, _)| k != key.as_str());
        }
        if let Some(sell) = sell {
            if let Some(rule) = rules.sales.iter_mut().find(|r| r.resource == key.as_str()) {
                rule.enabled = true;
                rule.cap = sell.get();
                rule.strict = true;
            } else {
                rules.sales.push(SaleRule {
                    resource: key.as_str().into(),
                    enabled: true,
                    cap: sell.get(),
                    amount: 10,
                    strict: true,
                });
            }
        } else if let Some(rule) = rules.sales.iter_mut().find(|r| r.resource == key.as_str()) {
            rule.enabled = false;
        }
        rules.validate()?;
        self.market.rules = rules;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn configure_market_sale(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        key: ResourceKey,
        cap: Option<Quantity>,
        enabled: bool,
        amount: Option<u8>,
    ) -> Result<(), Error> {
        require_market(town, ledger)?;
        if !sellable(key.as_str()) {
            return Err(Error::MarketRule);
        }
        let mut rules = self.market.rules.clone();
        let previous = rules.sales.iter().find(|r| r.resource == key.as_str());
        let cap = cap
            .map(|n| n.get())
            .or_else(|| (!enabled).then(|| previous.map_or(10., |r| r.cap)))
            .ok_or(Error::MarketRule)?;
        let amount = amount.unwrap_or_else(|| previous.map_or(10, |r| r.amount));
        if cap < 1.
            || !(1..=10).contains(&amount)
            || (enabled
                && rules
                    .purchases
                    .iter()
                    .any(|(k, n)| k == key.as_str() && *n >= cap))
        {
            return Err(Error::MarketRule);
        }
        let rule = SaleRule {
            resource: key.as_str().into(),
            cap,
            amount,
            enabled,
            strict: previous.is_some_and(|r| r.strict),
        };
        if let Some(old) = rules.sales.iter_mut().find(|r| r.resource == key.as_str()) {
            *old = rule;
        } else {
            rules.sales.push(rule);
        }
        self.market.rules = rules;
        Ok(())
    }
    /// Purchase candidates take priority over a new sale trip; callers try the
    /// remaining sources in stable distance order through shared navigation.
    pub fn sale_sources(
        &self,
        town: &Town,
        ledger: &Ledger,
        market: &BuildingId,
        worker: &Worker,
    ) -> Result<Vec<BuildingId>, Error> {
        if worker.job() != "trading" || !available(town, ledger, market) {
            return Err(Error::MarketRule);
        }
        if !self.market.rules.purchase_candidates(ledger)?.is_empty() {
            return Ok(Vec::new());
        }
        let origin = town.building(market)?.location;
        let mut result = Vec::new();
        for s in ledger.stores() {
            if s.policy.kind != "market"
                && !s.policy.construction
                && town
                    .building(&s.id)
                    .is_ok_and(|b| b.construction == ConstructionState::Complete)
                && sum(&self.market.rules.sale_cargo(ledger, &s.id, worker)?) > 0.
            {
                result.push(s.id.clone());
            }
        }
        result.sort_by_key(|id| {
            let p = town.building(id).unwrap().location;
            (i64::from(p.x) - i64::from(origin.x)).abs()
                + (i64::from(p.y) - i64::from(origin.y)).abs()
        });
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn start_sale(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        worker: &Worker,
        source: &BuildingId,
        market: &BuildingId,
        navigation: NavigationRevision,
    ) -> Result<u64, Error> {
        if !self
            .sale_sources(town, ledger, market, worker)?
            .contains(source)
        {
            return Err(Error::MarketRule);
        }
        let id = self.start(town, ledger, actor, source, market, navigation)?;
        self.trips.get_mut(actor).unwrap().operation = HaulOperation::Sell;
        if let Some(p) = self
            .market
            .purchases
            .iter_mut()
            .find(|p| p.actor == actor.identity().serial())
        {
            p.work = 0.;
        }
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance_merchant(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        hall: &BuildingId,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        if worker.job() != "trading" {
            return Err(Error::MarketRule);
        }
        let rules = self.market.rules.clone();
        let producer = Producer::recipe(Workplace::new("market"));
        let person = worker.clone();
        self.advance_rules(
            town,
            ledger,
            actor,
            worker,
            &producer,
            &person,
            Some(&rules),
            hall,
            elapsed,
            navigation,
            segment,
        )
    }
    /// update is a persisted, monotonically increasing host simulation event ID,
    /// scoped per merchant. Replayed/older events never accumulate time or buy.
    #[allow(clippy::too_many_arguments)]
    pub fn purchase_batch(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        market: &BuildingId,
        update: u64,
        elapsed: Seconds,
    ) -> Result<PurchaseOutcome, Error> {
        if update == 0 {
            return Err(Error::InvalidUpdate);
        }
        town.actor(actor)?;
        if worker.job() != "trading" {
            return Err(Error::MarketRule);
        }
        let serial = actor.identity().serial();
        if self
            .market
            .purchases
            .iter()
            .any(|p| p.actor == serial && p.update >= update)
        {
            return Ok(PurchaseOutcome::Replayed);
        }
        let (mut state, mut goods, mut person) =
            (self.market.clone(), ledger.clone(), worker.clone());
        let index = state.purchases.iter().position(|p| p.actor == serial);
        if !available(town, &goods, market) {
            if let Some(index) = index {
                state.purchases[index].work = 0.;
                state.purchases[index].update = update;
            }
            self.market = state;
            return Ok(PurchaseOutcome::Unavailable);
        }
        let endpoint = Endpoint::capture(town, market)?;
        let index = if let Some(index) = index {
            index
        } else {
            state.purchases.push(Purchase {
                actor: serial,
                endpoint: endpoint.clone(),
                work: 0.,
                last: None,
                update: 0,
            });
            state.purchases.len() - 1
        };
        let purchase = &mut state.purchases[index];
        if purchase.endpoint != endpoint {
            purchase.work = 0.;
            purchase.endpoint = endpoint;
        }
        purchase.update = update;
        let outcome = if self.trips.contains_key(actor) {
            PurchaseOutcome::OnTrip
        } else if town.actor(actor)?.state().task != Task::Idle {
            purchase.work = 0.;
            PurchaseOutcome::Unavailable
        } else {
            let candidates = state.rules.purchase_candidates(&goods)?;
            if candidates.is_empty() {
                purchase.work = 0.;
                PurchaseOutcome::Idle
            } else if purchase.work + elapsed.get() < 3. {
                purchase.work += elapsed.get();
                PurchaseOutcome::Waiting
            } else {
                let index = purchase
                    .last
                    .as_ref()
                    .and_then(|last| candidates.iter().position(|(k, _)| k.as_str() == last))
                    .map_or(0, |i| (i + 1) % candidates.len());
                let (key, n) = &candidates[index];
                let received = goods.purchase(key, *n)?;
                purchase.work = 0.;
                purchase.last = Some(key.as_str().into());
                person.practice("trading", Seconds::new(3.).unwrap())?;
                PurchaseOutcome::Bought {
                    resource: key.clone(),
                    amount: received,
                }
            }
        };
        self.market = state;
        *ledger = goods;
        *worker = person;
        Ok(outcome)
    }
    /// Quick-sale command receipt is town-scoped; the host preserves its sequence
    /// across reload. Retrying an applied command cannot sell a second batch.
    pub fn quick_sale(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        update: u64,
        key: ResourceKey,
        amount: u16,
    ) -> Result<bool, Error> {
        if update == 0 {
            return Err(Error::InvalidUpdate);
        }
        if update <= self.market.manual_update {
            return Ok(false);
        }
        require_market(town, ledger)?;
        if !sellable(key.as_str()) || !(1..=200).contains(&amount) {
            return Err(Error::MarketRule);
        }
        ledger.sell_free(&key, q(f64::from(amount)))?;
        self.market.manual_update = update;
        Ok(true)
    }
}
