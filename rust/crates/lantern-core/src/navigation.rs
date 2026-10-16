//! Single-owner derived navigation service, independent of simulation cadence.
//!
//! Hosts revalidate requests against the owning Town and current navigation
//! revision on every service call. No actor transform, cargo or task is held here.
//! There is no overflow queue: denied callers retain their own request and retry.
use crate::{
    model::{ModelError, NavigationRevision, RouteOutcome, RouteRequest, Town},
    search::{Outcome, Query, Search},
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
    /// Maximum lifetime search operations, not a wall-clock timeout. Each
    /// operation inserts at most eight frontier/map records or one path tile.
    pub work_per_job: usize,
    pub query_records: usize,
    pub identity_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            jobs: 128,
            quantum: 128,
            work_per_job: 32768,
            query_records: 65536,
            identity_bytes: 1024,
        }
    }
}
enum State {
    Searching(Search),
    Ready(Outcome),
    Limited,
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
            State::Ready(_) => Status::Ready,
            State::Limited => Status::Waiting(Waiting::ResourceLimit),
        }
    }
}
/// A completed result retains its slot until taken/cancelled. It cannot escape
/// the model's generation/origin/navigation checks when applied.
pub struct Completion {
    request: RouteRequest,
    outcome: Outcome,
}
impl Completion {
    pub fn request(&self) -> &RouteRequest {
        &self.request
    }
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
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
pub struct Service {
    limits: Limits,
    jobs: VecDeque<Job>,
    reservation: Option<RouteRequest>,
    reservation_seen: bool,
    turn: usize,
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
        }
    }
    /// Metered operations consumed by this retained generation (also after
    /// completion/resource exhaustion). Zero means it has not received work.
    pub fn work(&self, request: &RouteRequest) -> Option<usize> {
        self.jobs
            .iter()
            .find(|j| &j.request == request)
            .map(|j| j.work)
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
        let search = Search::new(query);
        let state = if search.outcome() == &Outcome::Pending {
            State::Searching(search)
        } else {
            State::Ready(search.into_outcome())
        };
        self.jobs.push_back(Job {
            request: request.clone(),
            priority,
            state,
            work: 0,
        });
        self.status(request).expect("inserted request")
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
                used += search.advance(self.limits.quantum.min(allowance - used).min(remaining));
                job.work = search.work;
                if search.outcome() != &Outcome::Pending {
                    let State::Searching(search) =
                        core::mem::replace(&mut job.state, State::Limited)
                    else {
                        unreachable!()
                    };
                    job.state = State::Ready(search.into_outcome());
                } else if search.work == self.limits.work_per_job {
                    // Capacity exhaustion is not a proof of unreachability.
                    job.state = State::Limited;
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
            .position(|j| &j.request == request && matches!(j.state, State::Ready(_)))?;
        let job = self.jobs.remove(index)?;
        let State::Ready(outcome) = job.state else {
            unreachable!()
        };
        Some(Completion {
            request: job.request,
            outcome,
        })
    }
}
