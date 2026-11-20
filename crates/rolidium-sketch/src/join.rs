// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Join ends: open lines and arcs that almost meet are connected by the shortest segments.
//!
//! A free end is an end that touches no other end. Free ends are paired from the closest pairs outwards, every end at most once,
//! only up to a largest distance. A gap below [`WELD_BELOW`] is closed by moving the two ends to their middle; a larger one gets a
//! straight segment (a bridge).

use crate::{DVec2, Entity, EntityId};
use glam::DVec3;

/// Gaps below this (mm) are welded: both ends move to the common middle point.
pub const WELD_BELOW: f64 = 0.01;
/// Ends closer than this (mm) touch.
pub const TOUCH: f64 = 1e-6;

/// One connection of two free ends: the ends (indices into the list given), their distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub gap: f64,
}

/// The pairing of free ends. `ends[i] = (owner, point)`; ends of one owner are never paired with each other.
/// Returns the pairs (closest first) and the indices of the ends that stay free.
pub fn pair_ends(ends: &[(usize, DVec3)], max_gap: f64) -> (Vec<Pair>, Vec<usize>) {
    // an end that touches another end is not free
    let free: Vec<usize> = (0..ends.len())
        .filter(|&i| {
            !(0..ends.len())
                .any(|j| j != i && ends[j].0 != ends[i].0 && ends[i].1.distance(ends[j].1) <= TOUCH)
        })
        .collect();
    let mut cand: Vec<Pair> = Vec::new();
    for (k, &i) in free.iter().enumerate() {
        for &j in &free[k + 1..] {
            if ends[i].0 == ends[j].0 {
                continue;
            }
            let gap = ends[i].1.distance(ends[j].1);
            if gap <= max_gap + 1e-12 {
                cand.push(Pair { a: i, b: j, gap });
            }
        }
    }
    cand.sort_by(|x, y| x.gap.total_cmp(&y.gap).then(x.a.cmp(&y.a)).then(x.b.cmp(&y.b)));
    let mut used = vec![false; ends.len()];
    let mut pairs = Vec::new();
    for p in cand {
        if !used[p.a] && !used[p.b] {
            used[p.a] = true;
            used[p.b] = true;
            pairs.push(p);
        }
    }
    let left = free.into_iter().filter(|i| !used[*i]).collect();
    (pairs, left)
}

/// The two ends of a line or an arc (`None` for a circle and a rectangle, which are closed).
pub fn entity_ends(e: &Entity) -> Option<[DVec2; 2]> {
    match *e {
        Entity::Line { a, b } => Some([a, b]),
        Entity::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let at = |t: f64| center + DVec2::new(t.cos(), t.sin()) * radius;
            Some([at(start), at(start + sweep)])
        }
        _ => None,
    }
}

/// The entity with one of its ends (0 first, 1 second) moved to `to`. An arc is drawn again through its other end, its middle
/// and the new end.
pub fn with_end_moved(e: &Entity, which: usize, to: DVec2) -> Option<Entity> {
    match *e {
        Entity::Line { a, b } => Some(if which == 0 {
            Entity::Line { a: to, b }
        } else {
            Entity::Line { a, b: to }
        }),
        Entity::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let ends = entity_ends(e)?;
            let t = start + sweep * 0.5;
            let mid = center + DVec2::new(t.cos(), t.sin()) * radius;
            if which == 0 {
                Entity::arc_through(to, mid, ends[1])
            } else {
                Entity::arc_through(ends[0], mid, to)
            }
        }
        _ => None,
    }
}

/// What joining would do to a set of entities.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JoinPlan {
    /// Entities whose ends were moved (welded gaps).
    pub replaced: Vec<(EntityId, Entity)>,
    /// New segments for the larger gaps.
    pub bridges: Vec<(DVec2, DVec2)>,
    /// Every connection (the two points before joining), for the preview.
    pub connections: Vec<(DVec2, DVec2)>,
    /// Ends that stay free.
    pub free: Vec<DVec2>,
}

impl JoinPlan {
    /// Number of gaps that get closed.
    pub fn joined(&self) -> usize {
        self.connections.len()
    }
}

