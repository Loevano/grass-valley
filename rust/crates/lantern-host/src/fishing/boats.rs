//! One stable hut owns one voyage. Only its captain consumes movement time;
//! crew catch accounting remains per actor in the single-owner ledger.
use super::*;
use crate::progression;
use std::cmp::Reverse;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoyagePhase {
    Approach,
    Board,
    Sail,
    Fish,
    Return,
    Deposit,
}
impl VoyagePhase {
    fn work(self) -> WorkPhase {
        match self {
            Self::Approach | Self::Board | Self::Sail => WorkPhase::Outbound,
            Self::Fish => WorkPhase::Working,
            Self::Return => WorkPhase::Returning,
            Self::Deposit => WorkPhase::Depositing,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Voyage {
    crew: Vec<u64>,
    origin: [i32; 2],
    source: [i32; 2],
    pier: Vec<[i32; 2]>,
    dock: [i32; 2],
    berth: [i32; 2],
    tier: u8,
    phase: VoyagePhase,
    return_requested: bool,
}
impl Voyage {
    fn actor(&self, town: &Town, n: usize) -> Result<ActorId, Error> {
        actor(town, self.crew[n])
    }
    fn captain(&self, town: &Town) -> Result<ActorId, Error> {
        self.actor(town, 0)
    }
    fn destination(&self) -> Option<Destination> {
        let p = match self.phase {
            VoyagePhase::Approach => self.dock,
            VoyagePhase::Board | VoyagePhase::Return => self.berth,
            VoyagePhase::Sail => self.source,
            _ => return None,
        };
        Some(Destination::Position(pos(tile(p))))
    }
    fn task(&self, id: &BuildingId, clock: f64) -> Task {
        Task::Work {
            workplace: id.clone(),
            target: Destination::Position(pos(tile(self.source))),
            phase: self.phase.work(),
            elapsed: sec(clock),
        }
    }
    fn matches(&self, town: &Town, id: &BuildingId, a: &ActorId) -> bool {
        matches!(town.actor(a).map(|a|&a.state().task),Ok(Task::Work{workplace,target,..}) if workplace==id && *target==Destination::Position(pos(tile(self.source))))
    }
}
fn actor(town: &Town, serial: u64) -> Result<ActorId, Error> {
    Ok(ActorId::from_saved(
        EntityIdentity::from_saved(town.owner().clone(), serial).map_err(|_| Error::Invalid)?,
    ))
}
fn capacity(ledger: &Ledger, id: &BuildingId) -> Result<f64, Error> {
    Ok(progression::fishing_boat_capacity(&ledger.store(id)?.policy.equipment) as f64)
}
fn fish_key(k: &ResourceKey) -> bool {
    crate::resources::public_catalog()["fish"][k.as_str()].is_object()
}
fn load(town: &Town, ledger: &Ledger, id: &BuildingId, v: &Voyage) -> Result<f64, Error> {
    let store = ledger.store(id)?;
    let mut n = 0.;
    for i in 0..v.crew.len() {
        if let Some(stock) = store.pending.get(&PendingOwner::Worker(v.actor(town, i)?)) {
            n += stock
                .ordered_entries()
                .filter(|(k, _)| fish_key(k))
                .map(|(_, n)| n.get())
                .sum::<f64>();
        }
    }
    Ok(n)
}
fn allowed(w: &Waters, g: &Geometry, origin: Tile, tier: u8, level: u8) -> Vec<Tile> {
    let exemptions = w.tiles();
    let reach = u32::from(level) + 5 + 2 * u32::from(tier - 1);
    w.connected(origin, reach, tier, |t| {
        !g.occupied.contains(&t) && g.open(t, &exemptions)
    })
}
fn select(w: &Waters, g: &Geometry, origin: Tile, tier: u8, level: u8) -> Option<Tile> {
    let depths = w.depths();
    let supplies: BTreeMap<_, _> = w.water.iter().map(|w| (w.tile, w.supply.get())).collect();
    allowed(w, g, origin, tier, level)
        .into_iter()
        .min_by_key(|&t| {
            (
                supplies[&t] <= 0.,
                Reverse(depths[&t]),
                Reverse(distance(origin, t)),
            )
        })
}
fn deck(w: &Waters, g: &Geometry, origin: Tile, previous: Option<&Voyage>) -> Vec<[i32; 2]> {
    let tiles = w.tiles();
    if let Some(v) = previous
        && v.origin == pair(origin)
        && !v.pier.is_empty()
        && g.valid_pier(&v.pier, origin, tile(*v.pier.last().unwrap()), &tiles)
    {
        return v.pier.clone();
    }
    let mut candidates: Vec<_> = tiles
        .iter()
        .filter(|&&t| distance(origin, t) <= 3 && !g.occupied.contains(&t))
        .copied()
        .collect();
    candidates.sort_by_key(|&t| (distance(origin, t), t.y, t.x));
    candidates
        .into_iter()
        .map(|t| g.pier(origin, t, &tiles))
        .find(|p| !p.is_empty())
        .unwrap_or_default()
}
fn afloat(
    town: &Town,
    a: &ActorId,
    water: &[Tile],
    dock: Tile,
    berth: Tile,
) -> Result<bool, Error> {
    let p = town.actor(a)?.state().position;
    let t = Tile {
        x: p.x.get().round_ties_even() as i32,
        y: p.y.get().round_ties_even() as i32,
    };
    Ok(water.contains(&t) && (t != dock || t == berth))
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Boats {
    voyages: BTreeMap<BuildingId, Voyage>,
    history: BTreeMap<ActorId, CatchHistory>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum BoatOutcome {
    Waiting,
    Advanced,
    RouteRequested(Seconds),
    Unloaded(Seconds),
    NeedsReconcile,
}
impl Boats {
    fn validate_owner(&self, town: &Town) -> Result<(), Error> {
        for id in self.voyages.keys() {
            id.identity()
                .validate_owner(town.owner())
                .map_err(|_| Error::Invalid)?;
        }
        for a in self.history.keys() {
            a.identity()
                .validate_owner(town.owner())
                .map_err(|_| Error::Invalid)?;
        }
        Ok(())
    }
    pub fn crew(&self, town: &Town, id: &BuildingId) -> Result<Vec<ActorId>, Error> {
        self.validate_owner(town)?;
        self.voyages
            .get(id)
            .map(|v| (0..v.crew.len()).map(|i| v.actor(town, i)).collect())
            .unwrap_or(Ok(vec![]))
    }
    pub fn captain(&self, town: &Town, id: &BuildingId) -> Result<ActorId, Error> {
        self.validate_owner(town)?;
        self.voyages.get(id).ok_or(Error::Invalid)?.captain(town)
    }
    pub fn phase(&self, id: &BuildingId) -> Option<VoyagePhase> {
        self.voyages.get(id).map(|v| v.phase)
    }
    pub fn source(&self, id: &BuildingId) -> Option<Tile> {
        self.voyages.get(id).map(|v| tile(v.source))
    }
    pub fn berth(&self, id: &BuildingId) -> Option<Tile> {
        self.voyages.get(id).map(|v| tile(v.berth))
    }
    pub fn cargo(&self, town: &Town, ledger: &Ledger, id: &BuildingId) -> Result<Stock, Error> {
        self.validate_owner(town)?;
        let mut entries: BTreeMap<ResourceKey, f64> = BTreeMap::new();
        for a in self.crew(town, id)? {
            if let Some(s) = ledger.store(id)?.pending.get(&PendingOwner::Worker(a)) {
                for (k, n) in s.ordered_entries() {
                    if fish_key(k) {
                        *entries.entry(k.clone()).or_default() += n.get();
                    }
                }
            }
        }
        Stock::from_entries(
            entries
                .into_iter()
                .map(|(k, n)| Ok((k, q(n)?)))
                .collect::<Result<Vec<_>, Error>>()?,
        )
        .map_err(Into::into)
    }
    fn sync(town: &mut Town, v: &Voyage) -> Result<(), Error> {
        let captain = v.captain(town)?;
        for i in 1..v.crew.len() {
            town.follow_work_actor(&v.actor(town, i)?, &captain)?;
        }
        Ok(())
    }
    fn transition(
        town: &mut Town,
        id: &BuildingId,
        v: &mut Voyage,
        phase: VoyagePhase,
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        v.phase = phase;
        let a = v.captain(town)?;
        town.set_task(&a, v.task(id, 0.))?;
        if let Some(d) = v.destination() {
            town.request_route(&a, d, nav)?;
        }
        Self::sync(town, v)
    }
    fn transfer_history(&mut self, land: &mut Fishers, a: &ActorId) {
        if let Some(h) = self.history.get(a)
            && land
                .history
                .get(a)
                .is_none_or(|old| h.last_batch > old.last_batch)
        {
            land.history.insert(a.clone(), h.clone());
        }
    }
    /// Ordered, assigned fishing roster is supplied by the resident owner. First
    /// tier members crew this boat; the rest stay available to the land owner.
    /// Handoffs reconcile both owners atomically, preserving output and batch IDs.
    #[allow(clippy::too_many_arguments)]
    pub fn reconcile(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        land: &mut Fishers,
        w: &Waters,
        g: &Geometry,
        id: &BuildingId,
        roster: &[ActorId],
        nav: NavigationRevision,
    ) -> Result<Vec<ActorId>, Error> {
        let (mut owner, mut model, mut goods, mut shore) =
            (self.clone(), town.clone(), ledger.clone(), land.clone());
        let result =
            owner.reconcile_inner(&mut model, &mut goods, &mut shore, w, g, id, roster, nav)?;
        *self = owner;
        *town = model;
        *ledger = goods;
        *land = shore;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn reconcile_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        land: &mut Fishers,
        w: &Waters,
        g: &Geometry,
        id: &BuildingId,
        roster: &[ActorId],
        nav: NavigationRevision,
    ) -> Result<Vec<ActorId>, Error> {
        self.validate_owner(town)?;
        land.validate_owner(town)?;
        let b = town.building(id)?;
        let policy = &ledger.store(id)?.policy;
        if b.construction != ConstructionState::Complete
            || policy.kind != "fishing_hut"
            || policy.construction
        {
            return Err(Error::Invalid);
        }
        let origin = b.location;
        let tier = progression::fishing_boat_level(&policy.equipment);
        let level = policy.level;
        let mut unique = BTreeSet::new();
        for a in roster {
            town.actor(a)?;
            if !unique.insert(a.clone()) {
                return Err(Error::Invalid);
            }
        }
        let crew = roster
            .iter()
            .take(usize::from(tier))
            .cloned()
            .collect::<Vec<_>>();
        let previous = self.voyages.get(id).cloned();
        let old_clock = previous
            .as_ref()
            .and_then(|v| town.actor(&v.captain(town).ok()?).ok())
            .and_then(|a| match a.state().task {
                Task::Work { elapsed, .. } => Some(elapsed.get()),
                _ => None,
            })
            .unwrap_or(0.);
        for a in &crew {
            if self
                .voyages
                .iter()
                .any(|(other, v)| other != id && v.crew.contains(&a.identity().serial()))
            {
                return Err(Error::Busy);
            }
            let valid = town.actor(a)?.state().task == Task::Idle
                || land.jobs.get(a).is_some_and(|j| j.matches(town, a))
                || previous.as_ref().is_some_and(|v| v.matches(town, id, a));
            if !valid
                || ledger.held().contains_key(a)
                || ledger.stores().iter().any(|s| {
                    s.id != *id && s.pending.contains_key(&PendingOwner::Worker(a.clone()))
                })
                || !town.actor(a)?.state().cargo.goods().is_empty()
            {
                return Err(Error::Busy);
            }
        }
        if let Some(v) = &previous {
            for i in 0..v.crew.len() {
                let a = v.actor(town, i)?;
                if !crew.contains(&a) {
                    if pending(ledger, id, &a)? {
                        ledger.orphan_pending(id, &a)?;
                    }
                    if v.matches(town, id, &a) {
                        town.set_task(&a, Task::Idle)?;
                    }
                    self.transfer_history(land, &a);
                }
            }
        }
        if crew.is_empty() {
            self.voyages.remove(id);
            return Ok(crew);
        }
        // Preserve a promoted captain's voyage pose, including offshore fractions.
        if let Some(v) = &previous {
            let old = v.captain(town)?;
            if old != crew[0] && crew.iter().any(|a| v.crew.contains(&a.identity().serial())) {
                let p = town.actor(&old)?.state().position;
                town.reposition_actor(&crew[0], p)?;
            }
        }
        for a in &crew {
            land.cancel(town, ledger, a)?;
            if let Some(h) = land.history.get(a)
                && self
                    .history
                    .get(a)
                    .is_none_or(|old| h.last_batch > old.last_batch)
            {
                self.history.insert(a.clone(), h.clone());
            }
        }
        let carrying = previous
            .as_ref()
            .is_some_and(|v| load(town, ledger, id, v).is_ok_and(|n| n > 0.));
        let locked = previous.as_ref().is_some_and(|v| {
            carrying || matches!(v.phase, VoyagePhase::Return | VoyagePhase::Deposit)
        });
        let source = if locked {
            previous.as_ref().map(|v| tile(v.source))
        } else {
            select(w, g, origin, tier, level)
        };
        let Some(source) = source else {
            self.voyages.remove(id);
            for a in &crew {
                if pending(ledger, id, a)? {
                    ledger.orphan_pending(id, a)?;
                }
                if previous.as_ref().is_some_and(|v| v.matches(town, id, a)) {
                    town.set_task(a, Task::Idle)?;
                }
                self.transfer_history(land, a);
            }
            return Ok(vec![]);
        };
        let pier = deck(w, g, origin, previous.as_ref());
        let dock = pier.last().copied().unwrap_or(pair(origin));
        let water = allowed(w, g, origin, tier, level);
        let berth = water
            .iter()
            .filter(|&&t| distance(t, tile(dock)) == 1 && !pier.contains(&pair(t)))
            .min_by_key(|&&t| (distance(origin, t), t.y, t.x))
            .copied()
            .map(pair)
            .unwrap_or(dock);
        let serials = crew
            .iter()
            .map(|a| a.identity().serial())
            .collect::<Vec<_>>();
        let unchanged = previous.as_ref().is_some_and(|v| {
            v.crew == serials
                && v.origin == pair(origin)
                && v.source == pair(source)
                && v.pier == pier
                && v.berth == berth
                && v.tier == tier
                && v.matches(town, id, &crew[0])
        });
        if unchanged {
            return Ok(crew);
        }
        let mut v = Voyage {
            crew: serials,
            origin: pair(origin),
            source: pair(source),
            pier,
            dock,
            berth,
            tier,
            phase: VoyagePhase::Approach,
            return_requested: previous.as_ref().is_some_and(|v| v.return_requested),
        };
        let preserved = previous.as_ref().filter(|old| {
            old.origin == v.origin
                && old.source == v.source
                && old.pier == v.pier
                && old.berth == v.berth
                && crew
                    .iter()
                    .any(|a| old.crew.contains(&a.identity().serial()))
        });
        let phase = if let Some(old) = preserved {
            old.phase
        } else if carrying
            && previous.as_ref().is_some_and(|old| {
                old.origin != pair(origin)
                    || old.phase == VoyagePhase::Return
                    || old.phase == VoyagePhase::Deposit
            })
        {
            VoyagePhase::Return
        } else if afloat(town, &crew[0], &water, tile(dock), tile(berth))? {
            VoyagePhase::Sail
        } else {
            VoyagePhase::Approach
        };
        Self::transition(town, id, &mut v, phase, nav)?;
        if preserved.is_some() && matches!(phase, VoyagePhase::Fish | VoyagePhase::Deposit) {
            town.set_task(&crew[0], v.task(id, old_clock))?;
            Self::sync(town, &v)?;
        }
        self.voyages.insert(id.clone(), v);
        Ok(crew)
    }
    pub fn request_return(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        id: &BuildingId,
    ) -> Result<(), Error> {
        self.validate_owner(town)?;
        let v = self.voyages.get_mut(id).ok_or(Error::Invalid)?;
        if load(town, ledger, id, v)? > 0. {
            v.return_requested = true;
        }
        Ok(())
    }
    /// Current access query for the captain only. Sea legs are cardinal and
    /// restricted to the connected, depth-allowed component; shore legs use roads.
    pub fn query(
        &self,
        town: &Town,
        ledger: &Ledger,
        w: &Waters,
        g: &Geometry,
        id: &BuildingId,
    ) -> Result<Query, Error> {
        self.validate_owner(town)?;
        let v = self.voyages.get(id).ok_or(Error::Invalid)?;
        let a = v.captain(town)?;
        if !v.matches(town, id, &a)
            || town.building(id)?.location != tile(v.origin)
            || (!v.pier.is_empty()
                && !g.valid_pier(&v.pier, tile(v.origin), tile(v.dock), g.index.water()))
        {
            return Err(Error::Invalid);
        }
        let target = match v.destination() {
            Some(Destination::Position(p)) => Tile {
                x: p.x.get() as i32,
                y: p.y.get() as i32,
            },
            _ => return Err(Error::Invalid),
        };
        let water = allowed(w, g, tile(v.origin), v.tier, ledger.store(id)?.policy.level);
        let sea = match v.phase {
            VoyagePhase::Sail => true,
            VoyagePhase::Return => afloat(town, &a, &water, tile(v.dock), tile(v.berth))?,
            _ => false,
        };
        let mut access = Access::default();
        if sea {
            access.exemptions = water.iter().copied().collect();
            access.only = Some(access.exemptions.clone());
        } else {
            access.exemptions.extend(v.pier.iter().copied().map(tile));
            access.exemptions.insert(tile(v.origin));
            if v.phase != VoyagePhase::Approach {
                access.exemptions.insert(tile(v.berth));
            }
        }
        if v.phase == VoyagePhase::Board {
            access.only = Some(access.exemptions.clone());
        }
        let p = town.actor(&a)?.state().position;
        let start = Tile {
            x: p.x.get().round_ties_even() as i32,
            y: p.y.get().round_ties_even() as i32,
        };
        Query::new(
            g.index.clone(),
            g.terrain.clone(),
            access,
            start,
            Target::Exact(target),
            !sea && v.phase != VoyagePhase::Board,
            RoadMode::Smooth,
        )
        .map_err(|_| Error::Invalid)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        w: &Waters,
        g: &Geometry,
        id: &BuildingId,
        elapsed: Seconds,
        nav: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<BoatOutcome, Error> {
        let (mut owner, mut model, mut goods) = (self.clone(), town.clone(), ledger.clone());
        let result =
            owner.advance_inner(&mut model, &mut goods, w, g, id, elapsed, nav, &mut segment)?;
        *self = owner;
        *town = model;
        *ledger = goods;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn advance_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        w: &Waters,
        g: &Geometry,
        id: &BuildingId,
        elapsed: Seconds,
        nav: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<BoatOutcome, Error> {
        self.validate_owner(town)?;
        let Some(mut v) = self.voyages.get(id).cloned() else {
            return Ok(BoatOutcome::Waiting);
        };
        let a = v.captain(town)?;
        if !town.building(id).is_ok_and(|b| {
            b.location == tile(v.origin) && b.construction == ConstructionState::Complete
        }) || (0..v.crew.len()).any(|i| {
            !v.actor(town, i)
                .is_ok_and(|member| v.matches(town, id, &member))
        }) || progression::fishing_boat_level(&ledger.store(id)?.policy.equipment) != v.tier
        {
            return Ok(BoatOutcome::NeedsReconcile);
        }
        if ledger.store(id)?.policy.construction {
            return Ok(BoatOutcome::Waiting);
        }
        let mut remaining = elapsed.get();
        {
            if v.destination().is_some() {
                town.resume_travel(&a, nav)?;
                self.voyages.insert(id.clone(), v.clone());
                let query = self.query(town, ledger, w, g, id)?;
                let movement = town.advance_travel(&a, sec(remaining), 4096, |x, y| {
                    query
                        .segment_safe(x, y)
                        .then(|| segment(x, y, Speed::new(1.8).unwrap()))
                        .flatten()
                })?;
                Self::sync(town, &v)?;
                if !matches!(town.actor(&a)?.state().travel, TravelIntent::Arrived(_)) {
                    return Ok(BoatOutcome::Waiting);
                }
                remaining = movement.remaining.get();
                let next = match v.phase {
                    VoyagePhase::Approach => VoyagePhase::Board,
                    VoyagePhase::Board => VoyagePhase::Sail,
                    VoyagePhase::Sail => VoyagePhase::Fish,
                    VoyagePhase::Return => VoyagePhase::Deposit,
                    _ => return Err(Error::Invalid),
                };
                Self::transition(town, id, &mut v, next, nav)?;
                self.voyages.insert(id.clone(), v.clone());
                if v.destination().is_some() {
                    return Ok(BoatOutcome::RouteRequested(sec(remaining)));
                }
            }
            let Task::Work { elapsed: clock, .. } = town.actor(&a)?.state().task else {
                return Err(Error::Invalid);
            };
            let duration = if v.phase == VoyagePhase::Deposit {
                0.65
            } else {
                3.2
            };
            let take = remaining.min((duration - clock.get()).max(0.));
            let clock = clock.get() + take;
            remaining -= take;
            if clock + 1e-9 < duration {
                town.set_task(&a, v.task(id, clock))?;
                Self::sync(town, &v)?;
                self.voyages.insert(id.clone(), v);
                return Ok(BoatOutcome::Advanced);
            }
            if v.phase == VoyagePhase::Deposit {
                for i in 0..v.crew.len() {
                    let member = v.actor(town, i)?;
                    if pending(ledger, id, &member)? {
                        ledger.drop_off(id, &member)?;
                    }
                }
                v.return_requested = false;
                Self::transition(town, id, &mut v, VoyagePhase::Sail, nav)?;
                self.voyages.insert(id.clone(), v);
                return Ok(BoatOutcome::Unloaded(sec(remaining)));
            }
            let n = load(town, ledger, id, &v)?;
            if n <= 0. {
                v.return_requested = false;
            }
            let exhausted = !w
                .water
                .iter()
                .any(|w| w.tile == tile(v.source) && w.supply.get() > 0.);
            let output = progression::fishing_catch(
                &ledger.store(id)?.policy.equipment,
                w.depths()
                    .get(&tile(v.source))
                    .copied()
                    .unwrap_or(1)
                    .min(255) as u8,
            )?;
            let blocked = ledger.store(id)?.local_room(output.as_str())? <= 1e-9
                || ledger.production_room(&output)? <= 0.;
            if n > 0.
                && (n + 1e-9 >= capacity(ledger, id)? || v.return_requested || exhausted || blocked)
            {
                Self::transition(town, id, &mut v, VoyagePhase::Return, nav)?;
                self.voyages.insert(id.clone(), v);
                return Ok(BoatOutcome::RouteRequested(sec(remaining)));
            }
            town.set_task(&a, v.task(id, remaining % 3.2))?;
            Self::sync(town, &v)?;
            self.voyages.insert(id.clone(), v);
            Ok(BoatOutcome::Advanced)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn attempt(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        w: &mut Waters,
        id: &BuildingId,
        a: &ActorId,
        worker: &mut Worker,
        batch: u64,
        roll: u8,
    ) -> Result<Catch, Error> {
        self.validate_owner(town)?;
        if batch == 0 || roll >= 10 {
            return Err(Error::Invalid);
        }
        if self.history.get(a).is_some_and(|h| batch <= h.last_batch) {
            return Ok(Catch::Skipped);
        }
        let v = self.voyages.get(id).ok_or(Error::Invalid)?;
        if !v.crew.contains(&a.identity().serial())
            || !v.matches(town, id, a)
            || worker.job() != "fishing"
        {
            return Err(Error::Invalid);
        }
        worker.qualification("fishing")?;
        let (mut owner, mut goods, mut waters, mut person) =
            (self.clone(), ledger.clone(), w.clone(), worker.clone());
        owner.history.entry(a.clone()).or_default().last_batch = batch;
        if v.phase != VoyagePhase::Fish
            || town.building(id)?.location != tile(v.origin)
            || ledger.store(id)?.policy.construction
            || progression::fishing_boat_level(&ledger.store(id)?.policy.equipment) != v.tier
        {
            *self = owner;
            return Ok(Catch::Skipped);
        }
        let policy = &ledger.store(id)?.policy;
        let depth = waters
            .depths()
            .get(&tile(v.source))
            .copied()
            .unwrap_or(1)
            .min(255) as u8;
        let output = progression::fishing_catch(&policy.equipment, depth)?;
        let load = load(town, ledger, id, v)?;
        let room = ledger
            .store(id)?
            .local_room(output.as_str())?
            .min(ledger.production_room(&output)?)
            .min((capacity(ledger, id)? - load).max(0.));
        if room <= 1e-9 {
            owner.voyages.get_mut(id).unwrap().return_requested = load > 0.;
            *self = owner;
            return Ok(Catch::Skipped);
        }
        let tool = match worker.tool.as_deref() {
            Some("fine_fishing_rod") => 0.25,
            Some("steel_fishing_rod") => 0.5,
            Some("masterwork_fishing_rod") => 0.75,
            _ => 0.,
        };
        let nets = ["nets", "wool_nets"]
            .iter()
            .filter(|k| policy.equipment.contains(**k))
            .count() as f64
            * 0.25;
        let amount = room.min(
            (1. + 0.25 * f64::from(policy.level - 1) + 0.25 * f64::from(v.tier - 1) + tool + nets)
                * person.efficiency("fishing")?,
        );
        let consumed = waters.take(tile(v.source), amount);
        let caught = roll != 0;
        if consumed > 0. {
            if caught {
                goods.produce(
                    id,
                    Some(a),
                    &Stock::default(),
                    &Stock::from_entries([(output, q(consumed)?)])?,
                )?;
                goods.adopt_boat_orphans(
                    id,
                    a,
                    q((capacity(ledger, id)? - load - consumed).max(0.))?,
                )?;
            }
            person.practice("fishing", sec(3.))?;
        }
        let h = owner.history.get_mut(a).unwrap();
        h.attempts = h.attempts.checked_add(1).ok_or(Error::Invalid)?;
        h.caught = caught;
        if waters
            .water
            .iter()
            .any(|w| w.tile == tile(v.source) && w.supply.get() <= 1e-9)
        {
            owner.voyages.get_mut(id).unwrap().return_requested =
                load > 0. || (caught && consumed > 0.);
        }
        *self = owner;
        *ledger = goods;
        *w = waters;
        *worker = person;
        Ok(Catch::Attempt {
            caught,
            amount: q(if caught { consumed } else { 0. })?,
            consumed: q(consumed)?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    voyages: Vec<(u64, Voyage)>,
    history: Vec<(u64, CatchHistory)>,
}
impl Boats {
    pub fn cancel(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        land: &mut Fishers,
        id: &BuildingId,
    ) -> Result<(), Error> {
        self.validate_owner(town)?;
        land.validate_owner(town)?;
        let Some(v) = self.voyages.get(id).cloned() else {
            return Ok(());
        };
        let (mut owner, mut model, mut goods, mut shore) =
            (self.clone(), town.clone(), ledger.clone(), land.clone());
        for i in 0..v.crew.len() {
            let a = v.actor(town, i)?;
            if pending(&goods, id, &a)? {
                goods.orphan_pending(id, &a)?;
            }
            if v.matches(&model, id, &a) {
                model.set_task(&a, Task::Idle)?;
            }
            owner.transfer_history(&mut shore, &a);
        }
        owner.voyages.remove(id);
        *self = owner;
        *town = model;
        *ledger = goods;
        *land = shore;
        Ok(())
    }
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        self.validate_owner(town)?;
        serde_json::to_string(&Saved {
            version: 1,
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            voyages: self
                .voyages
                .iter()
                .map(|(id, v)| (id.identity().serial(), v.clone()))
                .collect(),
            history: self
                .history
                .iter()
                .map(|(a, h)| (a.identity().serial(), h.clone()))
                .collect(),
        })
        .map_err(|_| Error::Invalid)
    }
    pub fn decode(json: &str, town: &Town, ledger: &Ledger, land: &Fishers) -> Result<Self, Error> {
        let s: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if s.version != 1
            || s.world != town.owner().world.as_str()
            || s.town != town.owner().town.as_str()
        {
            return Err(Error::Invalid);
        }
        land.validate_owner(town)?;
        let mut owner = Self::default();
        let mut all = BTreeSet::new();
        for (serial, h) in s.history {
            let a = actor(town, serial)?;
            town.actor(&a)?;
            if h.last_batch == 0
                || h.attempts > h.last_batch
                || (h.attempts == 0 && h.caught)
                || owner.history.insert(a, h).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        for (serial, v) in s.voyages {
            let id = BuildingId::from_saved(
                EntityIdentity::from_saved(town.owner().clone(), serial)
                    .map_err(|_| Error::Invalid)?,
            );
            let b = town.building(&id)?;
            let policy = &ledger.store(&id)?.policy;
            if v.tier == 0
                || v.tier > 3
                || v.crew.is_empty()
                || v.crew.len() > usize::from(v.tier)
                || v.tier != progression::fishing_boat_level(&policy.equipment)
                || policy.kind != "fishing_hut"
                || b.construction != ConstructionState::Complete
                || b.location != tile(v.origin)
                || owner.voyages.contains_key(&id)
            {
                return Err(Error::Invalid);
            }
            let mut seen = BTreeSet::new();
            if v.pier.is_empty() {
                if v.dock != v.origin {
                    return Err(Error::Invalid);
                }
            } else if v.pier.len() < 2
                || v.pier.len() > 4
                || v.pier[0] != v.origin
                || v.pier.last() != Some(&v.dock)
                || !v.pier.iter().all(|&p| seen.insert(p))
                || !v
                    .pier
                    .windows(2)
                    .all(|p| distance(tile(p[0]), tile(p[1])) == 1)
            {
                return Err(Error::Invalid);
            }
            if distance(tile(v.dock), tile(v.berth)) > 1
                || (v.berth != v.dock && v.pier.contains(&v.berth))
            {
                return Err(Error::Invalid);
            }
            if load(town, ledger, &id, &v)? <= 0.
                && distance(tile(v.origin), tile(v.source))
                    > i64::from(policy.level) + 5 + 2 * i64::from(v.tier - 1)
            {
                return Err(Error::Invalid);
            }
            let leader = town.actor(&v.captain(town)?)?.state();
            let Task::Work { elapsed, phase, .. } = leader.task else {
                return Err(Error::Invalid);
            };
            let limit = match v.phase {
                VoyagePhase::Fish => 3.2,
                VoyagePhase::Deposit => 0.65,
                _ => 0.,
            };
            if phase != v.phase.work()
                || elapsed.get() > limit
                || (limit > 0. && elapsed.get() >= limit)
                || v.destination().as_ref() != leader.travel.destination()
            {
                return Err(Error::Invalid);
            }
            for i in 0..v.crew.len() {
                let a = v.actor(town, i)?;
                let state = town.actor(&a)?.state();
                if !all.insert(a.clone())
                    || land.jobs.contains_key(&a)
                    || !v.matches(town, &id, &a)
                    || state.task != leader.task
                    || state.travel != leader.travel
                    || state.position != leader.position
                    || state.facing != leader.facing
                    || ledger.held().contains_key(&a)
                    || ledger.stores().iter().any(|s| {
                        s.id != id && s.pending.contains_key(&PendingOwner::Worker(a.clone()))
                    })
                    || !state.cargo.goods().is_empty()
                    || ledger
                        .store(&id)?
                        .pending
                        .get(&PendingOwner::Worker(a))
                        .is_some_and(|p| p.ordered_entries().any(|(k, _)| !fish_key(k)))
                {
                    return Err(Error::Invalid);
                }
            }
            owner.voyages.insert(id, v);
        }
        Ok(owner)
    }
}
