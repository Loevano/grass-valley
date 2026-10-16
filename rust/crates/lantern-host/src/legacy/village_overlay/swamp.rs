//! Explicit complete swamp apply phase; metadata selection/migration stays external.
use super::*;
use std::collections::{BTreeMap, VecDeque};

type Point = (i32, i32);
type Addition = (&'static str, i32, i32, &'static str, &'static str);
const ROOT: &str = "$.swamp_overlay";
const SITES: &[Point] = &[
    (-23, -8),
    (-20, -9),
    (-18, -7),
    (-22, -4),
    (-18, -1),
    (-15, -5),
    (-14, 3),
    (-17, 3),
    (-15, 6),
    (-11, 10),
    (-8, 10),
    (-8, 14),
    (-11, -19),
    (-7, -18),
    (-12, -15),
    (-6, -13),
    (-3, -17),
    (-2, -13),
    (2, -11),
    (5, -12),
    (9, -12),
    (11, -9),
    (5, -7),
    (11, -3),
    (14, -1),
    (19, -2),
    (12, 4),
    (20, 3),
    (22, 5),
    (24, 8),
    (27, 8),
    (23, 12),
    (19, 12),
    (-3, 11),
    (3, 10),
    (4, 14),
    (8, 13),
    (9, 18),
    (13, 19),
    (6, 21),
    (12, 22),
    (0, 15),
];
const STREETS: &[&[Point]] = &[
    &[(-25, -6), (-19, -6), (-19, -3), (-13, -3), (-13, 0), (0, 0)],
    &[
        (0, 0),
        (0, -5),
        (-1, -5),
        (-1, -10),
        (-5, -10),
        (-5, -15),
        (-9, -15),
        (-9, -21),
    ],
    &[
        (0, 0),
        (8, 0),
        (8, 2),
        (15, 2),
        (15, 7),
        (20, 7),
        (20, 10),
        (29, 10),
    ],
    &[
        (0, 0),
        (0, 6),
        (1, 6),
        (1, 12),
        (6, 12),
        (6, 16),
        (10, 16),
        (10, 24),
    ],
    &[(-13, 0), (-13, 7), (-10, 7), (-10, 12), (-5, 12), (-5, 16)],
    &[(-5, -10), (8, -10), (8, -4), (20, -4)],
];

/// Source-preserving private materialization of Python `swamp_village.apply` v0–2.
/// Includes organic geometry and record upgrades; never ensures world metadata,
/// installs cache entries, mutates archives, repairs tiny water or activates chunks.
/// ```compile_fail
/// use lantern_host::legacy::SwampOverlay;
/// fn leak(view: &SwampOverlay) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct SwampOverlay {
    source: Vec<u8>,
    record: Vec<u8>,
}
impl SwampOverlay {
    pub fn materialize(
        source: &[u8],
        villages: &[u8],
        key: ChunkKey,
        world_size: u32,
    ) -> Result<Self, BoundaryError> {
        if ![480, 1920].contains(&world_size)
            || key.x >= world_size / 40
            || key.y >= world_size / 40
        {
            return Err(BoundaryError::invalid(
                ROOT,
                "unsupported world bounds or chunk key",
            ));
        }
        let mut record = Node::parse(source, ROOT)?;
        let previous = version(&record, "swamp_version", 2, ROOT)?.unwrap_or(0);
        version(&record, "village_version", 3, ROOT)?;
        version(&record, "terrain_version", 7, ROOT)?;
        let (left, top) = (key.x as i32 * 40, key.y as i32 * 40);
        rows(&record, ROOT)?;
        validate_resources(&record, left, top, ROOT)?;
        if let Some(clock) = record.optional("clock", ROOT)? {
            clock.nonnegative_number(&format!("{ROOT}.clock"))?;
        }
        let nodes = Node::parse(villages, &format!("{ROOT}.villages"))?;
        let selected = nodes
            .array(&format!("{ROOT}.villages"))?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let village = Village::parse(node, i, world_size, ROOT)?;
                let swamp = version(node, "swamp_version", 2, &format!("{ROOT}.villages[{i}]"))?
                    .unwrap_or(0);
                Ok((village, swamp))
            })
            .collect::<Result<Vec<_>, BoundaryError>>()?;
        // Original marker is deliberately captured once, rather than refreshed
        // after each village; mixed/overlapping metadata depends on this order.
        for (village, swamp) in selected {
            if !village.expanded || swamp == 0 || previous >= swamp {
                continue;
            }
            if swamp == 1 {
                apply_legacy_swamp(&mut record, &village, left, top, ROOT)?;
                break; // Python returns even when this first eligible square misses.
            }
            apply_organic(&mut record, &village, left, top, previous)?;
        }
        let mut output = Vec::new();
        write_node(&record, &mut output);
        Ok(Self {
            source: source.to_vec(),
            record: output,
        })
    }
    pub fn private_source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn private_record_bytes(&self) -> &[u8] {
        &self.record
    }
}

