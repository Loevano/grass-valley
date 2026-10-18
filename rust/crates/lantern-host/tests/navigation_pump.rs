use lantern_core::{
    model::*,
    navigation::{Limits, Priority},
};
use lantern_host::navigation::*;
use std::{cell::Cell, time::Duration};
#[path = "../examples/support/navigation.rs"]
mod fixture;
use fixture::*;
#[derive(Default)]
struct Fake(Cell<Duration>);
impl Clock for Fake {
    fn now(&self) -> Duration {
        self.0.get()
    }
}
fn requests(towns: &mut [Town]) -> Vec<(usize, RouteRequest)> {
    let mut result = vec![];
    for (i, t) in towns.iter_mut().enumerate() {
        let ids: Vec<_> = t.actors().iter().map(|a| a.state().id.clone()).collect();
        for id in ids {
            result.push((
                i,
                t.request_route(
                    &id,
                    Destination::Position(position(63., 0.)),
                    NavigationRevision(1),
                )
                .unwrap(),
            ));
        }
    }
    result
}
fn intents<'a>(
    r: &'a [(usize, RouteRequest)],
    q: std::rc::Rc<lantern_core::search::Query>,
) -> Vec<Intent<'a>> {
    r.iter()
        .map(|(town, request)| Intent {
            town: *town,
            request,
            query: q.clone(),
            priority: Priority::Ordinary,
        })
        .collect()
}
fn pump(allowance: usize, jobs: usize) -> NavigationPump {
    NavigationPump::new(
        Limits {
            jobs,
            cache_entries: 0,
            ..Limits::default()
        },
        Cadence {
            allowance,
            ..Cadence::default()
        },
        Duration::ZERO,
    )
}
#[test]
fn independent_clock_allowance_and_missed_releases_without_catchup() {
    let (mut towns, _) = towns(4);
    let r = requests(&mut towns);
    let i = intents(&r, query("wall", false));
    let clock = Fake::default();
    let mut p = pump(7, 128);
    let state = towns[0].actors()[0].state().clone();
    assert!(p.poll(&clock, &mut towns, &i, |_| {}).unwrap());
    assert_eq!(p.diagnostics().operations, 7);
    assert!(
        !p.poll(&clock, &mut towns, &i, |_| panic!("not due"))
            .unwrap()
    );
    clock.0.set(Duration::from_millis(11));
    p.poll(&clock, &mut towns, &i, |_| {
        clock.0.set(Duration::from_millis(16))
    })
    .unwrap();
    assert_eq!(p.diagnostics().pumps, 2);
    assert_eq!(p.diagnostics().operations, 14);
    assert_eq!(p.diagnostics().missed_releases, 10);
    assert_eq!(p.diagnostics().overruns, 1);
    assert_eq!(p.next_due(), Duration::from_millis(17));
    assert_eq!(*towns[0].actors()[0].state(), state);
    clock.0.set(Duration::from_millis(15));
    assert_eq!(
        p.poll(&clock, &mut towns, &i, |_| {}),
        Err(PollError::ClockRegressed)
    );
}
#[test]
fn saturation_retries_terminal_draining_and_finite_completion() {
    let (mut towns, _) = towns(16);
    let r = requests(&mut towns);
    let i = intents(&r, query("wall", false));
    let clock = Fake::default();
    let mut p = pump(32768, 2);
    let mut done = 0;
    for _ in 0..100 {
        clock.0.set(p.next_due());
        p.poll(&clock, &mut towns, &i, |e| {
            if let Some((Terminal::Ready, result)) = e.adopted {
                result.unwrap();
                done += 1;
            }
        })
        .unwrap();
        if done == 16 {
            break;
        }
    }
    assert_eq!(done, 16);
    assert_eq!(p.diagnostics().max_jobs, 2);
    assert_eq!(p.service().resident_jobs(), 0);
    assert!(!p.service().has_reservation());
    assert_eq!(p.diagnostics().rejected, 0);
    for t in &towns {
        for a in t.actors() {
            assert_eq!(a.state().position, position(0., 0.));
        }
    }
}
#[test]
fn negative_consumption_then_new_generation_open_recovery() {
    let (mut towns, _) = towns(4);
    let mut r = requests(&mut towns);
    let clock = Fake::default();
    let mut p = NavigationPump::new(Limits::default(), Cadence::default(), Duration::ZERO);
    let mut negatives = 0;
    for _ in 0..10 {
        clock.0.set(p.next_due());
        p.poll(
            &clock,
            &mut towns,
            &intents(&r, query("negative", false)),
            |e| {
                if let Some((Terminal::Unreachable, result)) = e.adopted {
                    result.unwrap();
                    negatives += 1;
                }
            },
        )
        .unwrap();
    }
    assert_eq!(negatives, 4);
    assert_eq!(p.service().resident_jobs(), 0);
    r = requests(&mut towns);
    clock.0.set(p.next_due());
    p.poll(
        &clock,
        &mut towns,
        &intents(&r, query("negative", false)),
        |e| {
            if let Some((Terminal::Unreachable, result)) = e.adopted {
                result.unwrap();
                negatives += 1;
            }
        },
    )
    .unwrap();
    assert_eq!(negatives, 8);
    assert_eq!(p.service().diagnostics().cache_hits, 4);
    r = requests(&mut towns);
    clock.0.set(p.next_due());
    let mut recovered = 0;
    p.poll(
        &clock,
        &mut towns,
        &intents(&r, query("opened-negative", true)),
        |e| {
            if let Some((Terminal::Ready, result)) = e.adopted {
                result.unwrap();
                recovered += 1;
            }
        },
    )
    .unwrap();
    assert_eq!(recovered, 4);
}
#[test]
fn irrelevant_refresh_retains_work_relevant_refresh_restarts() {
    let (mut towns, _) = towns(4);
    let mut r = requests(&mut towns);
    r.truncate(1);
    let clock = Fake::default();
    let mut p = pump(4, 128);
    p.poll(
        &clock,
        &mut towns,
        &intents(&r, query("wall", false)),
        |_| {},
    )
    .unwrap();
    clock.0.set(p.next_due());
    p.poll(
        &clock,
        &mut towns,
        &intents(&r, query("distant-obstacle", true)),
        |_| {},
    )
    .unwrap();
    assert_eq!(p.service().diagnostics().restarts, 0);
    assert_eq!(p.service().work(&r[0].1), Some(8));
    clock.0.set(p.next_due());
    p.poll(
        &clock,
        &mut towns,
        &intents(&r, query("relevant-obstacle", true)),
        |_| {},
    )
    .unwrap();
    assert_eq!(p.service().diagnostics().restarts, 1);
    assert_eq!(p.service().work(&r[0].1), Some(4));
}
#[test]
fn stale_omitted_and_late_cancel_do_not_replace_actor_task() {
    let (mut towns, _) = towns(4);
    let old = requests(&mut towns);
    let clock = Fake::default();
    let mut p = pump(1, 128);
    p.poll(
        &clock,
        &mut towns,
        &intents(&old, query("wall", false)),
        |_| {},
    )
    .unwrap();
    let new = requests(&mut towns);
    let state = towns[0].actors()[0].state().clone();
    clock.0.set(p.next_due());
    p.poll(
        &clock,
        &mut towns,
        &intents(&new, query("wall", false)),
        |_| {},
    )
    .unwrap();
    assert_eq!(p.diagnostics().cancellations, 4);
    p.cancel(&old[0].1);
    assert!(p.service().status(&new[0].1).is_some());
    assert_eq!(*towns[0].actors()[0].state(), state);
    clock.0.set(p.next_due());
    p.poll(&clock, &mut towns, &[], |_| {}).unwrap();
    assert_eq!(p.service().resident_jobs(), 0);
}
#[test]
fn oversized_input_is_explicit_and_leaves_service_unchanged() {
    let (mut towns, _) = towns(4);
    let r = requests(&mut towns);
    let clock = Fake::default();
    let mut p = NavigationPump::new(
        Limits::default(),
        Cadence {
            max_intents: 1,
            ..Cadence::default()
        },
        Duration::ZERO,
    );
    assert_eq!(
        p.poll(
            &clock,
            &mut towns,
            &intents(&r, query("wall", false)),
            |_| {}
        ),
        Err(PollError::TooManyIntents)
    );
    assert_eq!(p.diagnostics().pumps, 0);
    assert_eq!(p.service().resident_jobs(), 0);
}
