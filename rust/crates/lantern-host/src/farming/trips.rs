//! Field-specific water and harvest legs. Goods stay in the R13 ledger.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaterSource {
    Surface(Tile),
    Well { id: BuildingId, tile: Tile },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaterBank {
    pub source: WaterSource,
    pub tile: Tile,
}
impl WaterBank {
    fn destination(&self) -> Destination {
        Destination::Position(
            Position::new(f64::from(self.tile.x), f64::from(self.tile.y)).unwrap(),
        )
    }
}
/// Current landscape-owned surface water and well references, supplied each step.
/// No discovery scan, terrain mutation or second water ownership lives here.
#[derive(Clone, Debug, Default)]
pub struct WaterSources {
    surface: Vec<Tile>,
    wells: Vec<BuildingId>,
    landscape_wells: Vec<(BuildingId, Tile)>,
}
impl WaterSources {
    pub fn new(surface: Vec<Tile>, wells: Vec<BuildingId>) -> Self {
        Self {
            surface,
            wells,
            landscape_wells: Vec::new(),
        }
    }
    /// The shared landscape adapter supplies current completed wells, including
    /// other towns' wells. Refresh this snapshot before every authoritative step;
    /// incomplete/deleted wells must not be admitted by that adapter.
    pub fn from_landscape(surface: Vec<Tile>, completed_wells: Vec<(BuildingId, Tile)>) -> Self {
        Self {
            surface,
            wells: Vec::new(),
            landscape_wells: completed_wells,
        }
    }
    fn banks(&self, town: &Town, ledger: &Ledger) -> Vec<WaterBank> {
        let mut banks = BTreeMap::new();
        let mut sources: Vec<_> = self
            .surface
            .iter()
            .copied()
            .map(WaterSource::Surface)
            .collect();
        sources.extend(self.wells.iter().filter_map(|id| {
            let b = town.building(id).ok()?;
            (Fields::complete(town, ledger, id) && ledger.store(id).ok()?.policy.kind == "well")
                .then(|| WaterSource::Well {
                    id: id.clone(),
                    tile: b.location,
                })
        }));
        sources.extend(
            self.landscape_wells
                .iter()
                .map(|(id, tile)| WaterSource::Well {
                    id: id.clone(),
                    tile: *tile,
                }),
        );
        for source in sources {
            let tile = match &source {
                WaterSource::Surface(t) | WaterSource::Well { tile: t, .. } => *t,
            };
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                if let (Some(x), Some(y)) = (tile.x.checked_add(dx), tile.y.checked_add(dy)) {
                    let tile = Tile { x, y };
                    banks.insert(
                        (x, y),
                        WaterBank {
                            source: source.clone(),
                            tile,
                        },
                    );
                }
            }
        }
        banks.into_values().collect()
    }
}
impl Fields {
    pub fn water_ready(&self, actor: &ActorId, workplace: &BuildingId, cycle: u64) -> bool {
        self.rounds
            .get(actor)
            .is_some_and(|(b, c)| b == workplace && *c == cycle)
    }
    pub(super) fn start_return(
        &mut self,
        town: &mut Town,
        actor: &ActorId,
        mut c: Claim,
        navigation: NavigationRevision,
    ) -> Result<(), Error> {
        c.clock = 0.;
        c.water = None;
        town.set_task(actor, task(&c, WorkPhase::Returning))?;
        town.request_route(
            actor,
            Destination::Building(c.workplace.clone()),
            navigation,
        )?;
        self.claims.insert(actor.clone(), c);
        Ok(())
    }
    pub(super) fn return_cargo(
        &mut self,
        town: &mut Town,
        actor: &ActorId,
        source: &BuildingId,
        navigation: NavigationRevision,
    ) -> Result<(), Error> {
        let c = if let Some(c) = self
            .previous
            .get(actor)
            .filter(|c| &c.workplace == source && self.field(&c.field).is_ok())
        {
            c.clone()
        } else {
            let tile = town.building(source)?.location;
            Claim {
                workplace: source.clone(),
                field: source.clone(),
                destination: Destination::Position(
                    Position::new(f64::from(tile.x), f64::from(tile.y)).unwrap(),
                ),
                practice_start: None,
                practice_seconds: 0.,
                clock: 0.,
                water: None,
            }
        };
        self.start_return(town, actor, c, navigation)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_return(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let c = self.claims.get(actor).cloned().ok_or(Error::Invalid)?;
        if pending(ledger, actor)?.as_ref() != Some(&c.workplace) {
            self.cancel(town, actor)?;
            return Ok(Outcome::Waiting(Wait::Cancelled));
        }
        let mut remaining = elapsed.get();
        if phase(town, actor)? == WorkPhase::Returning {
            town.resume_travel(actor, navigation)?;
            let moved = town.advance_travel(actor, elapsed, 4096, |a, b| {
                segment(a, b, Speed::new(1.8).unwrap())
            })?;
            if town.actor(actor)?.state().travel
                != TravelIntent::Arrived(Destination::Building(c.workplace.clone()))
            {
                return Ok(Outcome::Waiting(Wait::Travel));
            }
            remaining = moved.remaining.get();
            town.set_task(actor, task(&c, WorkPhase::Depositing))?;
        }
        let clock = &mut self.claims.get_mut(actor).ok_or(Error::Invalid)?.clock;
        let take = remaining.min((0.65 - *clock).max(0.));
        *clock += take;
        remaining -= take;
        if *clock + 1e-9 < 0.65 {
            return Ok(Outcome::Progress);
        }
        ledger.drop_off(&c.workplace, actor)?;
        self.cancel(town, actor)?;
        Ok(Outcome::Unloaded {
            workplace: c.workplace,
            remaining: secs(remaining),
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_water(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        elapsed: Seconds,
        navigation: NavigationRevision,
        sources: &WaterSources,
        select: &mut impl FnMut(Position, &[WaterBank]) -> Option<WaterBank>,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let mut c = self.claims.get(actor).cloned().ok_or(Error::Invalid)?;
        let mut remaining = elapsed.get();
        let mut current = phase(town, actor)?;
        let banks = sources.banks(town, ledger);
        if (matches!(
            current,
            WorkPhase::FetchingWater | WorkPhase::CollectingWater
        ) && !c.water.as_ref().is_some_and(|bank| banks.contains(bank)))
            || (current == WorkPhase::FetchingWater
                && matches!(
                    town.actor(actor)?.route(),
                    lantern_core::model::RouteState::Unreachable(_)
                ))
        {
            c.water = None;
            c.clock = 0.;
            town.set_task(actor, task(&c, WorkPhase::Working))?;
            self.claims.insert(actor.clone(), c.clone());
            current = WorkPhase::Working;
        }
        if current == WorkPhase::Outbound {
            town.resume_travel(actor, navigation)?;
            let moved = town.advance_travel(actor, secs(remaining), 4096, |a, b| {
                segment(a, b, Speed::new(1.8).unwrap())
            })?;
            if town.actor(actor)?.state().travel != TravelIntent::Arrived(c.destination.clone()) {
                return Ok(Outcome::Waiting(Wait::Travel));
            }
            remaining = moved.remaining.get();
            current = WorkPhase::Working;
            town.set_task(actor, task(&c, current))?;
        }
        if current == WorkPhase::Working {
            let Some(bank) = select(town.actor(actor)?.state().position, &banks) else {
                return Ok(Outcome::Waiting(Wait::Water));
            };
            if !banks.contains(&bank) {
                return Err(Error::Invalid);
            }
            c.water = Some(bank.clone());
            c.clock = 0.;
            town.set_task(actor, task(&c, WorkPhase::FetchingWater))?;
            town.request_route(actor, bank.destination(), navigation)?;
            self.claims.insert(actor.clone(), c);
            return Ok(Outcome::TravelStarted(secs(remaining)));
        }
        if matches!(
            current,
            WorkPhase::FetchingWater | WorkPhase::ReturningWater
        ) {
            let dest = if current == WorkPhase::FetchingWater {
                c.water.as_ref().ok_or(Error::Invalid)?.destination()
            } else {
                c.destination.clone()
            };
            town.resume_travel(actor, navigation)?;
            let moved = town.advance_travel(actor, secs(remaining), 4096, |a, b| {
                segment(a, b, Speed::new(1.8).unwrap())
            })?;
            if town.actor(actor)?.state().travel != TravelIntent::Arrived(dest) {
                return Ok(Outcome::Waiting(Wait::Travel));
            }
            remaining = moved.remaining.get();
            if current == WorkPhase::ReturningWater {
                town.set_task(actor, task(&c, WorkPhase::Working))?;
                return self.advance_inner(
                    town,
                    ledger,
                    actor,
                    worker,
                    secs(remaining),
                    navigation,
                    sources,
                    select,
                    segment,
                );
            }
            current = WorkPhase::CollectingWater;
            town.set_task(actor, task(&c, current))?;
        }
        if current != WorkPhase::CollectingWater {
            return Err(Error::Invalid);
        }
        let clock = &mut self.claims.get_mut(actor).ok_or(Error::Invalid)?.clock;
        let take = remaining.min((0.65 - *clock).max(0.));
        *clock += take;
        remaining -= take;
        if *clock + 1e-9 < 0.65 {
            return Ok(Outcome::Progress);
        }
        let cycle = self.field(&c.field)?.state.cycle;
        self.rounds
            .insert(actor.clone(), (c.workplace.clone(), cycle));
        self.claims.get_mut(actor).ok_or(Error::Invalid)?.clock = 0.;
        town.set_task(actor, task(&c, WorkPhase::ReturningWater))?;
        town.request_route(actor, c.destination.clone(), navigation)?;
        Ok(Outcome::WaterCollected {
            field: c.field,
            remaining: secs(remaining),
        })
    }
}
