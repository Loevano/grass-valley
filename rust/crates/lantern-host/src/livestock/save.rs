use super::*;
pub(super) fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    barns: Vec<Barn>,
    keepers: Vec<keepers::Keeper>,
    orders: Vec<Order>,
    name_sequence: u64,
    next_animal: u64,
    next_order: u64,
}
impl Livestock {
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        self.validate(town)?;
        serde_json::to_string(&Saved {
            version: 1,
            world: self.owner.world.as_str().into(),
            town: self.owner.town.as_str().into(),
            barns: self.barns.clone(),
            keepers: self.keepers.clone(),
            orders: self.orders.clone(),
            name_sequence: self.name_sequence,
            next_animal: self.next_animal,
            next_order: self.next_order,
        })
        .map_err(|_| Error::Invalid)
    }
    /// Model, ledger, personal practice and this owner must be restored from one
    /// generation. Routes rebuild; no growth, pickup or reward runs during load.
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let s: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if s.version != 1
            || s.world != town.owner().world.as_str()
            || s.town != town.owner().town.as_str()
            || s.next_animal == 0
            || s.next_order == 0
        {
            return Err(Error::Invalid);
        }
        let result = Self {
            owner: town.owner().clone(),
            barns: s.barns,
            keepers: s.keepers,
            orders: s.orders,
            name_sequence: s.name_sequence,
            next_animal: s.next_animal,
            next_order: s.next_order,
        };
        let mut barns = BTreeSet::new();
        let mut animals = BTreeSet::new();
        let mut names = BTreeSet::new();
        for b in &result.barns {
            let id = bid(town, b.id)?;
            town.building(&id)?;
            if !barns.insert(b.id) || ledger.store(&id)?.policy.kind != "barn" {
                return Err(Error::Invalid);
            }
            let mut species = BTreeSet::new();
            for h in &b.herds {
                if !species.insert(h.species) {
                    return Err(Error::Invalid);
                }
                for a in &h.animals {
                    if a.id == 0
                        || a.id >= result.next_animal
                        || !animals.insert(a.id)
                        || a.name.trim().is_empty()
                        || !names.insert(a.name.clone())
                        || !a.age.is_finite()
                        || a.age < 0.
                        || a.age > h.species.duration()
                    {
                        return Err(Error::Invalid);
                    }
                }
            }
        }
        let mut actors = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        for k in &result.keepers {
            let a = aid(town, k.actor)?;
            let id = bid(town, k.barn)?;
            let state = town.actor(&a)?.state();
            if matches!(
                state.task,
                Task::Transport { .. } | Task::Construction { .. } | Task::Idle
            ) {
                if !actors.insert(k.actor)
                    || !barns.contains(&k.barn)
                    || k.cursor >= 8
                    || k.origin != pair(town.building(&id)?.location)
                    || k.destination.is_some_and(|d| {
                        (i64::from(d[0]) - i64::from(k.origin[0])).abs() > 3
                            || (i64::from(d[1]) - i64::from(k.origin[1])).abs() > 3
                    })
                {
                    return Err(Error::Invalid);
                }
                // The trip/construction owner validates its own clocks and cargo.
                continue;
            }
            let Task::Work { phase, elapsed, .. } = state.task else {
                return Err(Error::Invalid);
            };
            let limit = match phase {
                WorkPhase::Outbound | WorkPhase::Returning => 0.,
                WorkPhase::Working => 3.2,
                WorkPhase::Depositing => 0.65,
                _ => return Err(Error::Invalid),
            };
            let expected = match phase {
                WorkPhase::Outbound => Some(k.target(town)?),
                WorkPhase::Returning => Some(Destination::Building(id.clone())),
                _ => None,
            };
            let destination_valid = k.destination.is_none_or(|d| {
                (i64::from(d[0]) - i64::from(k.origin[0])).abs() <= 3
                    && (i64::from(d[1]) - i64::from(k.origin[1])).abs() <= 3
                    && d != k.origin
            });
            if !actors.insert(k.actor)
                || !barns.contains(&k.barn)
                || k.cursor >= 8
                || !destination_valid
                || k.origin != pair(town.building(&id)?.location)
                || !k.matches(town)
                || elapsed.get() > limit
                || (limit > 0. && elapsed.get() >= limit)
                || expected.as_ref() != state.travel.destination()
                || !state.cargo.goods().is_empty()
                || ledger.held().contains_key(&a)
            {
                return Err(Error::Invalid);
            }
            if matches!(phase, WorkPhase::Outbound | WorkPhase::Working)
                && let Some(d) = k.destination
                && !destinations.insert((k.barn, d))
            {
                return Err(Error::Invalid);
            }
        }
        let mut shops = BTreeSet::new();
        let mut orders = BTreeSet::new();
        for o in &result.orders {
            let shop = bid(town, o.shop)?;
            let barn = bid(town, o.barn)?;
            town.building(&shop)?;
            town.building(&barn)?;
            if o.id == 0
                || o.id >= result.next_order
                || !orders.insert(o.id)
                || !shops.insert(o.shop)
                || !barns.contains(&o.barn)
                || ledger.store(&shop)?.policy.kind != "butcher"
                || o.shop_origin != pair(town.building(&shop)?.location)
                || o.barn_origin != pair(town.building(&barn)?.location)
                || o.animal.id == 0
                || o.animal.id >= result.next_animal
                || !animals.insert(o.animal.id)
                || o.animal.age != o.species.duration()
                || o.animal.name.trim().is_empty()
                || !names.insert(o.animal.name.clone())
                || !o.remaining.is_finite()
                || !(0.0..=60.).contains(&o.remaining)
                || (o.remaining < 60. && !o.processing_started)
                || (matches!(
                    o.leg,
                    ButcherLeg::Drag | ButcherLeg::ToShop | ButcherLeg::Processing
                ) && o.position.is_none())
                || (matches!(o.leg, ButcherLeg::ToShop | ButcherLeg::Processing)
                    != o.processing_started)
            {
                return Err(Error::Invalid);
            }
            if let Some(p) = o.position {
                point(p)?;
            }
            if let Some(serial) = o.handler {
                let a = aid(town, serial)?;
                let state = town.actor(&a)?.state();
                let Task::Work { phase, elapsed, .. } = state.task else {
                    return Err(Error::Invalid);
                };
                let limit = match o.leg {
                    ButcherLeg::Finding => 1.2,
                    ButcherLeg::Processing => 60.,
                    _ => 0.,
                };
                let expected = if matches!(
                    o.leg,
                    ButcherLeg::Pickup | ButcherLeg::Drag | ButcherLeg::ToShop
                ) {
                    Some(o.target(town)?)
                } else {
                    None
                };
                if !actors.insert(serial)
                    || !o.matches(town, &a)
                    || phase != o.leg.phase()
                    || elapsed.get() > limit
                    || (o.leg == ButcherLeg::Finding && elapsed.get() >= limit)
                    || (o.leg == ButcherLeg::Processing && elapsed.get() != (60. - o.remaining))
                    || expected.as_ref() != state.travel.destination()
                    || (o.leg == ButcherLeg::Drag && o.position != Some(xy(state.position)))
                    || (o.leg == ButcherLeg::Finding
                        && o.position.is_some_and(|p| p != xy(state.position)))
                    || (o.leg == ButcherLeg::Processing
                        && state.position != pos(rounded(state.position)))
                    || !state.cargo.goods().is_empty()
                    || ledger.held().contains_key(&a)
                {
                    return Err(Error::Invalid);
                }
            }
        }
        Ok(result)
    }
}
