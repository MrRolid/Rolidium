// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The distances of a point (the centre of a circle, a hole, a piece of a pattern) to the two nearest straight edges, as live dimensions that can be
//! typed over, and the two snaps that go with them: equal distance from both edges and the middle of an edge.

use crate::DVec2;

/// One straight edge near a point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeDim {
    /// The two ends of the edge.
    pub a: DVec2,
    pub b: DVec2,
    /// The foot of the perpendicular from the point.
    pub foot: DVec2,
    /// The distance of the point to the line of the edge.
    pub dist: f64,
    /// The unit vector from the foot to the point (the side the point is on).
    pub normal: DVec2,
}

impl EdgeDim {
    pub fn middle(&self) -> DVec2 {
        (self.a + self.b) * 0.5
    }
}

/// The nearest edge and, if there is one, the nearest edge that is not parallel to it (the two usually square to each other).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeDims {
    pub first: EdgeDim,
    pub second: Option<EdgeDim>,
}

/// What a snap to a place related to the edges means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSnap {
    /// Both distances are equal (a symmetric corner).
    Equal,
    /// The point stands over the middle of the nearest edge (of the first, or of the second).
    Middle(usize),
}

fn dim_of(p: DVec2, a: DVec2, b: DVec2) -> Option<EdgeDim> {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-12 {
        return None;
    }
    let t = (p - a).dot(d) / len2;
    // the foot has to lie on the edge (a little beyond its ends is allowed: a point right at a corner)
    let slack = 1e-6 / len2.sqrt();
    if !(-slack..=1.0 + slack).contains(&t) {
        return None;
    }
    let foot = a + d * t.clamp(0.0, 1.0);
    let v = p - foot;
    let dist = v.length();
    let normal = if dist > 1e-9 {
        v / dist
    } else {
        // on the edge: the side is any one of the two
        DVec2::new(-d.y, d.x) / len2.sqrt()
    };
    Some(EdgeDim {
        a,
        b,
        foot,
        dist,
        normal,
    })
}

/// The two nearest straight edges of `p` among `edges` (pairs of end points). The second one is the nearest of those that make an angle of
/// more than 30 degrees with the first. `None` when no edge has the foot of the perpendicular from `p` on it.
pub fn nearest_edges(p: DVec2, edges: &[(DVec2, DVec2)]) -> Option<EdgeDims> {
    let mut dims: Vec<EdgeDim> = edges.iter().filter_map(|(a, b)| dim_of(p, *a, *b)).collect();
    dims.sort_by(|x, y| x.dist.total_cmp(&y.dist));
    let first = *dims.first()?;
    let d1 = (first.b - first.a).normalize();
    let second = dims
        .iter()
        .skip(1)
        .find(|e| {
            let d2 = (e.b - e.a).normalize();
            (d1.x * d2.y - d1.y * d2.x).abs() > 0.5
        })
        .copied();
    Some(EdgeDims { first, second })
}

impl EdgeDims {
    /// The point that has the distances `d1` and `d2` to the two edges (`None`: as they are now), on the sides `p` is on. With one edge only,
    /// the point moves along the normal of that edge.
    pub fn place(&self, d1: Option<f64>, d2: Option<f64>) -> DVec2 {
        let (e1, e2) = (self.first, self.second);
        let t1 = d1.unwrap_or(e1.dist);
        let Some(e2) = e2 else {
            return e1.foot + e1.normal * t1;
        };
        let t2 = d2.unwrap_or(e2.dist);
        // n1 . (x - foot1) = t1 and n2 . (x - foot2) = t2
        let (n1, n2) = (e1.normal, e2.normal);
        let det = n1.x * n2.y - n1.y * n2.x;
        let r1 = t1 + n1.dot(e1.foot);
        let r2 = t2 + n2.dot(e2.foot);
        if det.abs() < 1e-9 {
            return e1.foot + n1 * t1;
        }
        DVec2::new((r1 * n2.y - r2 * n1.y) / det, (n1.x * r2 - n2.x * r1) / det)
    }

