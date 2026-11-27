// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Snapping to the curves of a sketch and of the face it is drawn on: the intersection of every two lines, arcs and circles, the quadrants of a circle or an arc, the
//! nearest point of a curve, and the foot of the perpendicular and the points of tangency seen from the last point. Pure functions of 2D geometry (tested below).

use glam::DVec2;
use std::f64::consts::{PI, TAU};

/// A line segment, or an arc of a circle (a whole circle has the sweep `TAU`; a negative sweep turns clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Seg(DVec2, DVec2),
    Arc {
        c: DVec2,
        r: f64,
        start: f64,
        sweep: f64,
    },
}

/// The angle `a` (radians) as a fraction along the arc from `start` by `sweep`, when it lies on it.
fn on_arc(a: f64, start: f64, sweep: f64) -> bool {
    if sweep.abs() >= TAU - 1e-9 {
        return true;
    }
    let (lo, hi) = if sweep >= 0.0 {
        (start, start + sweep)
    } else {
        (start + sweep, start)
    };
    let mut d = (a - lo).rem_euclid(TAU);
    if d > TAU - 1e-9 {
        d = 0.0;
    }
    d <= hi - lo + 1e-9
}

fn angle_of(c: DVec2, p: DVec2) -> f64 {
    (p.y - c.y).atan2(p.x - c.x)
}

impl Curve {
    /// Whether the point (known to lie on the circle of an arc) is on the arc itself.
    fn holds(&self, p: DVec2) -> bool {
        match *self {
            Curve::Seg(a, b) => {
                let d = b - a;
                let l2 = d.length_squared();
                l2 > 1e-18 && {
                    let t = (p - a).dot(d) / l2;
                    (-1e-9..=1.0 + 1e-9).contains(&t)
                }
            }
            Curve::Arc { c, start, sweep, .. } => on_arc(angle_of(c, p), start, sweep),
        }
    }

    /// The nearest point of the curve to `p`.
    pub fn nearest(&self, p: DVec2) -> DVec2 {
        match *self {
            Curve::Seg(a, b) => {
                let d = b - a;
                let l2 = d.length_squared();
                if l2 < 1e-18 {
                    return a;
                }
                a + d * ((p - a).dot(d) / l2).clamp(0.0, 1.0)
            }
            Curve::Arc { c, r, start, sweep } => {
                let v = p - c;
                let on_circle = if v.length() < 1e-12 {
                    c + DVec2::new(r, 0.0)
                } else {
                    c + v.normalize() * r
                };
                if on_arc(angle_of(c, on_circle), start, sweep) {
                    return on_circle;
                }
                let end = |a: f64| c + DVec2::new(a.cos(), a.sin()) * r;
                let (e0, e1) = (end(start), end(start + sweep));
                if e0.distance(p) <= e1.distance(p) {
                    e0
                } else {
                    e1
                }
            }
        }
    }

    pub fn distance(&self, p: DVec2) -> f64 {
        self.nearest(p).distance(p)
    }

    /// The quadrant points (the angles 0, 90, 180 and 270 degrees) of an arc that lie on it.
    pub fn quadrants(&self) -> Vec<DVec2> {
        match *self {
            Curve::Seg(..) => Vec::new(),
            Curve::Arc { c, r, start, sweep } => (0..4)
                .map(|k| k as f64 * PI / 2.0)
                .filter(|a| on_arc(*a, start, sweep))
                .map(|a| c + DVec2::new(a.cos(), a.sin()) * r)
                .collect(),
        }
    }

    /// The feet of the perpendiculars from `from` to the curve (for a circle the two points of the line through the centre).
    pub fn feet_from(&self, from: DVec2) -> Vec<DVec2> {
        match *self {
            Curve::Seg(a, b) => {
                let d = b - a;
                let l2 = d.length_squared();
                if l2 < 1e-18 {
                    return Vec::new();
                }
                let t = (from - a).dot(d) / l2;
                if (0.0..=1.0).contains(&t) {
                    vec![a + d * t]
                } else {
                    Vec::new()
                }
            }
            Curve::Arc { c, r, .. } => {
                let v = from - c;
                if v.length() < 1e-9 {
                    return Vec::new();
                }
                let u = v.normalize();
                [c + u * r, c - u * r]
                    .into_iter()
                    .filter(|p| self.holds(*p))
                    .collect()
            }
        }
    }

    /// The points of the curve where a line from `from` touches it (the two tangent points of a circle seen from outside).
    pub fn tangent_points_from(&self, from: DVec2) -> Vec<DVec2> {
        match *self {
            Curve::Seg(..) => Vec::new(),
            Curve::Arc { c, r, .. } => {
                let d = from.distance(c);
                if d <= r * (1.0 + 1e-9) {
                    return Vec::new();
                }
                let (base, alpha) = (angle_of(c, from), (r / d).acos());
                [base + alpha, base - alpha]
                    .into_iter()
                    .map(|a| c + DVec2::new(a.cos(), a.sin()) * r)
                    .filter(|p| self.holds(*p))
                    .collect()
            }
        }
    }
}

