// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! 2D geometry for manufacturing: contours made of line segments and circular arcs.
//! No dependency on the CAD kernel or on the rest of the project.

pub use glam::DVec2;
use std::f64::consts::TAU;

/// A line segment or a circular arc. Arcs run from `start` (radians) by `sweep`
/// (positive = counter-clockwise, negative = clockwise); `|sweep| = TAU` is a full circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line {
        a: DVec2,
        b: DVec2,
    },
    Arc {
        center: DVec2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Segment {
    pub fn point_at_angle(center: DVec2, radius: f64, ang: f64) -> DVec2 {
        center + DVec2::new(ang.cos(), ang.sin()) * radius
    }

    pub fn start_point(&self) -> DVec2 {
        match *self {
            Segment::Line { a, .. } => a,
            Segment::Arc {
                center,
                radius,
                start,
                ..
            } => Self::point_at_angle(center, radius, start),
        }
    }

    pub fn end_point(&self) -> DVec2 {
        match *self {
            Segment::Line { b, .. } => b,
            Segment::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::point_at_angle(center, radius, start + sweep),
        }
    }

    pub fn mid_point(&self) -> DVec2 {
        match *self {
            Segment::Line { a, b } => (a + b) * 0.5,
            Segment::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::point_at_angle(center, radius, start + sweep * 0.5),
        }
    }

    pub fn reversed(&self) -> Segment {
        match *self {
            Segment::Line { a, b } => Segment::Line { a: b, b: a },
            Segment::Arc {
                center,
                radius,
                start,
                sweep,
            } => Segment::Arc {
                center,
                radius,
                start: start + sweep,
                sweep: -sweep,
            },
        }
    }

    pub fn length(&self) -> f64 {
        match *self {
            Segment::Line { a, b } => a.distance(b),
            Segment::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }

    /// Direction of travel (radians) at the start.
    pub fn start_tangent(&self) -> f64 {
        match *self {
            Segment::Line { a, b } => (b - a).to_angle(),
            Segment::Arc { start, sweep, .. } => start + sweep.signum() * std::f64::consts::FRAC_PI_2,
        }
    }

    /// Signed curvature (positive = turns left). Zero for lines.
    pub fn curvature(&self) -> f64 {
        match *self {
            Segment::Line { .. } => 0.0,
            Segment::Arc { radius, sweep, .. } => sweep.signum() / radius,
        }
    }

    /// Contribution to the signed area (Green's theorem, counter-clockwise positive).
    pub fn area_term(&self) -> f64 {
        match *self {
            Segment::Line { a, b } => 0.5 * (a.x * b.y - b.x * a.y),
            Segment::Arc {
                center: c,
                radius: r,
                start,
                sweep,
            } => {
                let (s1, e1) = (start, start + sweep);
                0.5 * (r * c.x * (e1.sin() - s1.sin()) - r * c.y * (e1.cos() - s1.cos()) + r * r * sweep)
            }
        }
    }

    /// Points along the segment, including both ends. Arcs are split so that no step exceeds `max_step` radians.
    pub fn polyline(&self, max_step: f64) -> Vec<DVec2> {
        match *self {
            Segment::Line { a, b } => vec![a, b],
            Segment::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let n = ((sweep.abs() / max_step).ceil() as usize).max(2);
                (0..=n)
                    .map(|i| Self::point_at_angle(center, radius, start + sweep * i as f64 / n as f64))
                    .collect()
            }
        }
    }
}

/// A closed chain of segments (each starts where the previous one ends).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contour {
    pub segments: Vec<Segment>,
}

impl Contour {
    pub fn signed_area(&self) -> f64 {
        self.segments.iter().map(Segment::area_term).sum()
    }

    pub fn reversed(&self) -> Contour {
        Contour {
            segments: self.segments.iter().rev().map(Segment::reversed).collect(),
        }
    }

    pub fn polyline(&self, max_step: f64) -> Vec<DVec2> {
        let mut pts = Vec::new();
        for s in &self.segments {
            let p = s.polyline(max_step);
            pts.extend_from_slice(&p[..p.len() - 1]);
        }
        pts
    }

    /// Point-in-contour test (even-odd on a fine polygon approximation of the arcs).
    pub fn contains(&self, p: DVec2) -> bool {
        point_in_polygon(&self.polyline(0.05), p)
    }

    pub fn is_closed(&self, tol: f64) -> bool {
        let n = self.segments.len();
        n > 0
            && (0..n).all(|i| {
                self.segments[i]
                    .end_point()
                    .distance(self.segments[(i + 1) % n].start_point())
                    <= tol
            })
    }

    /// Total length of all segments.
    pub fn length(&self) -> f64 {
        self.segments.iter().map(Segment::length).sum()
    }
}

pub fn point_in_polygon(poly: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Normalises an angle into `[0, TAU)`.
pub fn norm_angle(a: f64) -> f64 {
    let r = a % TAU;
    if r < 0.0 {
        r + TAU
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn full_circle_area_and_square_area() {
        let c = Contour {
            segments: vec![Segment::Arc {
                center: DVec2::new(3.0, -2.0),
                radius: 5.0,
                start: 0.7,
                sweep: TAU,
            }],
        };
        assert!((c.signed_area() - PI * 25.0).abs() < 1e-9);
        assert!((c.reversed().signed_area() + PI * 25.0).abs() < 1e-9);
        let p = [
            DVec2::ZERO,
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 3.0),
            DVec2::new(0.0, 3.0),
        ];
        let sq = Contour {
            segments: (0..4)
                .map(|i| Segment::Line {
                    a: p[i],
                    b: p[(i + 1) % 4],
                })
                .collect(),
        };
        assert!((sq.signed_area() - 12.0).abs() < 1e-12);
        assert!(sq.is_closed(1e-9));
        assert!(sq.contains(DVec2::new(1.0, 1.0)) && !sq.contains(DVec2::new(5.0, 1.0)));
    }

    #[test]
    fn half_disc_area() {
        let arc = Segment::Arc {
            center: DVec2::ZERO,
            radius: 2.0,
            start: 0.0,
            sweep: PI,
        };
        let line = Segment::Line {
            a: DVec2::new(-2.0, 0.0),
            b: DVec2::new(2.0, 0.0),
        };
        let c = Contour {
            segments: vec![arc, line],
        };
        assert!((c.signed_area() - PI * 2.0).abs() < 1e-9);
    }
}