    /// The point where the snaps catch: equal distances (within `tol`), or over the middle of an edge (within `tol` along it). The point itself when
    /// nothing is near.
    pub fn snap(&self, p: DVec2, tol: f64) -> (DVec2, Option<EdgeSnap>) {
        // the middle of an edge: the point moves along the edge until its foot is the middle
        for (i, e) in [Some(self.first), self.second].into_iter().enumerate() {
            let Some(e) = e else { continue };
            let along = (p - e.middle()).dot((e.b - e.a).normalize());
            if along.abs() <= tol {
                let moved = p - (e.b - e.a).normalize() * along;
                return (moved, Some(EdgeSnap::Middle(i)));
            }
        }
        if let Some(e2) = self.second {
            if (self.first.dist - e2.dist).abs() <= tol {
                let d = (self.first.dist + e2.dist) * 0.5;
                return (self.place(Some(d), Some(d)), Some(EdgeSnap::Equal));
            }
        }
        (p, None)
    }
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

    #[test]
    fn the_two_nearest_edges_of_a_point_in_a_plate() {
        let d = nearest_edges(DVec2::new(5.0, 7.0), &rect(100.0, 60.0)).unwrap();
        assert!((d.first.dist - 5.0).abs() < 1e-9, "{d:?}");
        assert!((d.second.unwrap().dist - 7.0).abs() < 1e-9);
        let d = nearest_edges(DVec2::new(95.0, 55.0), &rect(100.0, 60.0)).unwrap();
        assert!((d.first.dist - 5.0).abs() < 1e-9 && (d.second.unwrap().dist - 5.0).abs() < 1e-9);
    }

    #[test]
    fn typed_distances_move_the_point() {
        let d = nearest_edges(DVec2::new(5.0, 7.0), &rect(100.0, 60.0)).unwrap();
        let p = d.place(Some(10.0), None);
        assert!((p - DVec2::new(10.0, 7.0)).length() < 1e-9, "{p:?}");
        let p = d.place(Some(10.0), Some(12.0));
        assert!((p - DVec2::new(10.0, 12.0)).length() < 1e-9, "{p:?}");
        // from the far corner: the distance is still to the nearest edges, on the side the point is
        let d = nearest_edges(DVec2::new(95.0, 55.0), &rect(100.0, 60.0)).unwrap();
        let p = d.place(Some(8.0), Some(9.0));
        assert!(
            (p - DVec2::new(92.0, 51.0)).length() < 1e-9 || (p - DVec2::new(91.0, 52.0)).length() < 1e-9,
            "{p:?}"
        );
    }

    #[test]
    fn edges_that_are_not_square_give_the_right_point() {
        // two edges at 60 degrees: the point with 5 and 7 mm to them
        let e = vec![
            (DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0)),
            (DVec2::new(0.0, 0.0), DVec2::new(50.0, 86.602_540_378)),
        ];
        let d = nearest_edges(DVec2::new(30.0, 10.0), &e).unwrap();
        let p = d.place(Some(5.0), Some(7.0));
        let back = nearest_edges(p, &e).unwrap();
        let mut ds = [back.first.dist, back.second.unwrap().dist];
        ds.sort_by(f64::total_cmp);
        assert!((ds[0] - 5.0).abs() < 1e-6 && (ds[1] - 7.0).abs() < 1e-6, "{ds:?}");
    }

    #[test]
    fn the_snaps_catch_equal_distance_and_the_middle_of_an_edge() {
        let d = nearest_edges(DVec2::new(5.0, 5.4), &rect(100.0, 60.0)).unwrap();
        let (p, k) = d.snap(DVec2::new(5.0, 5.4), 0.5);
        assert_eq!(k, Some(EdgeSnap::Equal));
        assert!((p - DVec2::new(5.2, 5.2)).length() < 1e-9, "{p:?}");
        let d = nearest_edges(DVec2::new(49.8, 5.0), &rect(100.0, 60.0)).unwrap();
        let (p, k) = d.snap(DVec2::new(49.8, 5.0), 0.5);
        assert_eq!(k, Some(EdgeSnap::Middle(0)));
        assert!((p.x - 50.0).abs() < 1e-9);
        let d = nearest_edges(DVec2::new(30.0, 8.0), &rect(100.0, 60.0)).unwrap();
        assert_eq!(d.snap(DVec2::new(30.0, 8.0), 0.5).1, None);
    }

    #[test]
    fn a_point_outside_every_edge_has_no_distances() {
        assert!(nearest_edges(DVec2::new(500.0, 500.0), &rect(10.0, 10.0)).is_none());
    }
}
