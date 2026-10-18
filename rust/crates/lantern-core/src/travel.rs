//! Generic authoritative travel. Task rules own arrival actions; presentation
//! samples and search state never own position or cargo.
use crate::units::{Position, Seconds};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    Left,
    #[default]
    Right,
}
impl Facing {
    pub fn get(self) -> i8 {
        match self {
            Self::Left => -1,
            Self::Right => 1,
        }
    }
    pub fn new(value: i8) -> Option<Self> {
        match value {
            -1 => Some(Self::Left),
            1 => Some(Self::Right),
            _ => None,
        }
    }
    pub(crate) fn moving(self, from: Position, to: Position) -> Self {
        let dx = to.x.get() - from.x.get();
        let dy = to.y.get() - from.y.get();
        if dx == dy {
            self
        } else if dx > dy {
            Self::Right
        } else {
            Self::Left
        }
    }
}

/// Validated speed in tiles per second (Euclidean worker metric).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speed(f64);
impl Speed {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    NotRequested,
    /// Includes navigation admission/resource waits, never a final failure.
    Waiting,
    Walking,
    Arrived,
    Unreachable,
    Blocked,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Advance {
    pub status: Status,
    /// Unused time is returned, never banked while waiting or blocked. The caller
    /// may use it for a subsequent task phase in the same simulation update.
    pub remaining: Seconds,
    /// True once per accepted route, only when movement consumes its final point.
    /// This is a travel notification, not a pickup/unload/work authorization.
    pub arrived_now: bool,
    pub segments: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: Position,
    pub facing: Facing,
    pub status: Status,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub time: Seconds,
    pub pose: Pose,
}
/// Latest confirmed update only; no future route, cargo or authoritative actions.
/// Rebuilt after reload. Consumers order updates using the host snapshot stream
/// (R21), rather than treating these relative sample times as a global clock.
#[derive(Clone, Debug, PartialEq)]
pub struct Playback {
    pub(crate) duration: Seconds,
    pub(crate) samples: Vec<Sample>,
}
impl Playback {
    pub fn stationary(pose: Pose) -> Self {
        Self {
            duration: Seconds::new(0.0).unwrap(),
            samples: alloc::vec![Sample {
                time: Seconds::new(0.0).unwrap(),
                pose
            }],
        }
    }
    pub fn duration(&self) -> Seconds {
        self.duration
    }
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }
    pub(crate) fn push(&mut self, time: Seconds, pose: Pose) {
        let sample = Sample { time, pose };
        if self.samples.last().is_some_and(|s| s.time == time) {
            *self.samples.last_mut().unwrap() = sample;
        } else {
            self.samples.push(sample);
        }
    }
    /// Interpolate only confirmed intervals, preserving corners and stationary
    /// waits. Clamp beyond the last sample; never extrapolate along a ready route.
    pub fn pose_at(&self, time: Seconds) -> Pose {
        for pair in self.samples.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if time < b.time {
                if a.pose.position == b.pose.position {
                    return a.pose;
                }
                let fraction =
                    ((time.get() - a.time.get()) / (b.time.get() - a.time.get())).max(0.0);
                return Pose {
                    position: Position::new(
                        a.pose.position.x.get()
                            + (b.pose.position.x.get() - a.pose.position.x.get()) * fraction,
                        a.pose.position.y.get()
                            + (b.pose.position.y.get() - a.pose.position.y.get()) * fraction,
                    )
                    .expect("confirmed finite movement interval"),
                    facing: b.pose.facing,
                    status: a.pose.status,
                };
            }
        }
        self.samples
            .last()
            .expect("playback always has an anchor")
            .pose
    }
}

/// Presentation-only catch-up after missed observations. The target must be a
/// confirmed pose. Cap correction to walking speed, without changing any actor
/// facts or authorizing work. Geometry-aware scene joining remains the consumer's
/// responsibility. An unrepresentable visual distance rejects instead of warping.
pub fn rejoin(visible: Pose, confirmed: Pose, elapsed: Seconds, speed: Speed) -> Option<Pose> {
    let dx = confirmed.position.x.get() - visible.position.x.get();
    let dy = confirmed.position.y.get() - visible.position.y.get();
    let distance = libm::hypot(dx, dy);
    if !distance.is_finite() {
        return None;
    }
    let duration = distance / speed.get();
    if distance == 0.0 || duration <= elapsed.get() {
        return Some(confirmed);
    }
    let fraction = if duration.is_infinite() {
        elapsed.get() * speed.get() / distance
    } else {
        elapsed.get() / duration
    };
    let position = Position::new(
        visible.position.x.get() + dx * fraction,
        visible.position.y.get() + dy * fraction,
    )
    .ok()?;
    Some(Pose {
        position,
        facing: if position == visible.position {
            visible.facing
        } else {
            visible.facing.moving(visible.position, position)
        },
        status: Status::Walking,
    })
}
