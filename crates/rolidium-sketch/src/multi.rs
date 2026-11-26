// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The quick multiplication of a circle that is being drawn (the right button): the same circle in every corner of the closed shape it is inside, with
//! the same distances from the two nearest walls, or evenly round the centre of the circle it is inside.

use crate::DVec2;

/// What a circle that is being drawn is inside.
#[derive(Debug, Clone, PartialEq)]
pub enum Container {
    /// A closed shape of straight walls (a rectangle, a polygon): its corners in order.
    Polygon(Vec<DVec2>),
    /// A circle: its centre and radius.
    Circle(DVec2, f64),
}

fn signed_area(p: &[DVec2]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f64>() * 0.5
}

/// Whether `c` is inside the polygon (a ray to the right).
pub fn inside_polygon(p: &[DVec2], c: DVec2) -> bool {
    let n = p.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        if (a.y > c.y) != (b.y > c.y) {
            let x = a.x + (c.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x > c.x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The closed chains of straight segments (each end meets exactly one other segment, within `tol`), as lists of corners. Chains that do not close, or
/// that branch, are left out.
pub fn closed_loops(segs: &[(DVec2, DVec2)], tol: f64) -> Vec<Vec<DVec2>> {
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for start in 0..segs.len() {
        if used[start] || segs[start].0.distance(segs[start].1) < tol {
            continue;
        }
        let mut pts = vec![segs[start].0];
        let mut here = segs[start].1;
        let mut chain = vec![start];
        let mut ok = false;
        loop {
            if here.distance(pts[0]) <= tol && chain.len() >= 3 {
                ok = true;
                break;
            }
            // the segments that start at `here` (either end), not used yet
            let next: Vec<(usize, bool)> = (0..segs.len())
                .filter(|i| !chain.contains(i) && segs[*i].0.distance(segs[*i].1) >= tol)
                .filter_map(|i| {
                    if segs[i].0.distance(here) <= tol {
                        Some((i, false))
                    } else if segs[i].1.distance(here) <= tol {
                        Some((i, true))
                    } else {
                        None
                    }
                })
                .collect();
            if next.len() != 1 {
                break;
            }
            let (i, rev) = next[0];
            pts.push(here);
            here = if rev { segs[i].0 } else { segs[i].1 };
            chain.push(i);
            if chain.len() > segs.len() {
                break;
            }
        }
        if ok && pts.len() >= 3 {
            for i in chain {
                used[i] = true;
            }
            loops.push(pts);
        }
    }
    loops
}

/// Whether the points lie on a circle (all within one per cent of the radius of the mean distance from their centroid): the centre and the radius.
pub fn circle_of_points(pts: &[DVec2]) -> Option<(DVec2, f64)> {
    if pts.len() < 8 {
        return None;
    }
    let c = pts.iter().copied().sum::<DVec2>() / pts.len() as f64;
    let r = pts.iter().map(|p| p.distance(c)).sum::<f64>() / pts.len() as f64;
    (r > 1e-6 && pts.iter().all(|p| (p.distance(c) - r).abs() <= 0.01 * r)).then_some((c, r))
}

/// What the point `c` is inside: the smallest closed shape of straight walls (`loops`) or circle (`circles`) that contains it.
pub fn container_of(c: DVec2, loops: &[Vec<DVec2>], circles: &[(DVec2, f64)]) -> Option<Container> {
    let mut best: Option<(f64, Container)> = None;
    for l in loops.iter().filter(|l| l.len() >= 3 && inside_polygon(l, c)) {
        let a = signed_area(l).abs();
        if best.as_ref().is_none_or(|(ba, _)| a < *ba) {
            best = Some((a, Container::Polygon(l.clone())));
        }
    }
    for (m, r) in circles.iter().filter(|(m, r)| m.distance(c) < *r - 1e-9) {
        let a = std::f64::consts::PI * r * r;
        if best.as_ref().is_none_or(|(ba, _)| a < *ba) {
            best = Some((a, Container::Circle(*m, *r)));
        }
    }
    best.map(|(_, k)| k)
}

fn dist_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let l2 = d.length_squared();
    if l2 < 1e-18 {
        return p.distance(a);
    }
    p.distance(a + d * ((p - a).dot(d) / l2).clamp(0.0, 1.0))
}

/// The same point in the other corners of the polygon: in the corner the point is nearest to, it has the distances `a` to the wall that comes into the
/// corner and `b` to the wall that leaves it; the corner after it has the distances the other way round, and so on, so that a rectangle gets the mirror
/// images (the distances along the same sides stay the same). The place of the original is not in the answer.
pub fn corner_copies(poly: &[DVec2], c: DVec2) -> Vec<DVec2> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    // inward unit normal of the wall k (from poly[k] to poly[k+1])
    let ccw = signed_area(poly) > 0.0;
    let wall = |k: usize| (poly[k % n], poly[(k + 1) % n]);
    let inward = |k: usize| {
        let (a, b) = wall(k);
        let d = (b - a).normalize_or_zero();
        let left = DVec2::new(-d.y, d.x);
        if ccw {
            left
        } else {
            -left
        }
    };
    let dist = |k: usize| {
        let (a, b) = wall(k);
        dist_to_segment(c, a, b)
    };
    // the corner whose two walls are the nearest
    let Some(i0) = (0..n).min_by(|&x, &y| {
        let f = |i: usize| {
            let (p, q) = (dist((i + n - 1) % n), dist(i));
            p * p + q * q
        };
        f(x).total_cmp(&f(y))
    }) else {
        return Vec::new();
    };
    let a = dist((i0 + n - 1) % n);
    let b = dist(i0);
    let mut out = Vec::new();
    for j in 1..n {
        let k = (i0 + j) % n;
        let (win, wout) = ((k + n - 1) % n, k);
        let (da, db) = if j % 2 == 0 { (a, b) } else { (b, a) };
        let (n1, n2) = (inward(win), inward(wout));
        let (p1, p2) = (wall(win).0, wall(wout).0);
        // n1 . (P - p1) = da and n2 . (P - p2) = db
        let det = n1.x * n2.y - n1.y * n2.x;
        if det.abs() < 1e-9 {
            continue;
        }
        let (r1, r2) = (da + n1.dot(p1), db + n2.dot(p2));
        let p = DVec2::new((r1 * n2.y - r2 * n1.y) / det, (n1.x * r2 - n2.x * r1) / det);
        if inside_polygon(poly, p) {
            out.push(p);
        }
    }
    out
}

/// The `n` places (the original included as the first, not in the answer) evenly round `centre` that the point `p` is turned to: `n - 1` points.
pub fn around_copies(centre: DVec2, p: DVec2, n: usize) -> Vec<DVec2> {
    let v = p - centre;
    (1..n.max(1))
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            centre + DVec2::new(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f64, h: f64) -> Vec<(DVec2, DVec2)> {
        let c = [
            DVec2::ZERO,
            DVec2::new(w, 0.0),
            DVec2::new(w, h),
            DVec2::new(0.0, h),
        ];
        (0..4).map(|i| (c[i], c[(i + 1) % 4])).collect()
    }

    fn has(v: &[DVec2], x: f64, y: f64) -> bool {
        v.iter().any(|p| (p.x - x).abs() < 1e-6 && (p.y - y).abs() < 1e-6)
    }

    #[test]
    fn a_rectangle_gives_the_four_corners_with_the_same_distances() {
        let loops = closed_loops(&rect(100.0, 60.0), 1e-6);
        assert_eq!(loops.len(), 1);
        let c = DVec2::new(10.0, 15.0);
        let Some(Container::Polygon(p)) = container_of(c, &loops, &[]) else {
            panic!()
        };
        let v = corner_copies(&p, c);
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(
            has(&v, 90.0, 15.0) && has(&v, 10.0, 45.0) && has(&v, 90.0, 45.0),
            "{v:?}"
        );
    }

    #[test]
    fn a_rectangle_in_any_order_and_direction() {
        let mut segs = rect(40.0, 40.0);
        segs.reverse();
        segs[1] = (segs[1].1, segs[1].0);
        let loops = closed_loops(&segs, 1e-6);
        assert_eq!(loops.len(), 1);
        let c = DVec2::new(30.0, 5.0);
        let v = corner_copies(&loops[0], c);
        assert!(
            has(&v, 10.0, 5.0) && has(&v, 30.0, 35.0) && has(&v, 10.0, 35.0),
            "{v:?}"
        );
    }

    #[test]
    fn an_open_chain_is_not_a_closed_shape() {
        let mut segs = rect(50.0, 50.0);
        segs.pop();
        assert!(closed_loops(&segs, 1e-6).is_empty());
    }

    #[test]
    fn the_smaller_of_two_shapes_wins() {
        let outer = closed_loops(&rect(100.0, 100.0), 1e-6);
        let c = DVec2::new(50.0, 50.0);
        assert!(matches!(
            container_of(c, &outer, &[(DVec2::new(50.0, 50.0), 20.0)]),
            Some(Container::Circle(..))
        ));
        assert!(matches!(
            container_of(c, &outer, &[(DVec2::new(50.0, 50.0), 200.0)]),
            Some(Container::Polygon(_))
        ));
        assert!(container_of(DVec2::new(500.0, 500.0), &outer, &[]).is_none());
    }

    #[test]
    fn evenly_round_a_centre() {
        let v = around_copies(DVec2::new(10.0, 10.0), DVec2::new(20.0, 10.0), 4);
        assert_eq!(v.len(), 3);
        assert!(
            has(&v, 10.0, 20.0) && has(&v, 0.0, 10.0) && has(&v, 10.0, 0.0),
            "{v:?}"
        );
        assert_eq!(around_copies(DVec2::ZERO, DVec2::X, 1).len(), 0);
    }

    #[test]
    fn a_polygon_of_points_on_a_circle_is_a_circle() {
        let pts: Vec<DVec2> = (0..32)
            .map(|i| DVec2::new(5.0, 7.0) + DVec2::from_angle(i as f64 * 0.19625) * 12.0)
            .collect();
        let (c, r) = circle_of_points(&pts).unwrap();
        assert!(
            c.distance(DVec2::new(5.0, 7.0)) < 0.1 && (r - 12.0).abs() < 0.1,
            "{c:?} {r}"
        );
        assert!(circle_of_points(&rect(10.0, 10.0).iter().map(|s| s.0).collect::<Vec<_>>()).is_none());
    }
}
