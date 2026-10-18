use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButcherLeg {
    Pickup,
    Finding,
    Drag,
    ToShop,
    Processing,
}
impl ButcherLeg {
    pub(super) fn phase(self) -> WorkPhase {
        match self {
            Self::Pickup | Self::ToShop => WorkPhase::Outbound,
            Self::Finding => WorkPhase::FindingAnimal,
            Self::Drag => WorkPhase::Dragging,
            Self::Processing => WorkPhase::Working,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub(super) id: u64,
    pub(super) shop: u64,
    pub(super) barn: u64,
    pub(super) animal: Animal,
    pub(super) species: Species,
    pub(super) remaining: f64,
    #[serde(deserialize_with = "super::save::required")]
    pub(super) position: Option<[f64; 2]>,
    #[serde(deserialize_with = "super::save::required")]
    pub(super) handler: Option<u64>,
    pub(super) leg: ButcherLeg,
    pub(super) processing_started: bool,
    pub(super) shop_origin: [i32; 2],
    pub(super) barn_origin: [i32; 2],
}
impl Order {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.animal.name
    }
    pub fn species(&self) -> Species {
        self.species
    }
    pub fn remaining(&self) -> Seconds {
        sec(self.remaining)
    }
    pub fn leg(&self) -> ButcherLeg {
        self.leg
    }
    pub fn position(&self) -> Option<Position> {
        self.position.and_then(|p| point(p).ok())
    }
    pub(super) fn target(&self, town: &Town) -> Result<Destination, Error> {
        Ok(match self.leg {
            ButcherLeg::Pickup => {
                if let Some(p) = self.position {
                    Destination::Position(point(p)?)
                } else {
                    Destination::Building(bid(town, self.barn)?)
                }
            }
            ButcherLeg::Finding => {
                if let Some(p) = self.position {
                    Destination::Position(point(p)?)
                } else {
                    Destination::Building(bid(town, self.barn)?)
                }
            }
            _ => Destination::Building(bid(town, self.shop)?),
        })
    }
    fn task(&self, town: &Town, clock: f64) -> Result<Task, Error> {
        Ok(Task::Work {
            workplace: bid(town, self.shop)?,
            target: Destination::Building(bid(town, self.barn)?),
            phase: self.leg.phase(),
            elapsed: sec(clock),
        })
    }
    pub(super) fn matches(&self, town: &Town, a: &ActorId) -> bool {
        matches!(town.actor(a).map(|a|&a.state().task),Ok(Task::Work{workplace,target,..}) if workplace.identity().serial()==self.shop && bid(town,self.barn).is_ok_and(|id|*target==Destination::Building(id)))
    }
}
impl Livestock {
    /// Command transaction over the existing recipe/trip owners. Pauses partial
    /// recipe work, returns in-transit cargo and then admits one manual order.
    #[allow(clippy::too_many_arguments)]
    pub fn interrupt_for_butchery(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        processors: &crate::processing::Processors,
        trips: &mut crate::logistics::LogisticsTrips,
        scene: &Scene,
        shop: &BuildingId,
        barn: &BuildingId,
        species: Species,
        roster: &[(ActorId, Worker)],
        hall: &BuildingId,
        nav: NavigationRevision,
    ) -> Result<u64, Error> {
        let (mut owner, mut model, mut goods, mut transport) =
            (self.clone(), town.clone(), ledger.clone(), trips.clone());
        for (a, _) in roster {
            transport
                .cancel(&mut model, &mut goods, a, hall)
                .map_err(|_| Error::Invalid)?;
            processors
                .pause_for_manual_butchery(&mut model, &mut goods, a, shop)
                .map_err(|_| Error::Invalid)?;
        }
        let id =
            owner.butcher_animal(&mut model, &goods, scene, shop, barn, species, roster, nav)?;
        *self = owner;
        *town = model;
        *ledger = goods;
        *trips = transport;
        Ok(id)
    }
    pub fn order(&self, shop: &BuildingId) -> Result<Option<&Order>, Error> {
        shop.identity()
            .validate_owner(&self.owner)
            .map_err(|_| Error::Invalid)?;
        Ok(self
            .orders
            .iter()
            .find(|o| o.shop == shop.identity().serial()))
    }
    /// Removes the first adult now. Only depletion of this species in this barn
    /// creates a free named young replacement. No meat/hides are awarded here.
    #[allow(clippy::too_many_arguments)]
    pub fn butcher_animal(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        scene: &Scene,
        shop: &BuildingId,
        barn: &BuildingId,
        species: Species,
        roster: &[(ActorId, Worker)],
        nav: NavigationRevision,
    ) -> Result<u64, Error> {
        self.validate(town)?;
        if !complete(town, ledger, shop, "butcher")
            || !complete(town, ledger, barn, "barn")
            || self.order(shop)?.is_some()
        {
            return Err(Error::Invalid);
        }
        let animals = self.animals(barn, species)?;
        let adult = animals
            .iter()
            .position(|a| a.age >= species.duration())
            .ok_or(Error::Invalid)?;
        if !roster.iter().any(|(a, w)| {
            town.actor(a).is_ok() && w.job() == "butchery" && w.qualification("butchery").is_ok()
        }) {
            return Err(Error::Invalid);
        }
        let (mut owner, mut model) = (self.clone(), town.clone());
        let herd = owner
            .barn_mut(barn)?
            .herds
            .iter_mut()
            .find(|h| h.species == species)
            .ok_or(Error::Invalid)?;
        let animal = herd.animals.remove(adult);
        if herd.animals.is_empty() {
            let mut reserved = owner.reserved();
            reserved.insert(animal.name.clone());
            let replacement = owner.fresh(&mut reserved)?;
            owner
                .barn_mut(barn)?
                .herds
                .iter_mut()
                .find(|h| h.species == species)
                .unwrap()
                .animals
                .push(replacement);
        }
        let id = owner.next_order;
        owner.next_order = id.checked_add(1).ok_or(Error::Invalid)?;
        owner.orders.push(Order {
            id,
            shop: shop.identity().serial(),
            barn: barn.identity().serial(),
            animal,
            species,
            remaining: 60.,
            position: None,
            handler: None,
            leg: ButcherLeg::Pickup,
            processing_started: false,
            shop_origin: pair(model.building(shop)?.location),
            barn_origin: pair(model.building(barn)?.location),
        });
        owner.reconcile_butchers(&mut model, ledger, scene, shop, roster, nav)?;
        *self = owner;
        *town = model;
        Ok(id)
    }
    fn transition_order(
        town: &mut Town,
        o: &mut Order,
        leg: ButcherLeg,
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        o.leg = leg;
        let Some(serial) = o.handler else {
            return Ok(());
        };
        let a = aid(town, serial)?;
        let clock = if leg == ButcherLeg::Processing {
            60. - o.remaining
        } else {
            0.
        };
        town.set_task(&a, o.task(town, clock)?)?;
        if matches!(
            leg,
            ButcherLeg::Pickup | ButcherLeg::Drag | ButcherLeg::ToShop
        ) {
            town.request_route(&a, o.target(town)?, nav)?;
        }
        Ok(())
    }
    /// One qualified actor handles the order. Replacements collect an already
    /// dragged animal at its saved fractional position; shared work never resets.
    pub fn reconcile_butchers(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        scene: &Scene,
        shop: &BuildingId,
        roster: &[(ActorId, Worker)],
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        self.validate(town)?;
        let Some(i) = self
            .orders
            .iter()
            .position(|o| o.shop == shop.identity().serial())
        else {
            return Ok(());
        };
        let (mut owner, mut model) = (self.clone(), town.clone());
        let mut o = owner.orders[i].clone();
        if !complete(&model, ledger, shop, "butcher") {
            return Err(Error::Invalid);
        }
        let mut ids = BTreeSet::new();
        let mut handler = None;
        for (a, w) in roster {
            model.actor(a)?;
            if !ids.insert(a.identity().serial()) {
                return Err(Error::Invalid);
            }
            if handler.is_none()
                && w.job() == "butchery"
                && w.qualification("butchery").is_ok()
                && !matches!(model.actor(a)?.state().task, Task::Construction { .. })
            {
                handler = Some(a.identity().serial());
            }
        }
        let previous = o.handler;
        let matching =
            previous.is_some_and(|n| aid(&model, n).is_ok_and(|a| o.matches(&model, &a)));
        if let Some(old) = previous
            && Some(old) != handler
            && matching
        {
            model.set_task(&aid(&model, old)?, Task::Idle)?;
        }
        let moved_shop = o.shop_origin != pair(model.building(shop)?.location);
        let barn = bid(&model, o.barn)?;
        let moved_barn = o.barn_origin != pair(model.building(&barn)?.location);
        o.shop_origin = pair(model.building(shop)?.location);
        o.barn_origin = pair(model.building(&barn)?.location);
        o.handler = handler;
        if let Some(serial) = handler {
            let a = aid(&model, serial)?;
            if owner.keepers.iter().any(|k| k.actor == serial)
                || owner
                    .orders
                    .iter()
                    .any(|other| other.id != o.id && other.handler == Some(serial))
                || ledger.held().contains_key(&a)
                || !model.actor(&a)?.state().cargo.goods().is_empty()
            {
                return Err(Error::Busy);
            }
            let same = previous == handler && matching;
            if !same && model.actor(&a)?.state().task != Task::Idle {
                return Err(Error::Busy);
            }
            let inside_shop = scene
                .footprint(&model, shop)?
                .perimeter()
                .contains(&rounded(model.actor(&a)?.state().position));
            if !same
                || moved_shop
                || (moved_barn && o.position.is_none())
                || (o.leg == ButcherLeg::Processing && !inside_shop)
            {
                let leg = if o.processing_started {
                    ButcherLeg::ToShop
                } else if same && o.leg == ButcherLeg::Drag {
                    ButcherLeg::Drag
                } else {
                    ButcherLeg::Pickup
                };
                Self::transition_order(&mut model, &mut o, leg, nav)?;
            }
        }
        owner.orders[i] = o;
        *self = owner;
        *town = model;
        Ok(())
    }
    pub fn butcher_query(
        &self,
        town: &Town,
        scene: &Scene,
        shop: &BuildingId,
        a: &ActorId,
    ) -> Result<Query, Error> {
        self.validate(town)?;
        let o = self.order(shop)?.ok_or(Error::Invalid)?;
        if o.handler != Some(a.identity().serial()) || !o.matches(town, a) {
            return Err(Error::Invalid);
        }
        let target = match o.leg {
            ButcherLeg::Pickup => {
                if let Some(p) = o.position {
                    Target::Exact(rounded(point(p)?))
                } else {
                    Target::Perimeter(scene.footprint(town, &bid(town, o.barn)?)?)
                }
            }
            ButcherLeg::Drag | ButcherLeg::ToShop => {
                Target::Perimeter(scene.footprint(town, shop)?)
            }
            _ => return Err(Error::Invalid),
        };
        scene.query(town.actor(a)?.state().position, target, None)
    }
    /// Decorate an R10 tile result with the saved exact animal position. This
    /// tiny final segment is checked against current obstacles/fences on advance.
    pub fn accept_butcher_route(
        &self,
        town: &mut Town,
        shop: &BuildingId,
        a: &ActorId,
        request: &RouteRequest,
        nav: NavigationRevision,
        result: RouteOutcome,
    ) -> Result<(), Error> {
        self.validate(town)?;
        let o = self.order(shop)?.ok_or(Error::Invalid)?;
        if o.handler != Some(a.identity().serial()) || !o.matches(town, a) {
            return Err(Error::Invalid);
        }
        let result = if o.leg == ButcherLeg::Pickup {
            if let Some(p) = o.position {
                let target = point(p)?;
                match result {
                    RouteOutcome::Arrived if town.actor(a)?.state().position != target => {
                        RouteOutcome::Ready(vec![target])
                    }
                    RouteOutcome::Ready(mut path) => {
                        if path.last() != Some(&target) {
                            path.push(target);
                        }
                        RouteOutcome::Ready(path)
                    }
                    other => other,
                }
            } else {
                result
            }
        } else {
            result
        };
        town.accept_route(a, request, nav, result)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance_butcher(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        scene: &Scene,
        shop: &BuildingId,
        a: &ActorId,
        worker: &mut Worker,
        elapsed: Seconds,
        nav: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        self.validate(town)?;
        let Some(i) = self
            .orders
            .iter()
            .position(|o| o.shop == shop.identity().serial())
        else {
            return Ok(Outcome::Waiting);
        };
        let o = &self.orders[i];
        if o.handler != Some(a.identity().serial()) {
            return Ok(Outcome::Waiting);
        }
        if !o.matches(town, a)
            || worker.job() != "butchery"
            || worker.qualification("butchery").is_err()
        {
            return Ok(Outcome::NeedsReconcile);
        }
        if !complete(town, ledger, shop, "butcher") {
            return Ok(Outcome::Waiting);
        }
        let (mut owner, mut model, mut goods, mut person) =
            (self.clone(), town.clone(), ledger.clone(), worker.clone());
        let mut o = owner.orders[i].clone();
        let mut remaining = elapsed.get();
        if matches!(
            o.leg,
            ButcherLeg::Pickup | ButcherLeg::Drag | ButcherLeg::ToShop
        ) {
            if o.shop_origin != pair(model.building(shop)?.location)
                || (o.position.is_none()
                    && o.barn_origin != pair(model.building(&bid(&model, o.barn)?)?.location))
            {
                return Ok(Outcome::NeedsReconcile);
            }
            model.resume_travel(a, nav)?;
            let query = self.butcher_query(&model, scene, shop, a)?;
            let movement = model.advance_travel(a, sec(remaining), 4096, |x, y| {
                let exact = o.leg == ButcherLeg::Pickup && o.position == Some(xy(y));
                let safe = if exact {
                    scene.segment_safe(x, y)
                } else {
                    query.segment_safe(x, y)
                };
                safe.then(|| {
                    segment(
                        x,
                        y,
                        Speed::new(if o.leg == ButcherLeg::Drag {
                            1.8 * 0.65
                        } else {
                            1.8
                        })
                        .unwrap(),
                    )
                })
                .flatten()
            })?;
            if o.leg == ButcherLeg::Drag {
                o.position = Some(xy(model.actor(a)?.state().position));
            }
            if !matches!(model.actor(a)?.state().travel, TravelIntent::Arrived(_)) {
                owner.orders[i] = o;
                *self = owner;
                *town = model;
                return Ok(Outcome::Waiting);
            }
            remaining = movement.remaining.get();
            let leg = if o.leg == ButcherLeg::Pickup {
                ButcherLeg::Finding
            } else {
                o.processing_started = true;
                ButcherLeg::Processing
            };
            Self::transition_order(&mut model, &mut o, leg, nav)?;
        }
        if o.leg == ButcherLeg::Finding {
            let Task::Work { elapsed: clock, .. } = model.actor(a)?.state().task else {
                return Err(Error::Invalid);
            };
            let take = remaining.min((1.2 - clock.get()).max(0.));
            let clock = clock.get() + take;
            remaining -= take;
            if clock + 1e-9 < 1.2 {
                model.set_task(a, o.task(&model, clock)?)?;
                owner.orders[i] = o;
                *self = owner;
                *town = model;
                return Ok(Outcome::Advanced);
            }
            o.position = Some(xy(model.actor(a)?.state().position));
            Self::transition_order(&mut model, &mut o, ButcherLeg::Drag, nav)?;
            owner.orders[i] = o;
            *self = owner;
            *town = model;
            return Ok(Outcome::RouteRequested(sec(remaining)));
        }
        if o.leg != ButcherLeg::Processing {
            return Err(Error::Invalid);
        }
        if !scene
            .footprint(&model, shop)?
            .perimeter()
            .contains(&rounded(model.actor(a)?.state().position))
        {
            return Ok(Outcome::NeedsReconcile);
        }
        o.remaining = (o.remaining - remaining).max(0.);
        model.set_task(a, o.task(&model, 60. - o.remaining)?)?;
        let mut outputs = vec![(key("meat"), quantity(o.species.meat())?)];
        if o.species.hides() > 0. {
            outputs.push((key("hides"), quantity(o.species.hides())?));
        }
        let outputs = Stock::from_entries(outputs)?;
        let fits = outputs.ordered_entries().all(|(k, n)| {
            goods.store(shop).is_ok_and(|s| {
                s.local_room(k.as_str())
                    .is_ok_and(|room| n.get() <= room + 1e-9)
            }) && goods
                .production_room(k)
                .is_ok_and(|room| n.get() <= room + 1e-9)
        });
        let result = if o.remaining <= 1e-9 && fits {
            goods.produce(shop, None, &Stock::default(), &outputs)?;
            person.practice("butchery", sec(60.))?;
            model.set_task(a, Task::Idle)?;
            owner.orders.remove(i);
            Outcome::Completed
        } else {
            owner.orders[i] = o;
            Outcome::Advanced
        };
        *self = owner;
        *town = model;
        *ledger = goods;
        *worker = person;
        Ok(result)
    }
}
