//! Single-owner ordered workshop requests. IDs are town-local monotonic u64s;
//! JSON/HTTP command adapters and legacy opaque-ID conversion remain later work.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkshopOrder {
    pub id: u64,
    pub recipe: String,
    pub amount: u16,
    pub remaining: f64,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub(super) struct Workshops {
    next: u64,
    pub queues: BTreeMap<BuildingId, Vec<WorkshopOrder>>,
}
impl Workshops {
    pub fn head(&self, id: &BuildingId) -> Option<&WorkshopOrder> {
        self.queues.get(id).and_then(|q| q.first())
    }
    pub fn check_owner(&self, town: &Town) -> Result<(), Error> {
        if self
            .queues
            .keys()
            .any(|id| id.identity().validate_owner(town.owner()).is_err())
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub fn save(&self) -> SavedWorkshops {
        SavedWorkshops {
            next: self.next,
            queues: self
                .queues
                .iter()
                .map(|(id, q)| (id.identity().serial(), q.clone()))
                .collect(),
        }
    }
    pub fn decode(saved: SavedWorkshops, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let mut result = Self {
            next: saved.next,
            queues: BTreeMap::new(),
        };
        let mut ids = BTreeSet::new();
        for (serial, queue) in saved.queues {
            let id = serial_building(town, serial)?;
            if ledger.store(&id)?.policy.kind != "workshop"
                || queue.len() > 50
                || result.queues.contains_key(&id)
            {
                return Err(Error::Invalid);
            }
            for (i, order) in queue.iter().enumerate() {
                let mut w = Workplace::new("workshop");
                w.request = Some(progression::Request {
                    recipe: order.recipe.clone(),
                    remaining: Quantity::new(order.remaining).map_err(|_| Error::Invalid)?,
                });
                let recipe = w.selected_recipe()?;
                if order.id == 0
                    || order.id > result.next
                    || !ids.insert(order.id)
                    || !(1..=999).contains(&order.amount)
                    || order.remaining <= 1e-9
                    || order.remaining > f64::from(order.amount)
                    || (i > 0 && order.remaining != f64::from(order.amount))
                    || recipe.output.as_str() != order.recipe
                {
                    return Err(Error::Invalid);
                }
            }
            result.queues.insert(id, queue);
        }
        Ok(result)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedWorkshops {
    next: u64,
    queues: Vec<(u64, Vec<WorkshopOrder>)>,
}

impl Processors {
    pub fn workshop_queue(&self, id: &BuildingId) -> &[WorkshopOrder] {
        self.workshops.queues.get(id).map_or(&[], Vec::as_slice)
    }
    /// Derived configuration for R13 evaluation and R14 supply planning. Caller
    /// request/legacy recipe fields can never override the authoritative head.
    pub fn workplace(&self, id: &BuildingId, base: &Workplace) -> Result<Workplace, Error> {
        let mut result = base.clone();
        if base.kind == "workshop" {
            result.recipe = None;
            result.request = self.workshops.head(id).map(|o| progression::Request {
                recipe: o.recipe.clone(),
                remaining: Quantity::new(o.remaining).expect("validated order"),
            });
        }
        Ok(result)
    }
    fn workshop_available(town: &Town, ledger: &Ledger, id: &BuildingId) -> Result<(), Error> {
        if town.building(id)?.construction != ConstructionState::Complete
            || ledger.store(id)?.policy.construction
            || ledger.store(id)?.policy.kind != "workshop"
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn queue_workshop(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        hall: &BuildingId,
        id: &BuildingId,
        recipe: &str,
        amount: u16,
        sandbox: bool,
        assigned: &[Worker],
    ) -> Result<u64, Error> {
        self.check_owner(town)?;
        Self::workshop_available(town, ledger, id)?;
        if !(1..=999).contains(&amount) {
            return Err(Error::Invalid);
        }
        if self.workshop_queue(id).len() >= 50 {
            return Err(Error::QueueFull);
        }
        progression::choose_recipe("workshop", recipe, sandbox, assigned)?;
        let next = self.workshops.next.checked_add(1).ok_or(Error::Exhausted)?;
        let (mut owner, mut model, mut goods, mut transport) =
            (self.clone(), town.clone(), ledger.clone(), trips.clone());
        owner.workshops.next = next;
        let queue = owner.workshops.queues.entry(id.clone()).or_default();
        queue.push(WorkshopOrder {
            id: next,
            recipe: recipe.into(),
            amount,
            remaining: f64::from(amount),
        });
        if queue.len() == 1 {
            owner.workshop_head_changed(&mut model, &mut goods, &mut transport, hall, id)?;
        }
        *self = owner;
        *town = model;
        *ledger = goods;
        *trips = transport;
        Ok(next)
    }
    pub fn cancel_workshop(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        hall: &BuildingId,
        id: &BuildingId,
        request: u64,
    ) -> Result<(), Error> {
        self.check_owner(town)?;
        Self::workshop_available(town, ledger, id)?;
        let position = self
            .workshop_queue(id)
            .iter()
            .position(|o| o.id == request)
            .ok_or(Error::MissingOrder)?;
        let (mut owner, mut model, mut goods, mut transport) =
            (self.clone(), town.clone(), ledger.clone(), trips.clone());
        owner.workshops.queues.get_mut(id).unwrap().remove(position);
        if position == 0 {
            owner.workshop_head_changed(&mut model, &mut goods, &mut transport, hall, id)?;
        }
        *self = owner;
        *town = model;
        *ledger = goods;
        *trips = transport;
        Ok(())
    }
    pub(super) fn workshop_head_changed(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        trips: &mut LogisticsTrips,
        hall: &BuildingId,
        id: &BuildingId,
    ) -> Result<(), Error> {
        let effective = self.workplace(id, &Workplace::new("workshop"))?;
        let recipe = effective
            .request
            .as_ref()
            .map(|_| effective.selected_recipe())
            .transpose()?;
        let obsolete = Stock::from_entries(
            ledger
                .store(id)?
                .inputs
                .ordered_entries()
                .filter(|(key, _)| {
                    recipe
                        .as_ref()
                        .is_none_or(|r| !r.inputs.entries().contains_key(*key))
                })
                .map(|(key, n)| (key.clone(), *n)),
        )?;
        ledger.release(id, &obsolete)?;
        trips.cancel_supplies_to(town, ledger, id, hall)?;
        for job in self
            .jobs
            .values_mut()
            .filter(|j| j.building == id.identity().serial())
        {
            job.elapsed = 0.;
            job.active = false;
            job.output = recipe
                .as_ref()
                .map_or("tools", |r| r.output.as_str())
                .into();
        }
        Ok(())
    }
    /// Workshop updates require the real trip owner, so completion and command
    /// head changes share one atomic input/cargo/progress/finished-goods event.
    #[allow(clippy::too_many_arguments)]
    pub fn advance_with_trips(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        ore: &mut IronDeposits,
        trips: &mut LogisticsTrips,
        hall: &BuildingId,
        actor: &ActorId,
        worker: &mut Worker,
        workplace: &Workplace,
        elapsed: Seconds,
        navigation: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let (mut owner, mut model, mut goods, mut deposits, mut transport, mut person) = (
            self.clone(),
            town.clone(),
            ledger.clone(),
            ore.clone(),
            trips.clone(),
            worker.clone(),
        );
        let result = owner.advance_inner(
            &mut model,
            &mut goods,
            &mut deposits,
            actor,
            &mut person,
            workplace,
            elapsed,
            navigation,
            &mut segment,
            Some((&mut transport, hall)),
        )?;
        *self = owner;
        *town = model;
        *ledger = goods;
        *ore = deposits;
        *trips = transport;
        *worker = person;
        Ok(result)
    }
}