/// The points where two curves cross.
pub fn intersections(a: &Curve, b: &Curve) -> Vec<DVec2> {
    match (*a, *b) {
        (Curve::Seg(p, q), Curve::Seg(r, s)) => {
            let (d1, d2) = (q - p, s - r);
            let den = d1.perp_dot(d2);
            if den.abs() < 1e-12 {
                return Vec::new();
            }
            let t = (r - p).perp_dot(d2) / den;
            let u = (r - p).perp_dot(d1) / den;
            if (-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u) {
                vec![p + d1 * t]
            } else {
                Vec::new()
            }
        }
        (Curve::Seg(p, q), arc @ Curve::Arc { c, r, .. })
        | (arc @ Curve::Arc { c, r, .. }, Curve::Seg(p, q)) => {
            let d = q - p;
            let f = p - c;
            let (aa, bb, cc) = (d.length_squared(), 2.0 * f.dot(d), f.length_squared() - r * r);
            if aa < 1e-18 {
                return Vec::new();
            }
            let disc = bb * bb - 4.0 * aa * cc;
            if disc < 0.0 {
                return Vec::new();
            }
            let sq = disc.sqrt();
            let mut out = Vec::new();
            for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
                if (-1e-9..=1.0 + 1e-9).contains(&t) {
                    let x = p + d * t;
                    if arc.holds(x) && !out.iter().any(|o: &DVec2| o.distance(x) < 1e-9) {
                        out.push(x);
                    }
                }
            }
            out
        }
        (Curve::Arc { c: c1, r: r1, .. }, Curve::Arc { c: c2, r: r2, .. }) => {
            let d = c1.distance(c2);
            if d < 1e-12 || d > r1 + r2 + 1e-9 || d < (r1 - r2).abs() - 1e-9 {
                return Vec::new();
            }
            let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let h = (r1 * r1 - x * x).max(0.0).sqrt();
            let u = (c2 - c1) / d;
            let mid = c1 + u * x;
            let perp = DVec2::new(-u.y, u.x);
            let mut out = Vec::new();
            for p in [mid + perp * h, mid - perp * h] {
                if a.holds(p) && b.holds(p) && !out.iter().any(|o: &DVec2| o.distance(p) < 1e-9) {
                    out.push(p);
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(x: f64, y: f64, r: f64) -> Curve {
        Curve::Arc {
            c: DVec2::new(x, y),
            r,
            start: 0.0,
            sweep: TAU,
        }
    }

    #[test]
    fn two_lines_cross_once_and_only_inside_both() {
        let a = Curve::Seg(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0));
        let b = Curve::Seg(DVec2::new(0.0, 10.0), DVec2::new(10.0, 0.0));
        assert_eq!(intersections(&a, &b), vec![DVec2::new(5.0, 5.0)]);
        let c = Curve::Seg(DVec2::new(20.0, 0.0), DVec2::new(30.0, 10.0));
        assert!(intersections(&a, &c).is_empty());
    }

    #[test]
    fn a_line_and_a_circle_cross_twice_an_arc_only_where_it_is() {
        let l = Curve::Seg(DVec2::new(-10.0, 0.0), DVec2::new(10.0, 0.0));
        let c = circle(0.0, 0.0, 5.0);
        let mut x = intersections(&l, &c);
        x.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(x.len(), 2);
        assert!((x[0].x + 5.0).abs() < 1e-9 && (x[1].x - 5.0).abs() < 1e-9);
        // the upper half only
        let half = Curve::Arc {
            c: DVec2::ZERO,
            r: 5.0,
            start: 0.0,
            sweep: PI,
        };
        assert_eq!(
            intersections(&l, &half).len(),
            2,
            "both crossings are the ends of the half circle"
        );
        let quarter = Curve::Arc {
            c: DVec2::ZERO,
            r: 5.0,
            start: PI / 2.0,
            sweep: PI / 2.0,
        };
        assert_eq!(intersections(&l, &quarter).len(), 1);
        // a line that misses
        let m = Curve::Seg(DVec2::new(-10.0, 9.0), DVec2::new(10.0, 9.0));
        assert!(intersections(&m, &c).is_empty());
    }

    #[test]
    fn two_circles_cross_in_two_points_or_none() {
        let a = circle(0.0, 0.0, 5.0);
        let b = circle(6.0, 0.0, 5.0);
        let x = intersections(&a, &b);
        assert_eq!(x.len(), 2);
        assert!(x
            .iter()
            .all(|p| (p.x - 3.0).abs() < 1e-9 && (p.length() - 5.0).abs() < 1e-9));
        assert!(intersections(&a, &circle(20.0, 0.0, 5.0)).is_empty());
    }

    #[test]
    fn a_circle_has_four_quadrants_an_arc_those_it_holds() {
        assert_eq!(circle(1.0, 2.0, 3.0).quadrants().len(), 4);
        let q = Curve::Arc {
            c: DVec2::ZERO,
            r: 2.0,
            start: 0.1,
            sweep: PI,
        };
        assert_eq!(q.quadrants().len(), 2, "90 and 180 degrees");
    }

    #[test]
    fn the_nearest_point_the_perpendicular_and_the_tangent() {
        let l = Curve::Seg(DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0));
        assert_eq!(l.nearest(DVec2::new(4.0, 3.0)), DVec2::new(4.0, 0.0));
        assert_eq!(l.nearest(DVec2::new(14.0, 3.0)), DVec2::new(10.0, 0.0));
        assert_eq!(l.feet_from(DVec2::new(4.0, 3.0)), vec![DVec2::new(4.0, 0.0)]);
        assert!(l.feet_from(DVec2::new(14.0, 3.0)).is_empty());
        let c = circle(0.0, 0.0, 5.0);
        let t = c.tangent_points_from(DVec2::new(13.0, 0.0));
        assert_eq!(t.len(), 2);
        for p in &t {
            // the radius to the point is square to the line from the point of view
            let to_view = DVec2::new(13.0, 0.0) - *p;
            assert!(p.dot(to_view).abs() < 1e-9);
        }
        assert!(
            c.tangent_points_from(DVec2::new(1.0, 0.0)).is_empty(),
            "no tangent from inside"
        );
        assert_eq!(c.feet_from(DVec2::new(8.0, 0.0)).len(), 2);
    }
}
