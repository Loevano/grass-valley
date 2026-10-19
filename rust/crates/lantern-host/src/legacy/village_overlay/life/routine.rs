use super::*;
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
pub(super) struct Routine {
    x: f64,
    y: f64,
    path: Vec<[f64; 2]>,
    stop: i32,
    wait: f64,
    elapsed: f64,
    phase: String,
    job: String,
    facing: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<[f64; 2]>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "integral_goal"
    )]
    goal: Option<[i32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paused: Option<bool>,
    #[serde(default, skip_serializing)]
    source: Option<Source>,
    #[serde(skip)]
    source_present: bool,
}
#[derive(Deserialize, Serialize)]
struct Source {
    #[serde(deserialize_with = "integral_number")]
    x: i32,
    #[serde(deserialize_with = "integral_number")]
    y: i32,
}
impl Routine {
    pub(super) fn read(n: &Node) -> Result<Self, BoundaryError> {
        let mut s: Self =
            serde_json::from_slice(&bytes(n)).map_err(|_| invalid("invalid routine fields"))?;
        s.source_present = n.optional("source", PATH)?.is_some();
        if ![s.x, s.y, s.wait, s.elapsed].iter().all(|n| n.is_finite())
            || s.wait < 0.
            || s.elapsed < 0.
            || s.x.abs() > i32::MAX as f64 - 64.
            || s.y.abs() > i32::MAX as f64 - 64.
            || !(-1..=3).contains(&s.stop)
            || !matches!(s.facing, -1 | 1)
            || !matches!(
                s.phase.as_str(),
                "idle" | "walking" | "working" | "sleeping"
            )
            || !matches!(
                s.job.as_str(),
                "idle"
                    | "woodcutting"
                    | "fishing"
                    | "gathering"
                    | "husbandry"
                    | "baking"
                    | "crafting"
            )
            || s.path
                .iter()
                .chain(s.origin.iter())
                .flatten()
                .any(|n| !n.is_finite() || n.abs() > i32::MAX as f64 - 64.)
            || (!s.path.is_empty() && s.goal.is_none())
        {
            return Err(invalid("unsupported routine state"));
        }
        Ok(s)
    }
    fn initial(n: &Node, index: usize) -> Result<Self, BoundaryError> {
        let (x, y) = anchor(n)?;
        Ok(Self {
            x: f64::from(x),
            y: f64::from(y),
            path: vec![],
            stop: -1,
            wait: index as f64 * 0.8,
            elapsed: 0.,
            phase: "idle".into(),
            job: "idle".into(),
            facing: 1,
            origin: Some([f64::from(x), f64::from(y)]),
            goal: None,
            paused: None,
            source: None,
            source_present: false,
        })
    }
    fn write(&self, n: &mut Node) -> Result<(), BoundaryError> {
        if n.optional("routine", PATH)?.is_none() {
            set(n, "routine", Node::Object(vec![]));
        }
        let target = field_mut(n, "routine", PATH)?;
        let value = Node::parse(
            &serde_json::to_vec(self).map_err(|_| invalid("invalid routine output"))?,
            PATH,
        )?;
        for (k, v) in value.object(PATH)? {
            set(target, k, v.clone());
        }
        if self.source_present {
            set(
                target,
                "source",
                if let Some(s) = &self.source {
                    Node::Object(vec![("x".into(), scalar(s.x)), ("y".into(), scalar(s.y))])
                } else {
                    scalar(serde_json::Value::Null)
                },
            );
        }
        Ok(())
    }
}
fn work_stop(n: &Node, v: &Village) -> Result<(Point, &'static str, Option<Point>), BoundaryError> {
    if let Some(stop) = n.optional("ambient_stop", PATH)? {
        return Ok((point(stop)?, "idle", None));
    }
    if !v.life {
        return Ok(((2, 3), "idle", None));
    }
    let role = text(n, "role")?;
    Ok(
        if role.contains("woodworker") || role.contains("boatwright") {
            ((-4, -4), "woodcutting", Some((-5, -4)))
        } else if role.contains("fisher") {
            ((6, -7), "fishing", Some((7, -7)))
        } else if role.contains("herbalist") || role.contains("apprentice") {
            ((-4, 6), "gathering", Some((-5, 6)))
        } else if ["keeper", "warden", "tender"]
            .iter()
            .any(|s| role.contains(s))
            && !role.contains("innkeeper")
        {
            ((-6, -7), "husbandry", Some((-7, -7)))
        } else if role.contains("innkeeper") {
            ((2, 8), "baking", Some((3, 8)))
        } else if ["weaver", "dyer", "mason"].iter().any(|s| role.contains(s)) {
            ((-2, -7), "crafting", Some((-3, -7)))
        } else {
            ((2, 3), "idle", Some((3, 3)))
        },
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn advance(
    n: &mut Node,
    v: &Village,
    index: usize,
    seconds: f64,
    blocked: &BTreeSet<Point>,
    fences: &[Segment],
    players: &[(f64, f64)],
) -> Result<(), BoundaryError> {
    let mut s = if let Some(state) = n.optional("routine", PATH)? {
        Routine::read(state)?
    } else {
        Routine::initial(n, index)?
    };
    if opt_text(n, "pose")?.as_deref() == Some("passed_out") {
        s.path.clear();
        s.phase = "sleeping".into();
        s.job = "idle".into();
        s.paused = Some(false);
        return s.write(n);
    }
    s.paused = Some(
        players
            .iter()
            .any(|&(x, y)| (x - s.x).hypot(y - s.y) <= 1.8),
    );
    if s.paused == Some(true) {
        return s.write(n);
    }
    let mut remaining = seconds;
    for _ in 0..32 {
        if remaining <= 0. {
            break;
        }
        if let Some(mut target) = s.path.first().map(|p| (p[0], p[1])) {
            if integral(target).is_some_and(|t| blocked.contains(&t))
                || !segment_open((s.x, s.y), target, fences)
            {
                let origin = s.origin.map(|p| (p[0], p[1])).unwrap_or(grid(anchor(n)?));
                let goal = s.goal.ok_or_else(|| invalid("missing route goal"))?;
                // A legacy swamp repair may retain a fractional origin. A
                // cardinal lattice from it cannot reach the integral goal.
                let detour = integral(origin)
                    .map(|o| route(o, (goal[0], goal[1]), blocked, v, fences, true))
                    .unwrap_or_default();
                s.path = if (s.x, s.y) != origin {
                    vec![[origin.0, origin.1]]
                } else {
                    vec![]
                };
                if detour.is_empty() {
                    s.job = "idle".into();
                    s.source = None;
                    s.source_present = true;
                }
                s.path.extend(
                    detour
                        .into_iter()
                        .map(|(x, y)| [f64::from(x), f64::from(y)]),
                );
                if s.path.is_empty() {
                    s.phase = "idle".into();
                    break;
                }
                target = (s.path[0][0], s.path[0][1]);
            }
            let (dx, dy) = (target.0 - s.x, target.1 - s.y);
            let duration = dx.hypot(dy) / 0.65;
            let used = remaining.min(duration);
            let fraction = if duration != 0. { used / duration } else { 1. };
            s.x += dx * fraction;
            s.y += dy * fraction;
            s.facing = if dx - dy >= 0. { 1 } else { -1 };
            s.phase = "walking".into();
            remaining -= used;
            if used < duration {
                break;
            }
            s.path.remove(0);
            s.origin = Some([target.0, target.1]);
            if s.path.is_empty() {
                s.phase = if s.job != "idle" && s.stop == 0 {
                    "working"
                } else {
                    "idle"
                }
                .into();
                s.elapsed = 0.;
            }
            continue;
        }
        if s.wait > 0. {
            let used = remaining.min(s.wait);
            s.wait -= used;
            s.elapsed += used;
            remaining -= used;
            continue;
        }
        let (work, mut job, mut source) = work_stop(n, v)?;
        let (vx, vy) = v.center;
        if source.is_some_and(|(x, y)| !blocked.contains(&(vx + x, vy + y))) {
            job = "idle";
            source = None;
        }
        let hall = if v.life {
            (vx - 2, vy + 8)
        } else {
            (vx - 2, vy - 2)
        };
        let market = if n.optional("ambient_stop", PATH)?.is_some() {
            (vx + 10 + (index % 2) as i32, vy + 6 + (index % 3) as i32)
        } else {
            (vx + 2, vy + 3)
        };
        let stops = [(vx + work.0, vy + work.1), market, hall, anchor(n)?];
        s.stop = (s.stop + 1) % 4;
        let goal = stops[s.stop as usize];
        s.goal = Some([goal.0, goal.1]);
        s.job = if s.stop == 0 { job } else { "idle" }.into();
        s.source = source.filter(|_| s.stop == 0).map(|(x, y)| Source {
            x: vx + x,
            y: vy + y,
        });
        s.source_present = true;
        s.wait = [8., 4., 3., 5.][s.stop as usize];
        s.elapsed = 0.;
        s.path = route(
            (s.x.round_ties_even() as i32, s.y.round_ties_even() as i32),
            goal,
            blocked,
            v,
            fences,
            true,
        )
        .into_iter()
        .map(|(x, y)| [f64::from(x), f64::from(y)])
        .collect();
        s.phase = if s.path.is_empty() { "idle" } else { "walking" }.into();
    }
    // Formatting rounds binary64 itself, matching Python round(x,6) rather than
    // introducing an extra multiplication-rounding boundary at x*1e6.
    s.x = format!("{:.6}", s.x).parse().unwrap();
    s.y = format!("{:.6}", s.y).parse().unwrap();
    s.write(n)
}

fn integral(p: (f64, f64)) -> Option<Point> {
    (p.0.fract() == 0. && p.1.fract() == 0.).then_some((p.0 as i32, p.1 as i32))
}

fn integral_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
    let n = f64::deserialize(d)?;
    checked_coordinate(n).ok_or_else(|| serde::de::Error::custom("expected integral coordinate"))
}
fn checked_coordinate(n: f64) -> Option<i32> {
    (n.is_finite() && n.fract() == 0. && n >= i32::MIN as f64 && n <= i32::MAX as f64)
        .then_some(n as i32)
}
fn integral_goal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<[i32; 2]>, D::Error> {
    let pair = Option::<[f64; 2]>::deserialize(d)?;
    pair.map(|p| {
        Ok([
            checked_coordinate(p[0]).ok_or_else(|| serde::de::Error::custom("invalid goal"))?,
            checked_coordinate(p[1]).ok_or_else(|| serde::de::Error::custom("invalid goal"))?,
        ])
    })
    .transpose()
}
