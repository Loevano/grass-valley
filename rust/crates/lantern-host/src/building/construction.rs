//! Python construction cadence over persistent actors. Planning is injected so
//! navigation work is budgeted by its owner, never an unbounded search in a tick.
use super::*;
use lantern_core::{
    model::{ConstructionPhase, Task, TravelIntent},
    search::{Access, Query, Target, Terrain},
    spatial::{LandAccess, RoadMode, SpatialIndex},
    units::Seconds,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Dedicated,
    Idle,
    Busy,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Claim {
    Building(BuildingId),
    Road(Tile),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Walk {
    pub claim: Claim,
    pub footprint: Footprint,
    pub role: Role,
    /// Includes the original (possibly fractional) position. Completed waypoints
    /// are retained across offline reload; no search frontier is persisted.
    pub path: Vec<Position>,
    pub step: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Builder {
    pub role: Role,
    pub claim: Option<Claim>,
    pub walk: Option<Walk>,
    pub helper: Option<BuildingId>,
    pub suspended: Option<Suspended>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Suspended {
    pub task: Task,
    pub travel: TravelIntent,
}
pub struct Plan {
    pub actor: ActorId,
    pub claim: Claim,
    pub origin: Position,
    pub start: Tile,
    pub query: Rc<Query>,
}
pub enum PlanResult {
    Pending,
    Unreachable,
    /// R09 waypoints, excluding the query start. Empty means already adjacent.
    Ready(Vec<Tile>),
}
impl BuildingWorld {
    pub fn builders(&self) -> &BTreeMap<ActorId, Builder> {
        &self.builders
    }

    pub fn restore_with_builders(
        model: ModelWorld,
        rules: BTreeMap<TownId, Rules>,
        landscape: Landscape,
        shared: bool,
        builders: BTreeMap<ActorId, Builder>,
    ) -> Result<Self, Error> {
        let mut world = Self::restore(model, rules, landscape, shared)?;
        let mut claims = BTreeSet::new();
        for (id, builder) in &builders {
            let town = world
                .model
                .towns()
                .iter()
                .find(|t| t.owner() == id.identity().owner())
                .ok_or(Error::Ownership)?;
            let actor = town.actor(id).map_err(|_| Error::InvalidFacts)?;
            if builder.role == Role::Busy && builder.claim.is_some() && builder.helper.is_none() {
                return Err(Error::InvalidFacts);
            }
            if let Some(helper) = &builder.helper {
                town.building(helper).map_err(|_| Error::InvalidFacts)?;
                let task = builder
                    .suspended
                    .as_ref()
                    .map_or(&actor.state().task, |s| &s.task);
                if *task != Task::Idle
                    && !matches!(task,Task::Work{workplace,..} if workplace==helper)
                {
                    return Err(Error::InvalidFacts);
                }
            }
            if builder.helper.is_some() && builder.role != Role::Busy {
                return Err(Error::InvalidFacts);
            }
            if let Some(saved) = &builder.suspended {
                if builder.helper.is_none() || builder.claim.is_none() {
                    return Err(Error::InvalidFacts);
                }
                town.validate_task(&saved.task)
                    .map_err(|_| Error::InvalidFacts)?;
                if !matches!(saved.task, Task::Idle | Task::Work { .. }) {
                    return Err(Error::InvalidFacts);
                }
                if let Some(lantern_core::model::Destination::Building(id)) =
                    saved.travel.destination()
                {
                    town.building(id).map_err(|_| Error::InvalidFacts)?;
                }
                if saved.task == Task::Idle && saved.travel.destination().is_some() {
                    return Err(Error::InvalidFacts);
                }
            }
            match &builder.claim {
                None => {
                    if builder.walk.is_some() || construction_task(&actor.state().task) {
                        return Err(Error::InvalidFacts);
                    }
                    if builder.role != Role::Busy && actor.state().task != Task::Idle {
                        return Err(Error::InvalidFacts);
                    }
                }
                Some(claim) => {
                    if builder.role == Role::Busy
                        && (builder.suspended.is_none()
                            || helper_target(town, &world.rules[&town.owner().town], builder)
                                .map(Claim::Building)
                                .as_ref()
                                != Some(claim))
                    {
                        return Err(Error::InvalidFacts);
                    }
                    if !claims.insert((town.owner().clone(), claim.clone())) {
                        return Err(Error::InvalidFacts);
                    }
                    let fp = target(town, &world.rules[&town.owner().town], claim)
                        .ok_or(Error::InvalidFacts)?;
                    if !matches_task(&actor.state().task, claim) {
                        return Err(Error::InvalidFacts);
                    }
                    if actor.state().travel != lantern_core::model::TravelIntent::None {
                        return Err(Error::InvalidFacts);
                    }
                    if let Some(w) = &builder.walk {
                        if &w.claim != claim
                            || w.role != builder.role
                            || w.footprint != fp
                            || w.path.is_empty()
                            || w.step >= w.path.len()
                            || w.path[w.step] != actor.state().position
                        {
                            return Err(Error::InvalidFacts);
                        }
                        // Stale geometry is allowed: the next tick revalidates it.
                        // Malformed/teleporting paths and non-perimeter ends aren't.
                        if !ends_at(fp, *w.path.last().unwrap())
                            || w.path.iter().enumerate().any(|(i, p)| {
                                if i == 0 {
                                    return false;
                                }
                                let prior = w.path[i - 1];
                                let limit = if i == 1 { 1.5 } else { 1.0 };
                                (p.x.get() - prior.x.get()).abs() > limit
                                    || (p.y.get() - prior.y.get()).abs() > limit
                                    || !integral(*p)
                            })
                        {
                            return Err(Error::InvalidFacts);
                        }
                    }
                }
            }
        }
        // Construction tasks cannot be orphaned by omission from the controller.
        for t in world.model.towns() {
            for a in t.actors() {
                if construction_task(&a.state().task) && !builders.contains_key(&a.state().id) {
                    return Err(Error::InvalidFacts);
                }
            }
        }
        world.builders = builders;
        Ok(world)
    }

    pub fn set_builder_role(
        &mut self,
        owner: &Owner,
        id: &ActorId,
        role: Role,
    ) -> Result<(), Error> {
        if owner.world != *self.model.id() || id.identity().owner() != owner {
            return Err(Error::Ownership);
        }
        let index = self
            .model
            .towns
            .iter()
            .position(|t| t.owner() == owner)
            .ok_or(Error::MissingTown)?;
        let mut town = self.model.towns[index].clone();
        // Explicitly switching an upgrade helper's role releases its profession
        // assignment; suspend/resume is only for temporary upgrade assistance.
        let mut previous = self.builders.get(id).cloned();
        if role != Role::Busy && previous.as_ref().is_some_and(|b| b.helper.is_some()) {
            town.set_task(id, Task::Idle).map_err(|_| Error::Model)?;
            let b = previous.as_mut().unwrap();
            b.helper = None;
            b.suspended = None;
            b.claim = None;
            b.walk = None;
        }
        let actor = town.actor(id).map_err(|_| Error::InvalidFacts)?;
        if construction_task(&actor.state().task) && !self.builders.contains_key(id) {
            return Err(Error::Unsupported);
        }
        if actor.state().task != Task::Idle
            && !construction_task(&actor.state().task)
            && role != Role::Busy
        {
            return Err(Error::Unsupported);
        }
        let mut builder = previous.unwrap_or(Builder {
            role,
            claim: None,
            walk: None,
            helper: None,
            suspended: None,
        });
        if builder.role != role {
            builder.walk = None;
            if role == Role::Busy {
                release(&mut town, id, &mut builder)?;
            }
        }
        builder.role = role;
        self.model.towns[index] = town;
        self.builders.insert(id.clone(), builder);
        Ok(())
    }

    /// Claims follow actor list order, with dedicated builders first, then idle
    /// helpers; previous claims lead each actor's site order. Only one helper per
    /// site. Explicit Busy actors and unregistered actors do no construction.
    /// A travelling update never contributes work, even if it reaches the end.
    pub fn advance_construction(
        &mut self,
        owner: &Owner,
        elapsed: Seconds,
        mut plan: impl FnMut(Plan) -> PlanResult,
    ) -> Result<(), Error> {
        if owner.world != *self.model.id() {
            return Err(Error::Ownership);
        }
        let index = self
            .model
            .towns
            .iter()
            .position(|t| t.owner() == owner)
            .ok_or(Error::MissingTown)?;
        let mut town = self.model.towns[index].clone();
        let mut rules = self.rules[&owner.town].clone();
        let mut builders = self.builders.clone();
        reconcile(&mut town, &rules, &mut builders)?;
        if rules.sandbox {
            let sites = town
                .buildings()
                .iter()
                .map(|b| b.id.clone())
                .collect::<Vec<_>>();
            for id in sites {
                upgrades::finish(&mut town, &mut rules, &id)?;
            }
            for road in &mut rules.roads {
                road.remaining = None;
            }
            for a in town
                .actors()
                .iter()
                .map(|a| a.state().id.clone())
                .collect::<Vec<_>>()
            {
                if let Some(b) = builders.get_mut(&a) {
                    release(&mut town, &a, b)?;
                }
            }
        } else {
            let geometry = self.geometry()?;
            let mut order = town
                .actors()
                .iter()
                .filter_map(|a| {
                    builders
                        .get(&a.state().id)
                        .filter(|b| {
                            b.role != Role::Busy || helper_target(&town, &rules, b).is_some()
                        })
                        .map(|b| (a.state().id.clone(), b.role))
                })
                .collect::<Vec<_>>();
            order.sort_by_key(|(_, role)| *role != Role::Dedicated);
            let mut claimed = BTreeSet::new();
            for (id, _) in order {
                let mut builder = builders.remove(&id).unwrap();
                let mut choices = town
                    .buildings()
                    .iter()
                    .map(|b| Claim::Building(b.id.clone()))
                    .chain(rules.roads.iter().map(|r| Claim::Road(r.tile)))
                    .filter(|c| target(&town, &rules, c).is_some())
                    .filter(|c| {
                        builder.role != Role::Busy
                            || helper_target(&town, &rules, &builder)
                                .map(Claim::Building)
                                .as_ref()
                                == Some(c)
                    })
                    .collect::<Vec<_>>();
                choices.sort_by_key(|c| builder.claim.as_ref() != Some(c));
                let mut selected = false;
                for claim in choices {
                    if claimed.contains(&claim) {
                        continue;
                    }
                    let fp = target(&town, &rules, &claim).unwrap();
                    let position = town.actor(&id).map_err(|_| Error::Model)?.state().position;
                    let retained = builder.walk.as_ref().is_some_and(|w| {
                        w.claim == claim
                            && w.footprint == fp
                            && w.role == builder.role
                            && w.path.get(w.step) == Some(&position)
                    });
                    let mut walk = if retained { builder.walk.clone() } else { None };
                    // Check only the next segment when retaining a route. Road
                    // speed changes apply immediately without discarding it.
                    if let Some(w) = &walk {
                        let query =
                            query(geometry.clone(), &self.landscape, rounded(position)?, fp)?;
                        if w.step + 1 < w.path.len()
                            && !query.segment_safe(position, w.path[w.step + 1])
                        {
                            walk = None;
                        }
                    }
                    if walk.is_none() {
                        let mut origin = position;
                        let mut start = rounded(origin)?;
                        if !open(&geometry, &self.landscape, start) {
                            // Python construction_route recovers an invalid origin
                            // by Manhattan distance, then x/y iteration order.
                            let recovery = (0..self.landscape.size as i32)
                                .flat_map(|x| {
                                    (0..self.landscape.size as i32).map(move |y| Tile { x, y })
                                })
                                .filter(|t| open(&geometry, &self.landscape, *t))
                                .min_by_key(|t| distance(*t, start));
                            let Some(recovery) = recovery else {
                                continue;
                            };
                            start = recovery;
                            origin = position_at(start);
                        }
                        let query = query(geometry.clone(), &self.landscape, start, fp)?;
                        match plan(Plan {
                            actor: id.clone(),
                            claim: claim.clone(),
                            origin,
                            start,
                            query: query.clone(),
                        }) {
                            PlanResult::Pending => {
                                suspend(&town, &id, &mut builder)?;
                                assign(&mut town, &id, &claim, ConstructionPhase::Outbound)?;
                                builder.claim = Some(claim.clone());
                                builder.walk = None;
                                claimed.insert(claim);
                                selected = true;
                                break;
                            }
                            PlanResult::Unreachable => continue,
                            PlanResult::Ready(path) => {
                                if !query.route_safe(start, &path) {
                                    return Err(Error::InvalidFacts);
                                }
                                let mut points = vec![origin];
                                if !integral(origin) {
                                    points.push(position_at(start));
                                }
                                points.extend(path.into_iter().map(position_at));
                                if !ends_at(fp, *points.last().unwrap()) {
                                    return Err(Error::InvalidFacts);
                                }
                                walk = Some(Walk {
                                    claim: claim.clone(),
                                    footprint: fp,
                                    role: builder.role,
                                    path: points,
                                    step: 0,
                                });
                                // Recovery becomes authoritative only with a valid route.
                                town.reposition_actor(&id, origin)
                                    .map_err(|_| Error::Model)?;
                            }
                        }
                    }
                    let mut walk = walk.unwrap();
                    let travelling = walk.step + 1 < walk.path.len();
                    suspend(&town, &id, &mut builder)?;
                    assign(
                        &mut town,
                        &id,
                        &claim,
                        if travelling {
                            ConstructionPhase::Outbound
                        } else {
                            ConstructionPhase::Building
                        },
                    )?;
                    let mut remaining = elapsed.get();
                    let query = query(
                        geometry.clone(),
                        &self.landscape,
                        rounded(walk.path[walk.step])?,
                        fp,
                    )?;
                    while walk.step + 1 < walk.path.len() {
                        let from = walk.path[walk.step];
                        let to = walk.path[walk.step + 1];
                        if !query.segment_safe(from, to) {
                            break;
                        }
                        let speed = 1.8
                            * if geometry.roads().contains(&rounded(from)?) {
                                1.5
                            } else {
                                1.0
                            };
                        let duration =
                            (to.x.get() - from.x.get()).hypot(to.y.get() - from.y.get()) / speed;
                        if remaining < duration {
                            break;
                        }
                        remaining -= duration;
                        walk.step += 1;
                        town.construction_waypoint(&id, walk.path[walk.step])
                            .map_err(|_| Error::Model)?;
                    }
                    builder.claim = Some(claim.clone());
                    builder.walk = Some(walk);
                    claimed.insert(claim.clone());
                    selected = true;
                    if !travelling && progress(&mut town, &mut rules, &claim, elapsed)? {
                        release(&mut town, &id, &mut builder)?;
                    }
                    break;
                }
                if !selected {
                    release(&mut town, &id, &mut builder)?;
                }
                builders.insert(id, builder);
            }
        }
        self.model.towns[index] = town;
        self.rules.insert(owner.town.clone(), rules);
        self.builders = builders;
        Ok(())
    }
    fn geometry(&self) -> Result<Rc<SpatialIndex>, Error> {
        let roads = self
            .rules
            .values()
            .flat_map(|r| {
                r.roads.iter().map(|r| spatial::Road {
                    tile: r.tile,
                    construction: if r.remaining.is_some() {
                        Construction::Site
                    } else {
                        Construction::Complete
                    },
                })
            })
            .collect::<Vec<_>>();
        SpatialIndex::derive(
            &self.structures(),
            &self.landscape.resources,
            &self.landscape.locals,
            &roads,
            self.landscape.size,
        )
        .map(Rc::new)
        .map_err(|_| Error::InvalidFacts)
    }
}
pub(super) fn reconcile(
    town: &mut Town,
    rules: &Rules,
    builders: &mut BTreeMap<ActorId, Builder>,
) -> Result<(), Error> {
    let ids = town
        .actors()
        .iter()
        .map(|a| a.state().id.clone())
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(b) = builders.get_mut(&id) {
            if b.helper.as_ref().is_some_and(|h|town.building(h).is_err()) {
                b.helper=None;b.suspended=None;release(town,&id,b)?;
            } else if b.suspended.as_ref().is_some_and(|s|town.validate_task(&s.task).is_err() || matches!(s.travel.destination(),Some(lantern_core::model::Destination::Building(id)) if town.building(id).is_err())) {
                b.suspended=None;release(town,&id,b)?;
            }
        }
        if let Some(b) = builders.get_mut(&id)
            && let Some(claim) = &b.claim
        {
            match target(town, rules, claim) {
                None => release(town, &id, b)?,
                Some(fp) => {
                    if b.walk.as_ref().is_some_and(|w| w.footprint != fp) {
                        b.walk = None;
                        assign(town, &id, claim, ConstructionPhase::Outbound)?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn construction_task(t: &Task) -> bool {
    matches!(t, Task::Construction { .. } | Task::RoadConstruction { .. })
}
fn matches_task(t: &Task, claim: &Claim) -> bool {
    matches!((t,claim),(Task::Construction{site,..},Claim::Building(id)) if site==id)
        || matches!((t,claim),(Task::RoadConstruction{tile,..},Claim::Road(t)) if tile==t)
}
fn assign(
    town: &mut Town,
    id: &ActorId,
    claim: &Claim,
    phase: ConstructionPhase,
) -> Result<(), Error> {
    let task = match claim {
        Claim::Building(site) => Task::Construction {
            site: site.clone(),
            phase,
            elapsed: Seconds::new(0.).unwrap(),
        },
        Claim::Road(tile) => Task::RoadConstruction { tile: *tile, phase },
    };
    if town.actor(id).map_err(|_| Error::Model)?.state().task != task {
        town.set_task(id, task).map_err(|_| Error::Model)?;
    }
    Ok(())
}
fn release(town: &mut Town, id: &ActorId, builder: &mut Builder) -> Result<(), Error> {
    if let Some(saved) = builder.suspended.take() {
        town.restore_task_intent(id, saved.task, saved.travel)
            .map_err(|_| Error::Model)?;
    } else if construction_task(&town.actor(id).map_err(|_| Error::Model)?.state().task) {
        town.set_task(id, Task::Idle).map_err(|_| Error::Model)?;
    }
    builder.claim = None;
    builder.walk = None;
    Ok(())
}
fn target(town: &Town, rules: &Rules, claim: &Claim) -> Option<Footprint> {
    match claim {
        Claim::Building(id) => town
            .building(id)
            .ok()
            .filter(|b| b.construction != ConstructionState::Complete)
            .and_then(|b| footprint(b.location, rules.sites.get(id)?).ok()),
        Claim::Road(tile) => rules
            .roads
            .iter()
            .find(|r| r.tile == *tile && r.remaining.is_some())
            .and_then(|r| Footprint::saved(r.tile, None).ok()),
    }
}
fn progress(
    town: &mut Town,
    rules: &mut Rules,
    claim: &Claim,
    elapsed: Seconds,
) -> Result<bool, Error> {
    match claim {
        Claim::Building(id) => {
            let ConstructionState::InProgress { remaining } =
                town.building(id).map_err(|_| Error::Model)?.construction
            else {
                return Err(Error::InvalidFacts);
            };
            let left = (remaining.get() - elapsed.get()).max(0.);
            if left == 0. {
                upgrades::finish(town, rules, id)?;
                return Ok(true);
            }
            town.set_construction(
                id,
                if left == 0. {
                    ConstructionState::Complete
                } else {
                    ConstructionState::InProgress {
                        remaining: Work::new(left).unwrap(),
                    }
                },
            )
            .map_err(|_| Error::Model)?;
            Ok(left == 0.)
        }
        Claim::Road(tile) => {
            let road = rules
                .roads
                .iter_mut()
                .find(|r| r.tile == *tile)
                .ok_or(Error::InvalidFacts)?;
            let left = (road.remaining.ok_or(Error::InvalidFacts)?.get() - elapsed.get()).max(0.);
            road.remaining = if left == 0. {
                None
            } else {
                Some(Work::new(left).unwrap())
            };
            Ok(left == 0.)
        }
    }
}
fn rounded(p: Position) -> Result<Tile, Error> {
    let (x, y) = (p.x.get().round_ties_even(), p.y.get().round_ties_even());
    if x < f64::from(i32::MIN)
        || x > f64::from(i32::MAX)
        || y < f64::from(i32::MIN)
        || y > f64::from(i32::MAX)
    {
        return Err(Error::InvalidFacts);
    }
    Ok(Tile {
        x: x as i32,
        y: y as i32,
    })
}
fn position_at(t: Tile) -> Position {
    Position::new(f64::from(t.x), f64::from(t.y)).unwrap()
}
fn integral(p: Position) -> bool {
    p.x.get().fract() == 0. && p.y.get().fract() == 0.
}
fn ends_at(fp: Footprint, p: Position) -> bool {
    integral(p) && rounded(p).is_ok_and(|t| fp.perimeter().contains(&t))
}
fn open(index: &SpatialIndex, land: &Landscape, tile: Tile) -> bool {
    inside(tile, land.size)
        && !land.unavailable.contains(&tile)
        && !index.occupied_for(
            tile,
            &spatial::AccessProfile {
                land: LandAccess::Construction,
                exemptions: &BTreeSet::new(),
                only: None,
            },
        )
}
fn query(
    index: Rc<SpatialIndex>,
    land: &Landscape,
    start: Tile,
    fp: Footprint,
) -> Result<Rc<Query>, Error> {
    Query::new(
        index,
        Terrain {
            cells: Some(land.unavailable.iter().map(|t| (*t, 'v')).collect()),
            bounds: None,
        },
        Access {
            land: LandAccess::Construction,
            ..Access::default()
        },
        start,
        Target::Perimeter(fp),
        true,
        RoadMode::Departure,
    )
    .map(Rc::new)
    .map_err(|_| Error::InvalidFacts)
}

fn helper_target(town: &Town, rules: &Rules, b: &Builder) -> Option<BuildingId> {
    let workplace = b.helper.as_ref()?;
    let site = rules.sites.get(workplace)?;
    let parent = site.parent.as_ref().unwrap_or(workplace);
    town.building(parent)
        .ok()
        .filter(|b| {
            b.construction != ConstructionState::Complete && rules.sites[parent].upgrade.is_some()
        })
        .map(|b| b.id.clone())
}
fn suspend(town: &Town, id: &ActorId, b: &mut Builder) -> Result<(), Error> {
    if b.role == Role::Busy && b.helper.is_some() && b.suspended.is_none() {
        let a = town.actor(id).map_err(|_| Error::Model)?.state();
        if !matches!(a.task, Task::Idle | Task::Work { .. }) {
            return Err(Error::Unsupported);
        }
        b.suspended = Some(Suspended {
            task: a.task.clone(),
            travel: a.travel.clone(),
        });
    }
    Ok(())
}
impl BuildingWorld {
    /// Explicit profession assignment supplied by the later gameplay owner.
    /// A linked-acre resident helps its farm, only while that farm is upgrading.
    pub fn set_upgrade_helper(
        &mut self,
        owner: &Owner,
        id: &ActorId,
        workplace: Option<BuildingId>,
    ) -> Result<(), Error> {
        if owner.world != *self.model.id() || id.identity().owner() != owner {
            return Err(Error::Ownership);
        }
        let index = self
            .model
            .towns
            .iter()
            .position(|t| t.owner() == owner)
            .ok_or(Error::MissingTown)?;
        let mut town = self.model.towns[index].clone();
        let mut builder = self.builders.get(id).cloned().unwrap_or(Builder {
            role: Role::Busy,
            claim: None,
            walk: None,
            helper: None,
            suspended: None,
        });
        release(&mut town, id, &mut builder)?;
        let task = &town
            .actor(id)
            .map_err(|_| Error::InvalidFacts)?
            .state()
            .task;
        if let Some(workplace) = &workplace {
            lookup(&town, workplace)?;
            if !matches!(task, Task::Idle)
                && !matches!(task,Task::Work{workplace:id,..} if id==workplace)
            {
                return Err(Error::Unsupported);
            }
        }
        builder.role = Role::Busy;
        builder.helper = workplace;
        self.model.towns[index] = town;
        self.builders.insert(id.clone(), builder);
        Ok(())
    }
}