/// The plan for `entities` (the selection, or the whole sketch) and the largest gap `max_gap` (mm).
pub fn plan_join(entities: &[(EntityId, Entity)], max_gap: f64) -> JoinPlan {
    let mut ends: Vec<(usize, DVec3)> = Vec::new();
    // which end of which entity every entry is
    let mut which: Vec<(usize, usize)> = Vec::new();
    for (i, (_, e)) in entities.iter().enumerate() {
        if let Some(pts) = entity_ends(e) {
            for (k, p) in pts.iter().enumerate() {
                ends.push((i, DVec3::new(p.x, p.y, 0.0)));
                which.push((i, k));
            }
        }
    }
    let (pairs, free) = pair_ends(&ends, max_gap);
    let p2 = |i: usize| DVec2::new(ends[i].1.x, ends[i].1.y);
    let mut work: Vec<Entity> = entities.iter().map(|(_, e)| *e).collect();
    let mut touched = vec![false; entities.len()];
    let mut plan = JoinPlan::default();
    for p in pairs {
        let (pa, pb) = (p2(p.a), p2(p.b));
        plan.connections.push((pa, pb));
        if p.gap < WELD_BELOW {
            let mid = (pa + pb) * 0.5;
            for (end, _) in [(p.a, pa), (p.b, pb)] {
                let (ei, k) = which[end];
                if let Some(n) = with_end_moved(&work[ei], k, mid) {
                    work[ei] = n;
                    touched[ei] = true;
                }
            }
        } else {
            plan.bridges.push((pa, pb));
        }
    }
    for (i, t) in touched.iter().enumerate() {
        if *t {
            plan.replaced.push((entities[i].0, work[i]));
        }
    }
    plan.free = free.into_iter().map(p2).collect();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> Entity {
        Entity::Line {
            a: DVec2::new(a.0, a.1),
            b: DVec2::new(b.0, b.1),
        }
    }

    /// A 10 x 10 square made of four lines with a gap of `g` at every corner.
    fn gapped_square(g: f64) -> Vec<(EntityId, Entity)> {
        vec![
            (1, line((g, 0.0), (10.0 - g, 0.0))),
            (2, line((10.0, g), (10.0, 10.0 - g))),
            (3, line((10.0 - g, 10.0), (g, 10.0))),
            (4, line((0.0, 10.0 - g), (0.0, g))),
        ]
    }

    #[test]
    fn a_rectangle_with_gaps_in_the_corners_is_closed_by_bridges() {
        let plan = plan_join(&gapped_square(0.5), 1.5);
        assert_eq!(plan.joined(), 4);
        assert_eq!(plan.bridges.len(), 4);
        assert!(plan.free.is_empty());
        assert!(plan.replaced.is_empty());
    }

    #[test]
    fn a_gap_above_the_limit_stays_open_and_its_ends_are_reported() {
        let plan = plan_join(&gapped_square(0.9), 1.0);
        // the diagonal of a corner gap is 0.9 * sqrt(2) = 1.27 > 1.0: nothing is joined
        assert_eq!(plan.joined(), 0);
        assert_eq!(plan.free.len(), 8);
    }

    #[test]
    fn a_tiny_gap_is_welded_by_moving_both_ends_to_the_middle() {
        let mut ents = gapped_square(0.001);
        // make the corner gaps tiny along one axis only: 0.001 apart
        ents[0].1 = line((0.0, 0.0), (9.9995, 0.0));
        ents[1].1 = line((10.0, 0.0), (10.0, 10.0));
        let plan = plan_join(&ents, 1.0);
        assert!(plan.joined() >= 1);
        let moved: Vec<_> = plan.replaced.iter().map(|(id, _)| *id).collect();
        assert!(moved.contains(&1) && moved.contains(&2), "{moved:?}");
        // after welding, the two ends are the same point
        let a = plan.replaced.iter().find(|(id, _)| *id == 1).unwrap().1;
        let b = plan.replaced.iter().find(|(id, _)| *id == 2).unwrap().1;
        let (ea, eb) = (entity_ends(&a).unwrap(), entity_ends(&b).unwrap());
        assert!(ea[1].distance(eb[0]) < 1e-12, "{ea:?} {eb:?}");
    }

    #[test]
    fn every_end_is_used_at_most_once_and_the_closest_pairs_come_first() {
        // three ends on a line: 0, 0.4 and 1.0; the pair (0, 0.4) wins and 1.0 stays free
        let ends = vec![
            (0, DVec3::new(0.0, 0.0, 0.0)),
            (1, DVec3::new(0.4, 0.0, 0.0)),
            (2, DVec3::new(1.0, 0.0, 0.0)),
        ];
        let (pairs, free) = pair_ends(&ends, 2.0);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].a, pairs[0].b), (0, 1));
        assert_eq!(free, vec![2]);
    }

    #[test]
    fn ends_that_touch_are_not_free_and_the_same_entity_is_not_paired_with_itself() {
        let ents = vec![
            (1, line((0.0, 0.0), (5.0, 0.0))),
            (2, line((5.0, 0.0), (5.0, 5.0))),
        ];
        // the shared corner touches (it is no free end); the other two ends are 7 apart
        let plan = plan_join(&ents, 2.0);
        assert_eq!(plan.joined(), 0);
        assert_eq!(plan.free.len(), 2);
        assert_eq!(plan_join(&ents, 10.0).joined(), 1);
    }

    #[test]
    fn an_arc_end_is_welded_by_redrawing_the_arc() {
        let arc =
            Entity::arc_through(DVec2::new(0.0, 0.0), DVec2::new(5.0, 3.0), DVec2::new(10.0, 0.0)).unwrap();
        let l = line((10.004, 0.0), (10.0, -8.0));
        let plan = plan_join(&[(1, arc), (2, l)], 0.5);
        assert_eq!(plan.joined(), 1);
        let new_arc = plan.replaced.iter().find(|(id, _)| *id == 1).unwrap().1;
        let new_line = plan.replaced.iter().find(|(id, _)| *id == 2).unwrap().1;
        let (ea, el) = (entity_ends(&new_arc).unwrap(), entity_ends(&new_line).unwrap());
        assert!(ea[1].distance(el[0]) < 1e-9, "{ea:?} {el:?}");
    }
}