struct Layout {
    roads: Vec<(Point, u8)>, // Python dict insertion order, including overwrites.
    decor: Vec<Addition>,
    footprint: BTreeSet<Point>,
    water: BTreeSet<Point>,
}
fn road(roads: &mut Vec<(Point, u8)>, point: Point, code: u8) {
    if let Some(entry) = roads.iter_mut().find(|entry| entry.0 == point) {
        entry.1 = code;
    } else {
        roads.push((point, code));
    }
}
fn connect(
    start: Point,
    goals: &BTreeSet<Point>,
    blocked: &BTreeSet<Point>,
    canal: &BTreeSet<Point>,
    roads: &mut Vec<(Point, u8)>,
) {
    let mut queue = VecDeque::from([start]);
    let mut parents = BTreeMap::from([(start, None)]);
    while let Some(point) = queue.pop_front() {
        if goals.contains(&point) {
            let mut cursor = Some(point);
            while let Some(p) = cursor {
                road(roads, p, if canal.contains(&p) { b'w' } else { b'p' });
                cursor = parents[&p];
            }
            return;
        }
        let (x, y) = point;
        for next in [(x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)] {
            if parents.contains_key(&next)
                || blocked.contains(&next)
                || next.0.abs().max(next.1.abs()) > 30
            {
                continue;
            }
            parents.insert(next, Some(point));
            queue.push_back(next);
        }
    }
    unreachable!("static Reedhaven lane is unreachable")
}
fn layout() -> &'static Layout {
    static LAYOUT: std::sync::OnceLock<Layout> = std::sync::OnceLock::new();
    LAYOUT.get_or_init(|| {
        let mut blocked: BTreeSet<Point> = BASE
            .iter()
            .chain(EXTRA.iter())
            .chain(PLANTS.iter())
            .map(|&(_, x, y)| (x, y))
            .collect();
        blocked.extend(LIFE.iter().map(|&(_, x, y, _, _)| (x, y)));
        blocked.extend(SITES);
        let canal: BTreeSet<Point> = [16, 17]
            .into_iter()
            .flat_map(|x| (-8..15).map(move |y| (x, y)))
            .collect();
        let mut roads = Vec::new();
        for street in STREETS {
            for pair in street.windows(2) {
                connect(
                    pair[0],
                    &BTreeSet::from([pair[1]]),
                    &blocked,
                    &canal,
                    &mut roads,
                );
            }
        }
        for ((x, y), _) in roads.clone() {
            if !blocked.contains(&(x, y + 1)) && y.abs() < 10 {
                road(
                    &mut roads,
                    (x, y + 1),
                    if canal.contains(&(x, y + 1)) {
                        b'w'
                    } else {
                        b'p'
                    },
                );
            }
        }
        let mut homes = Vec::new();
        for &(x, y) in SITES {
            let home = [(x, y + 1), (x + 1, y), (x - 1, y), (x, y - 1)]
                .into_iter()
                .min_by_key(|&(hx, hy)| {
                    roads
                        .iter()
                        .map(|&((rx, ry), _)| (hx - rx).abs() + (hy - ry).abs())
                        .min()
                        .unwrap()
                })
                .unwrap();
            connect(
                home,
                &roads.iter().map(|r| r.0).collect(),
                &blocked,
                &canal,
                &mut roads,
            );
            homes.push(home);
        }
        for stop in [
            (-1, 1),
            (1, 1),
            (-6, -1),
            (6, -1),
            (-6, 4),
            (6, 4),
            (-1, -6),
            (1, 7),
            (-4, -4),
            (6, -7),
            (-4, 6),
            (-6, -7),
            (2, 8),
            (-2, -7),
            (-2, 8),
            (2, 3),
            (10, 6),
            (11, 6),
            (10, 7),
            (11, 7),
            (10, 8),
            (11, 8),
            (13, 5),
            (11, 6),
            (13, 9),
            (-11, 9),
        ] {
            connect(
                stop,
                &roads.iter().map(|r| r.0).collect(),
                &blocked,
                &canal,
                &mut roads,
            );
        }
        for (cx, cy, rx, ry) in [(2, 3, 4, 3), (12, 6, 3, 2)] {
            for y in cy - ry..=cy + ry {
                for x in cx - rx..=cx + rx {
                    // Python performs two binary64 divisions and powers before the sum.
                    if (f64::from(x - cx) / f64::from(rx)).powi(2)
                        + (f64::from(y - cy) / f64::from(ry)).powi(2)
                        <= 1.0
                        && !blocked.contains(&(x, y))
                    {
                        road(&mut roads, (x, y), b'c');
                    }
                }
            }
        }
        let mut reserved = blocked.clone();
        reserved.extend(roads.iter().map(|r| r.0));
        reserved.extend(&canal);
        reserved.extend(homes);
        let offsets = [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, 1),
            (1, 1),
            (-1, -1),
            (1, -1),
            (-2, 0),
            (2, 0),
            (0, -2),
            (0, 2),
        ];
        let mut decor = Vec::new();
        for (index, &(x, y)) in SITES.iter().enumerate() {
            for (variant, label) in [
                ("bench", "Bench"),
                ("candle_lantern", "Candle lantern"),
                (
                    ["ferns", "cattails", "mushrooms", "reeds"][index % 4],
                    ["Ferns", "Cattails", "Mushrooms", "Reeds"][index % 4],
                ),
            ] {
                let p = offsets
                    .iter()
                    .map(|&(dx, dy)| (x + dx, y + dy))
                    .find(|p| !reserved.contains(p))
                    .expect("static decoration space");
                reserved.insert(p);
                decor.push(("village_decor", p.0, p.1, variant, label));
            }
        }
        for (x, y) in [(10, 5), (14, 4), (14, 8), (-10, 8)] {
            let p = if reserved.contains(&(x, y)) {
                (-4..=4)
                    .flat_map(|dx| (-4..=4).map(move |dy| (x + dx, y + dy)))
                    .filter(|p| !reserved.contains(p))
                    .min_by_key(|&(px, py)| (px - x).abs() + (py - y).abs())
                    .unwrap()
            } else {
                (x, y)
            };
            reserved.insert(p);
            decor.push(("village_decor", p.0, p.1, "barrels", "Ale barrels"));
        }
        let mut footprint: BTreeSet<Point> = (-9..=9)
            .flat_map(|y| (-9..=9).map(move |x| (x, y)))
            .collect();
        for &(x, y) in &reserved {
            let radius: i32 = if SITES.contains(&(x, y)) { 2 } else { 1 };
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if dx * dx + dy * dy <= radius * radius + 1 {
                        footprint.insert((x + dx, y + dy));
                    }
                }
            }
        }
        let water = canal
            .into_iter()
            .filter(|p| !roads.iter().any(|r| r.0 == *p) && !blocked.contains(p))
            .collect();
        Layout {
            roads,
            decor,
            footprint,
            water,
        }
    })
}
fn apply_organic(
    record: &mut Node,
    village: &Village,
    left: i32,
    top: i32,
    previous: u64,
) -> Result<(), BoundaryError> {
    let data = layout();
    let touched: BTreeSet<Point> = data.footprint.union(&data.water).copied().collect();
    let (vx, vy) = (village.x, village.y);
    if !touched
        .iter()
        .any(|&(x, y)| inside(vx + x, vy + y, left, top))
    {
        return Ok(());
    }
    let mut terrain = rows(record, ROOT)?;
    let core = |x: i32, y: i32| (x - vx).abs().max((y - vy).abs()) <= 9;
    for field in ["map_resources", "tree_stumps"] {
        field_mut(record, field).retain(|item| {
            let (x, y) = position(item, ROOT).unwrap();
            core(x, y)
                || !(touched.contains(&(x - vx, y - vy))
                    || item
                        .optional("village", ROOT)
                        .unwrap()
                        .is_some_and(|n| n.string(ROOT).is_ok_and(|name| name == village.name)))
        });
    }
    if previous == 1 {
        for y in -19..=18 {
            for x in -19..=18 {
                if !touched.contains(&(x, y)) && inside(vx + x, vy + y, left, top) {
                    terrain[(vy + y - top) as usize][(vx + x - left) as usize] = b'b';
                    if (x * 11 + y * 7).rem_euclid(9) == 0
                        && !field_mut(record, "map_resources")
                            .iter()
                            .any(|item| position(item, ROOT).unwrap() == (vx + x, vy + y))
                    {
                        append(
                            record,
                            Node::Object(vec![
                                ("kind".into(), scalar("tree")),
                                ("x".into(), scalar(vx + x)),
                                ("y".into(), scalar(vy + y)),
                            ]),
                        );
                    }
                }
            }
        }
    }
    for &(x, y) in &touched {
        if inside(vx + x, vy + y, left, top) && !core(vx + x, vy + y) {
            terrain[(vy + y - top) as usize][(vx + x - left) as usize] =
                if data.water.contains(&(x, y)) {
                    b'b'
                } else {
                    b'g'
                };
        }
    }
    for &((x, y), material) in &data.roads {
        if inside(vx + x, vy + y, left, top) {
            terrain[(vy + y - top) as usize][(vx + x - left) as usize] = material;
        }
    }
    for (index, &(x, y)) in SITES.iter().enumerate() {
        let (kind, variant, label) = if (x, y) == (12, 4) {
            ("village_hut", "tavern", "The Crooked Candle")
        } else if index % 7 == 0 {
            ("village_market", "stall", "Marsh market stall")
        } else if index % 5 == 0 {
            ("village_hut", "workshop", "Reedworker’s shed")
        } else {
            (
                "village_hut",
                ["reed_house", "net_house", "moss_house"][index % 3],
                "Stilt cottage",
            )
        };
        append_swamp_item(record, village, left, top, (kind, x, y, variant, label));
    }
    for &item in &data.decor {
        append_swamp_item(record, village, left, top, item);
    }
    for &(x, y) in &data.water {
        if inside(vx + x, vy + y, left, top) {
            append(
                record,
                Node::Object(vec![
                    ("kind".into(), scalar("water")),
                    ("x".into(), scalar(vx + x)),
                    ("y".into(), scalar(vy + y)),
                ]),
            );
        }
    }
    commit_rows(record, &terrain);
    set(record, "swamp_version", scalar(2));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_python_layout_order_materials_decor_reservations_and_footprint() {
        let expected: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/terrain/organic_swamp_layout.json"
        ))
        .unwrap();
        let data = layout();
        let roads: Vec<_> = data
            .roads
            .iter()
            .map(|&(p, code)| serde_json::json!([p, (code as char).to_string()]))
            .collect();
        let decor: Vec<_> = data.decor.iter().map(|&(kind,x,y,variant,label)| serde_json::json!({"kind":kind,"x":x,"y":y,"variant":variant,"label":label})).collect();
        assert_eq!(
            serde_json::json!({"roads":roads,"decor":decor,"footprint":data.footprint,"water":data.water}),
            expected
        );
    }
}
