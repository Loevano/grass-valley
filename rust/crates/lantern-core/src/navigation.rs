//! Single-owner derived navigation service, independent of simulation cadence.
//!
//! Hosts revalidate requests against the owning Town and current navigation
//! revision on every service call. No actor transform, cargo or task is held here.
//! There is no overflow queue: denied callers retain their own request and retry.
use crate::{
    model::{ModelError, NavigationRevision, RouteOutcome, RouteRequest, Town},
    search::{Dependencies, Outcome, Query, Search},
    units::Position,
};
use alloc::{collections::VecDeque, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Blocked,
    Cargo,
    Ordinary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    Capacity,
    Reserved,
    QueryLimit,
    IdentityLimit,
    ResourceLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Searching,
    Ready,
    Waiting(Waiting),
    Stale,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub jobs: usize,
    pub quantum: usize,
    /// Maximum operations per search attempt, reset after relevant refresh,
    /// not cumulative work across a request generation or a wall-clock timeout. Each
    /// operation inserts at most eight frontier/map records or one path tile.
    pub work_per_job: usize,
    pub query_records: usize,
    pub identity_bytes: usize,
    /// Maximum observation records per job, checked before each search operation
    /// with room reserved for its worst-case expansion and arrival records.
    pub dependency_records: usize,
    pub cache_entries: usize,
    /// Aggregate cache units: query input records + dependencies + waypoints.
    pub cache_records: usize,
    pub cache_waypoints: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            jobs: 128,
            quantum: 128,
            work_per_job: 32768,
            query_records: 65536,
            identity_bytes: 1024,
            dependency_records: 65536,
            cache_entries: 64,
            cache_records: 262144,
            cache_waypoints: 32768,
        }
    }
}
enum State {
    Searching(Search),
    Ready(Outcome, Dependencies),
    Limited(Waiting),
}
struct Job {
    request: RouteRequest,
    priority: Priority,
    state: State,
    work: usize,
}
impl Job {
    fn status(&self) -> Status {
        match self.state {
            State::Searching(_) => Status::Searching,
            State::Ready(..) => Status::Ready,
            State::Limited(reason) => Status::Waiting(reason),
        }
    }
}
/// A completed result retains its slot until taken/cancelled. It cannot escape
/// the model's generation/origin/navigation checks when applied.
pub struct Completion {
    request: RouteRequest,
    outcome: Outcome,
    dependencies: Dependencies,
}
impl Completion {
    pub fn request(&self) -> &RouteRequest {
        &self.request
    }
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }
    /// Validate current graph observations, then use the original model token.
    /// Host must supply current actor-specific query inputs; no global revision
    /// equality is needed after this proof. Stale generation/origin still rejects.
    pub fn apply_current(self, town: &mut Town, query: &Query) -> Result<(), ModelError> {
        if !town.route_generation_is_current(&self.request) || !self.dependencies.valid_for(query) {
            return Err(ModelError::StaleRoute);
        }
        let revision = self.request.navigation();
        self.apply(town, revision)
    }
    pub fn valid_for(&self, query: &Query) -> bool {
        self.dependencies.valid_for(query)
    }
    pub fn apply(self, town: &mut Town, navigation: NavigationRevision) -> Result<(), ModelError> {
        if !town.route_is_current(&self.request, navigation) {
            return Err(ModelError::StaleRoute);
        }
        let result = match self.outcome {
            Outcome::Arrived => RouteOutcome::Arrived,
            Outcome::Unreachable => RouteOutcome::Unreachable,
            Outcome::Completed { path, .. } => RouteOutcome::Ready(
                path.into_iter()
                    .map(|p| Position::new(f64::from(p.x), f64::from(p.y)).expect("finite tile"))
                    .collect(),
            ),
            _ => unreachable!("only terminal outcomes become completions"),
        };
        town.accept_route(self.request.actor(), &self.request, navigation, result)
    }
}
struct Cached {
    outcome: Outcome,
    dependencies: Dependencies,
    units: usize,
    waypoints: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub restarts: usize,
    pub retained: usize,
    pub cache_hits: usize,
    pub cache_evictions: usize,
    pub validation_records: usize,
}
pub struct Service {
    limits: Limits,
    jobs: VecDeque<Job>,
    reservation: Option<RouteRequest>,
    reservation_seen: bool,
    turn: usize,
    cache: VecDeque<Cached>,
    diagnostics: Diagnostics,
}
impl Service {
    pub fn new(limits: Limits) -> Self {
        assert!(limits.jobs > 0 && limits.quantum > 0 && limits.work_per_job > 0);
        Self {
            limits,
            jobs: VecDeque::new(),
            reservation: None,
            reservation_seen: false,
            turn: 0,
            cache: VecDeque::new(),
            diagnostics: Diagnostics::default(),
        }
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }
    pub fn cache_usage(&self) -> (usize, usize, usize) {
        (
            self.cache.len(),
            self.cache.iter().map(|c| c.units).sum(),
            self.cache.iter().map(|c| c.waypoints).sum(),
        )
    }
    /// Bridge for local invalidation: owner checks generation/origin independently
    /// of the world-wide revision; refresh performs the navigation proof.
    pub fn submit_current(
        &mut self,
        town: &Town,
        request: &RouteRequest,
        query: Rc<Query>,
        priority: Priority,
    ) -> Status {
        if !town.route_generation_is_current(request) {
            return Status::Stale;
        }
        if self.status(request).is_some() {
            self.refresh(request, query.clone());
        }
        self.submit(town, request, request.navigation(), query, priority)
    }
    /// Replace immutable graph data while preserving only proven unchanged work.
    /// Validation/preparation/destruction is synchronous maintenance, not charged
    /// to the search allowance. Oversized replacement releases the old job.
    pub fn refresh(&mut self, request: &RouteRequest, query: Rc<Query>) -> Option<Status> {
        let job = self.jobs.iter_mut().find(|j| &j.request == request)?;
        if query.retained_records() > self.limits.query_records {
            job.state = State::Limited(Waiting::QueryLimit);
            return Some(Status::Waiting(Waiting::QueryLimit));
        }
        let valid = match &mut job.state {
            State::Searching(search) => {
                self.diagnostics.validation_records +=
                    search.dependencies().validation_records(&query);
                search.refresh(query.clone())
            }
            State::Ready(_, deps) => {
                self.diagnostics.validation_records += deps.validation_records(&query);
                let valid = deps.valid_for(&query);
                if valid {
                    deps.replace_query(query.clone());
                }
                valid
            }
            State::Limited(_) => return Some(job.status()),
        };
        if valid {
            self.diagnostics.retained += 1;
        } else {
            self.diagnostics.restarts += 1;
            job.state = Self::search_state(query, self.limits);
            job.work = 0;
        }
        Some(job.status())
    }
    fn search_state(query: Rc<Query>, limits: Limits) -> State {
        let search = Search::bounded(query, limits.work_per_job, limits.dependency_records);
        if search.outcome() == &Outcome::Pending {
            State::Searching(search)
        } else {
            let (outcome, deps) = search.into_parts();
            State::Ready(outcome, deps)
        }
    }
    /// Metered operations in this generation's current search attempt (also after
    /// completion/resource exhaustion). Relevant refresh resets this to zero;
    /// advance returns actual work across calls, and diagnostics count restarts.
    pub fn work(&self, request: &RouteRequest) -> Option<usize> {
        self.jobs
            .iter()
            .find(|j| &j.request == request)
            .map(|j| j.work)
    }
    /// Currently retained logical backing units; includes dense unused capacity,
    /// query/index inputs charged per owner, and cache aggregate backing units.
    /// Identity bytes/allocator metadata are separate from these record units.
    pub fn retained_records(&self) -> usize {
        self.cache_usage().1
            + self
                .jobs
                .iter()
                .map(|job| match &job.state {
                    State::Searching(search) => search.retained_records(),
                    State::Ready(outcome, deps) => {
                        deps.query().retained_records()
                            + deps.retained_records()
                            + match outcome {
                                Outcome::Completed { path, .. } => path.capacity(),
                                _ => 0,
                            }
                    }
                    State::Limited(_) => 0,
                })
                .sum::<usize>()
    }
    pub fn resident_jobs(&self) -> usize {
        self.jobs.len()
    }
    pub fn has_reservation(&self) -> bool {
        self.reservation.is_some()
    }
    pub fn status(&self, request: &RouteRequest) -> Option<Status> {
        self.jobs
            .iter()
            .find(|j| &j.request == request)
            .map(Job::status)
    }
    /// Cancel exactly this generation. A late cancellation cannot cancel a
    /// replacement. Drops query, frontier and result allocations immediately.
    pub fn cancel(&mut self, request: &RouteRequest) {
        self.jobs.retain(|j| &j.request != request);
        if self.reservation.as_ref() == Some(request) {
            self.reservation = None;
        }
    }
    /// Idempotent retries do not restart searches or move them ahead in a lane.
    /// A same-actor replacement releases its old work before admission. Priority
    /// may increase on retry (blocked/cargo urgency), but never decrease.
    /// Query preparation and correct actor-specific goal/access policy remain
    /// the host's responsibility, just as for Search directly.
    pub fn submit(
        &mut self,
        town: &Town,
        request: &RouteRequest,
        navigation: NavigationRevision,
        query: Rc<Query>,
        priority: Priority,
    ) -> Status {
        if !town.route_is_current(request, navigation) {
            return Status::Stale;
        }
        if let Some(job) = self.jobs.iter_mut().find(|j| &j.request == request) {
            job.priority = job.priority.min(priority);
            return job.status();
        }
        self.jobs.retain(|j| j.request.actor() != request.actor());
        if self
            .reservation
            .as_ref()
            .is_some_and(|r| r.actor() == request.actor() && r != request)
        {
            self.reservation = None;
        }
        if request.identity_bytes() > self.limits.identity_bytes {
            return Status::Waiting(Waiting::IdentityLimit);
        }
        if self.limits.dependency_records == 0 {
            return Status::Waiting(Waiting::ResourceLimit);
        }
        if query.retained_records() > self.limits.query_records {
            return Status::Waiting(Waiting::QueryLimit);
        }
        if self.jobs.len() == self.limits.jobs {
            if self.reservation.is_none() {
                self.reservation = Some(request.clone());
            }
            if self.reservation.as_ref() == Some(request) {
                self.reservation_seen = true;
            }
            return Status::Waiting(Waiting::Capacity);
        }
        if self.reservation.as_ref().is_some_and(|r| r != request) {
            return Status::Waiting(Waiting::Reserved);
        }
        self.reservation = None;
        let hit = self
            .cache
            .iter()
            .position(|c| c.dependencies.valid_for(&query));
        let state = if let Some(i) = hit {
            let cached = self.cache.remove(i).expect("cache hit");
            let state = State::Ready(cached.outcome.clone(), cached.dependencies.refreshed(query));
            self.cache.push_back(cached);
            self.diagnostics.cache_hits += 1;
            state
        } else {
            Self::search_state(query, self.limits)
        };
        self.jobs.push_back(Job {
            request: request.clone(),
            priority,
            state,
            work: 0,
        });
        self.status(request).expect("inserted request")
    }
    /// Release obsolete/omitted owner requests before admission, without consuming
    /// the retry lease. Returns released resident slots (reservation is separate).
    pub fn cancel_obsolete(&mut self, mut current: impl FnMut(&RouteRequest) -> bool) -> usize {
        let before = self.jobs.len();
        self.jobs.retain(|j| current(&j.request));
        if self.reservation.as_ref().is_some_and(|r| !current(r)) {
            self.reservation = None;
        }
        before - self.jobs.len()
    }
    /// Prune all stale generations, including terminal answers/reservation, then
    /// serve weighted FIFO lanes (4 blocked / 2 cargo / 1 ordinary). Validation
    /// must consult the authoritative owner; closures must not invent a second
    /// actor registry. No wall-clock guarantee is implied by the work allowance.
    /// Hosts retry denied requests between service calls. A reservation expires
    /// on a service call if its owner has not retried since the previous call;
    /// abandoned-but-current requests therefore cannot hold admission forever.
    /// Fair retry callers get one protected next admission; this is not an
    /// unconditional fairness promise for callers that stop retrying.
    pub fn advance(
        &mut self,
        allowance: usize,
        mut current: impl FnMut(&RouteRequest) -> bool,
    ) -> usize {
        self.jobs.retain(|j| current(&j.request));
        if self
            .reservation
            .as_ref()
            .is_some_and(|r| !self.reservation_seen || !current(r))
        {
            self.reservation = None;
        }
        self.reservation_seen = false;
        const LANES: [Priority; 7] = [
            Priority::Blocked,
            Priority::Blocked,
            Priority::Blocked,
            Priority::Blocked,
            Priority::Cargo,
            Priority::Cargo,
            Priority::Ordinary,
        ];
        let mut used = 0;
        while used < allowance {
            let mut selected = None;
            for _ in 0..LANES.len() {
                let lane = LANES[self.turn];
                self.turn = (self.turn + 1) % LANES.len();
                selected = self
                    .jobs
                    .iter()
                    .position(|j| j.priority == lane && matches!(j.state, State::Searching(_)));
                if selected.is_some() {
                    break;
                }
            }
            let Some(index) = selected else {
                break;
            };
            let mut job = self.jobs.remove(index).expect("selected job");
            if let State::Searching(search) = &mut job.state {
                let remaining = self.limits.work_per_job - search.work;
                let mut quantum = self.limits.quantum.min(allowance - used).min(remaining);
                while quantum > 0 && search.outcome() == &Outcome::Pending {
                    if search.dependencies().records().saturating_add(2)
                        > self.limits.dependency_records
                    {
                        break;
                    }
                    used += search.advance(1);
                    quantum -= 1;
                }
                job.work = search.work;
                if search.outcome() != &Outcome::Pending {
                    let State::Searching(search) =
                        core::mem::replace(&mut job.state, State::Limited(Waiting::ResourceLimit))
                    else {
                        unreachable!()
                    };
                    let (outcome, deps) = search.into_parts();
                    job.state = State::Ready(outcome, deps);
                } else if search.work == self.limits.work_per_job
                    || search.dependencies().records().saturating_add(2)
                        > self.limits.dependency_records
                {
                    // Capacity exhaustion is not a proof of unreachability.
                    job.state = State::Limited(Waiting::ResourceLimit);
                }
            }
            self.jobs.push_back(job);
        }
        used
    }
    pub fn take(&mut self, request: &RouteRequest) -> Option<Completion> {
        let index = self
            .jobs
            .iter()
            .position(|j| &j.request == request && matches!(j.state, State::Ready(..)))?;
        let job = self.jobs.remove(index)?;
        let State::Ready(outcome, dependencies) = job.state else {
            unreachable!()
        };
        let waypoints = match &outcome {
            Outcome::Completed { path, .. } => path.len(),
            _ => 0,
        };
        self.cache
            .retain(|cached| !cached.dependencies.valid_for(dependencies.query()));
        let units = dependencies
            .query()
            .retained_records()
            .saturating_add(dependencies.retained_records())
            .saturating_add(waypoints);
        if self.limits.cache_entries > 0
            && units <= self.limits.cache_records
            && waypoints <= self.limits.cache_waypoints
        {
            loop {
                let (entries, records, paths) = self.cache_usage();
                if entries < self.limits.cache_entries
                    && records <= self.limits.cache_records - units
                    && paths <= self.limits.cache_waypoints - waypoints
                {
                    break;
                }
                self.cache.pop_front();
                self.diagnostics.cache_evictions += 1;
            }
            self.cache.push_back(Cached {
                outcome: outcome.clone(),
                dependencies: dependencies.clone(),
                units,
                waypoints,
            });
        }
        Some(Completion {
            request: job.request,
            outcome,
            dependencies,
        })
    }
}
