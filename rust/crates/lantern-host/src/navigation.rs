//! Single-owner navigation entry point, independent of production updates.
//! Deadlines and counters are derived and never serialized.
use lantern_core::{
    model::{ModelError, RouteRequest, Town},
    navigation::{Limits, Priority, Service, Status},
    search::{Outcome, Query},
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
/// Injectable monotonic time; simulation/game time must not implement this clock.
pub trait Clock {
    fn now(&self) -> Duration;
}
pub struct MonotonicClock(Instant);
impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for MonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Cadence {
    /// Idle delay after each completed pump, not a production/balance timestep.
    pub interval: Duration,
    pub allowance: usize,
    /// Bound synchronous admission/refresh/adoption traversal, including retries.
    pub max_intents: usize,
}
impl Default for Cadence {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(1),
            allowance: 32768,
            max_intents: 1024,
        }
    }
}
/// Borrowed current owner intent. Pending identity belongs in the authoritative
/// actor, not an adapter overflow registry. Update actor-specific queries after
/// geometry/access changes before polling. Omitted requests are cancelled.
pub struct Intent<'a> {
    pub town: usize,
    pub request: &'a RouteRequest,
    pub query: Rc<Query>,
    pub priority: Priority,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Ready,
    Arrived,
    Unreachable,
}
pub struct Event<'a> {
    pub request: &'a RouteRequest,
    pub status: Status,
    /// Attempt operations after this pump, before terminal consumption.
    pub work: usize,
    pub adopted: Option<(Terminal, Result<(), ModelError>)>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub pumps: u64,
    pub operations: u64,
    pub completions: u64,
    pub rejected: u64,
    pub cancellations: u64,
    pub overruns: u64,
    pub missed_releases: u64,
    pub max_jobs: usize,
    pub max_observed_retained_units: usize,
    pub max_duration: Duration,
    pub max_lateness: Duration,
    pub busy: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollError {
    TooManyIntents,
    ClockRegressed,
}
/// Bounded derived service state; no thread, gameplay mutations or retry registry.
pub struct NavigationPump {
    service: Service,
    cadence: Cadence,
    next_due: Duration,
    last_time: Duration,
    diagnostics: Diagnostics,
}
impl NavigationPump {
    pub fn new(limits: Limits, cadence: Cadence, now: Duration) -> Self {
        assert!(!cadence.interval.is_zero() && cadence.allowance > 0 && cadence.max_intents > 0);
        Self {
            service: Service::new(limits),
            cadence,
            next_due: now,
            last_time: now,
            diagnostics: Diagnostics::default(),
        }
    }
    pub fn next_due(&self) -> Duration {
        self.next_due
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }
    pub fn service(&self) -> &Service {
        &self.service
    }
    /// Cancellation releases even terminal answers between deadlines.
    pub fn cancel(&mut self, request: &RouteRequest) {
        if self.service.status(request).is_some() {
            self.diagnostics.cancellations += 1;
        }
        self.service.cancel(request);
    }
    /// Retry/refresh current intents, advance at most the allowance, then consume
    /// and adopt ready answers. Query preparation, certificate/cache work,
    /// conversion and destruction are synchronous maintenance, not search work.
    /// Clock duration includes all pump maintenance and the observer; caller must
    /// separately measure query preparation. Operations do not bound wall time.
    ///
    /// Missed releases coalesce into ONE pump, never catch-up bursts. Next release
    /// is finish + interval, yielding to other host work even after an overrun.
    /// Errors before service leave it unchanged. Too many intents is not silently
    /// truncated. Inputs must contain each current request at most once.
    pub fn poll(
        &mut self,
        clock: &impl Clock,
        towns: &mut [Town],
        intents: &[Intent<'_>],
        mut observe: impl FnMut(Event<'_>),
    ) -> Result<bool, PollError> {
        let start = clock.now();
        if start < self.last_time {
            return Err(PollError::ClockRegressed);
        }
        if intents.len() > self.cadence.max_intents {
            return Err(PollError::TooManyIntents);
        }
        self.last_time = start;
        if start < self.next_due {
            return Ok(false);
        }
        let lateness = start - self.next_due;
        self.diagnostics.max_lateness = self.diagnostics.max_lateness.max(lateness);
        self.diagnostics.missed_releases += (lateness.as_nanos() / self.cadence.interval.as_nanos())
            .min(u128::from(u64::MAX)) as u64;
        let before = self.service.resident_jobs();
        self.service.cancel_obsolete(|r| {
            intents.iter().any(|i| {
                i.request == r
                    && towns
                        .get(i.town)
                        .is_some_and(|t| t.route_generation_is_current(r))
            })
        });
        self.diagnostics.cancellations += (before - self.service.resident_jobs()) as u64;
        for i in intents {
            if let Some(town) = towns.get(i.town) {
                self.service
                    .submit_current(town, i.request, i.query.clone(), i.priority);
            }
        }
        self.diagnostics.max_jobs = self.diagnostics.max_jobs.max(self.service.resident_jobs());
        let used = self.service.advance(self.cadence.allowance, |r| {
            intents.iter().any(|i| {
                i.request == r
                    && towns
                        .get(i.town)
                        .is_some_and(|t| t.route_generation_is_current(r))
            })
        });
        self.diagnostics.operations += used as u64;
        self.diagnostics.max_observed_retained_units = self
            .diagnostics
            .max_observed_retained_units
            .max(self.service.retained_records());
        for i in intents {
            let status = self.service.status(i.request).unwrap_or_else(|| {
                if towns
                    .get(i.town)
                    .is_some_and(|t| t.route_generation_is_current(i.request))
                {
                    self.service.submit_current(
                        &towns[i.town],
                        i.request,
                        i.query.clone(),
                        i.priority,
                    )
                } else {
                    Status::Stale
                }
            });
            let work = self.service.work(i.request).unwrap_or(0);
            let adopted = self.service.take(i.request).map(|completion| {
                let kind = match completion.outcome() {
                    Outcome::Completed { .. } => Terminal::Ready,
                    Outcome::Arrived => Terminal::Arrived,
                    Outcome::Unreachable => Terminal::Unreachable,
                    _ => unreachable!("only terminal completions"),
                };
                let result = towns
                    .get_mut(i.town)
                    .ok_or(ModelError::StaleRoute)
                    .and_then(|town| completion.apply_current(town, &i.query));
                self.diagnostics.completions += 1;
                self.diagnostics.rejected += u64::from(result.is_err());
                (kind, result)
            });
            observe(Event {
                request: i.request,
                status,
                work,
                adopted,
            });
        }
        self.diagnostics.max_observed_retained_units = self
            .diagnostics
            .max_observed_retained_units
            .max(self.service.retained_records());
        let finish = clock.now().max(start);
        let duration = finish - start;
        self.last_time = finish;
        self.next_due = finish.saturating_add(self.cadence.interval);
        self.diagnostics.pumps += 1;
        self.diagnostics.overruns += u64::from(duration > self.cadence.interval);
        self.diagnostics.busy += duration;
        self.diagnostics.max_duration = self.diagnostics.max_duration.max(duration);
        Ok(true)
    }
}
