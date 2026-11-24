// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Drawing aids of the sketch: alignment guides, extensions, intersections, parallel and
//! perpendicular directions, mirrored helper points, and the points of 3D features projected into the
//! sketch plane. Pure functions on 2D data, so they can be tested.

use crate::sketchmode::SnapLabel;
use rolidium_doc::rolidium_sketch::DVec2;

/// A dotted guide line from `a` to `b` (sketch coordinates).
pub type Guide = (DVec2, DVec2);

#[derive(Clone, Debug)]
pub struct GuideSnap {
    pub pos: DVec2,
    pub label: SnapLabel,
    pub guides: Vec<Guide>,
}

pub struct GuideInput<'a> {
    pub raw: DVec2,
    pub mm_per_px: f64,
    /// Points other things can align with: ends, midpoints, centres, face vertices, projections.
    pub refs: &'a [DVec2],
    /// Straight pieces: sketch lines, rectangle sides, face boundary edges.
    pub segments: &'a [(DVec2, DVec2)],
    /// First point of the line being drawn (for parallel / perpendicular).
    pub line_start: Option<DVec2>,
    /// Helper points that can be snapped to (mirrored copies).
    pub helpers: &'a [DVec2],
}

/// Intersection of two segments, if they cross (or touch) inside both.
pub fn segment_intersection(a: (DVec2, DVec2), b: (DVec2, DVec2)) -> Option<DVec2> {
    let r = a.1 - a.0;
    let s = b.1 - b.0;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let qp = b.0 - a.0;
    let t = qp.perp_dot(s) / den;
    let u = qp.perp_dot(r) / den;
    let e = 1e-9;
    ((-e..=1.0 + e).contains(&t) && (-e..=1.0 + e).contains(&u)).then(|| a.0 + r * t)
}

/// Intersection of two infinite lines.
fn line_intersection(a: (DVec2, DVec2), b: (DVec2, DVec2)) -> Option<DVec2> {
    let r = a.1 - a.0;
    let s = b.1 - b.0;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (b.0 - a.0).perp_dot(s) / den;
    Some(a.0 + r * t)
}

#[allow(dead_code)]
/// Mirrored copies of `points` over the two axes of symmetry of a rectangle (centre `c`) and over both.
pub fn mirrored_points(points: &[DVec2], c: DVec2) -> Vec<DVec2> {
    let mut out = Vec::new();
    for p in points {
        for q in [
            DVec2::new(2.0 * c.x - p.x, p.y),
            DVec2::new(p.x, 2.0 * c.y - p.y),
            DVec2::new(2.0 * c.x - p.x, 2.0 * c.y - p.y),
        ] {
            if q.distance(*p) > 1e-6 && !out.iter().any(|o: &DVec2| o.distance(q) < 1e-6) {
                out.push(q);
            }
        }
    }
    out
}

/// Which axes of symmetry (through the centre of the bounding box, parallel to the sketch axes) an outline has:
/// `(centre, mirrored across the vertical axis, mirrored across the horizontal axis)`. Works for rectangles,
/// circles, rings, rectangles with holes, ... (the mirror image of every vertex must lie on the outline).
pub fn symmetry_axes(outline: &[(DVec2, DVec2)]) -> Option<(DVec2, bool, bool)> {
    if outline.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for (a, b) in outline {
        lo = lo.min(*a).min(*b);
        hi = hi.max(*a).max(*b);
    }
    let c = (lo + hi) * 0.5;
    let tol = 1e-3 + 0.003 * (hi - lo).max_element();
    let near = |p: DVec2| {
        outline.iter().any(|(a, b)| {
            let d = *b - *a;
            let l2 = d.length_squared();
            let t = if l2 < 1e-18 {
                0.0
            } else {
                ((p - *a).dot(d) / l2).clamp(0.0, 1.0)
            };
            (*a + d * t).distance(p) <= tol
        })
    };
    let pts: Vec<DVec2> = outline.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let fx = pts.iter().all(|p| near(DVec2::new(2.0 * c.x - p.x, p.y)));
    let fy = pts.iter().all(|p| near(DVec2::new(p.x, 2.0 * c.y - p.y)));
    (fx || fy).then_some((c, fx, fy))
}

/// Mirrored copies of `points` over the symmetry axes that exist (`fx`: vertical axis, `fy`: horizontal), and over both.
pub fn mirrored_over(points: &[DVec2], c: DVec2, fx: bool, fy: bool) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::new();
    for p in points {
        let mut cands = Vec::new();
        if fx {
            cands.push(DVec2::new(2.0 * c.x - p.x, p.y));
        }
        if fy {
            cands.push(DVec2::new(p.x, 2.0 * c.y - p.y));
        }
        if fx && fy {
            cands.push(DVec2::new(2.0 * c.x - p.x, 2.0 * c.y - p.y));
        }
        for q in cands {
            if q.distance(*p) > 1e-6 && !out.iter().any(|o| o.distance(q) < 1e-6) {
                out.push(q);
            }
        }
    }
    out
}

#[allow(dead_code)]
/// If the segments form an axis-aligned rectangle, its centre.
pub fn rectangle_centre(segments: &[(DVec2, DVec2)]) -> Option<DVec2> {
    if segments.len() != 4 {
        return None;
    }
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for (a, b) in segments {
        let d = *b - *a;
        if d.x.abs() > 1e-6 && d.y.abs() > 1e-6 {
            return None;
        }
        lo = lo.min(*a).min(*b);
        hi = hi.max(*a).max(*b);
    }
    let on_edge = |p: DVec2| {
        (p.x - lo.x).abs() < 1e-6
            || (p.x - hi.x).abs() < 1e-6
            || (p.y - lo.y).abs() < 1e-6
            || (p.y - hi.y).abs() < 1e-6
    };
    segments
        .iter()
        .all(|(a, b)| on_edge(*a) && on_edge(*b))
        .then(|| (lo + hi) * 0.5)
}

fn dotted(from: DVec2, to: DVec2) -> Guide {
    (from, to)
}

/// The best guided position for the cursor, if any aid is in reach.
pub fn guided_snap(inp: &GuideInput) -> Option<GuideSnap> {
    let r_pt = 12.0 * inp.mm_per_px;
    let r_line = 6.0 * inp.mm_per_px;
    let raw = inp.raw;
    // helper points (mirrored copies)
    if let Some(h) = inp
        .helpers
        .iter()
        .filter(|h| h.distance(raw) <= r_pt)
        .min_by(|a, b| a.distance(raw).total_cmp(&b.distance(raw)))
    {
        return Some(GuideSnap {
            pos: *h,
            label: SnapLabel::Symmetric,
            guides: Vec::new(),
        });
    }
    // crossings of the curves
    let mut best: Option<(f64, DVec2)> = None;
    for (i, a) in inp.segments.iter().enumerate() {
        for b in &inp.segments[i + 1..] {
            if let Some(p) = segment_intersection(*a, *b) {
                let d = p.distance(raw);
                if d <= r_pt && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, p));
                }
            }
        }
    }
    if let Some((_, p)) = best {
        return Some(GuideSnap {
            pos: p,
            label: SnapLabel::Intersection,
            guides: Vec::new(),
        });
    }
    // alignment with reference points
    let near = |f: &dyn Fn(&DVec2) -> f64| -> Option<DVec2> {
        inp.refs
            .iter()
            .filter(|p| f(p).abs() <= r_line)
            .min_by(|a, b| f(a).abs().total_cmp(&f(b).abs()))
            .copied()
    };
    let vx = near(&|p| raw.x - p.x); // vertical guide through a reference
    let hy = near(&|p| raw.y - p.y); // horizontal guide
                                     // extension of a segment (beyond its end)
    let mut ext: Option<(f64, (DVec2, DVec2), DVec2)> = None;
    for s in inp.segments {
        let d = s.1 - s.0;
        let l2 = d.length_squared();
        if l2 < 1e-12 {
            continue;
        }
        let t = (raw - s.0).dot(d) / l2;
        if (0.0..=1.0).contains(&t) {
            continue; // on the piece itself, not an extension
        }
        let foot = s.0 + d * t;
        let dist = foot.distance(raw);
        if dist <= r_line && ext.is_none_or(|(bd, _, _)| dist < bd) {
            let end = if t < 0.0 { s.0 } else { s.1 };
            ext = Some((dist, *s, end));
        }
    }
    // combinations first: a crossing of two guides is a snap point
    if let (Some(v), Some(h)) = (vx, hy) {
        let p = DVec2::new(v.x, h.y);
        return Some(GuideSnap {
            pos: p,
            label: SnapLabel::Alignment,
            guides: vec![dotted(v, p), dotted(h, p)],
        });
    }
    if let Some((_, s, end)) = ext {
        if let Some(v) = vx {
            if let Some(p) = line_intersection(s, (DVec2::new(v.x, 0.0), DVec2::new(v.x, 1.0))) {
                return Some(GuideSnap {
                    pos: p,
                    label: SnapLabel::Alignment,
                    guides: vec![dotted(end, p), dotted(v, p)],
                });
            }
        }
        if let Some(h) = hy {
            if let Some(p) = line_intersection(s, (DVec2::new(0.0, h.y), DVec2::new(1.0, h.y))) {
                return Some(GuideSnap {
                    pos: p,
                    label: SnapLabel::Alignment,
                    guides: vec![dotted(end, p), dotted(h, p)],
                });
            }
        }
        // two extensions crossing
        for o in inp.segments {
            if *o == s {
                continue;
            }
            if let Some(p) = line_intersection(s, *o) {
                if p.distance(raw) <= r_pt {
                    let end2 = if (p - o.0).length() < (p - o.1).length() {
                        o.0
                    } else {
                        o.1
                    };
                    return Some(GuideSnap {
                        pos: p,
                        label: SnapLabel::Extension,
                        guides: vec![dotted(end, p), dotted(end2, p)],
                    });
                }
            }
        }
    }
    // parallel / perpendicular to an existing piece while drawing a line
    if let Some(start) = inp.line_start {
        let v = raw - start;
        let len = v.length();
        if len > 3.0 * inp.mm_per_px {
            let tol = 2.5_f64.to_radians();
            for s in inp.segments {
                let d = (s.1 - s.0).normalize_or_zero();
                if d == DVec2::ZERO {
                    continue;
                }
                for (dir, label) in [
                    (d, SnapLabel::Parallel),
                    (DVec2::new(-d.y, d.x), SnapLabel::Perpendicular),
                ] {
                    for sign in [1.0, -1.0] {
                        let dd = dir * sign;
                        let ang = v.normalize().dot(dd).clamp(-1.0, 1.0).acos();
                        if ang < tol {
                            let p = start + dd * len;
                            let mid = (s.0 + s.1) * 0.5;
                            return Some(GuideSnap {
                                pos: p,
                                label,
                                guides: vec![dotted(mid, mid + dd * 0.0), dotted(start, p)],
                            });
                        }
                    }
                }
            }
        }
    }
    if let Some((_, s, end)) = ext {
        let d = s.1 - s.0;
        let foot = s.0 + d * ((raw - s.0).dot(d) / d.length_squared());
        return Some(GuideSnap {
            pos: foot,
            label: SnapLabel::Extension,
            guides: vec![dotted(end, foot)],
        });
    }
    if let Some(v) = vx {
        let p = DVec2::new(v.x, raw.y);
        return Some(GuideSnap {
            pos: p,
            label: SnapLabel::Alignment,
            guides: vec![dotted(v, p)],
        });
    }
    if let Some(h) = hy {
        let p = DVec2::new(raw.x, h.y);
        return Some(GuideSnap {
            pos: p,
            label: SnapLabel::Alignment,
            guides: vec![dotted(h, p)],
        });
    }
    None
}

/// Directions in which a curve ending at `p` would continue tangentially (lines and arcs of `ents`).
pub fn tangent_dirs(p: DVec2, ents: &[rolidium_doc::rolidium_sketch::Entity]) -> Vec<DVec2> {
    use rolidium_doc::rolidium_sketch::{Entity, Segment};
    let mut out = Vec::new();
    let eps = 1e-6;
    let line = |a: DVec2, b: DVec2| -> Option<DVec2> {
        if a.distance(p) < eps {
            Some((a - b).normalize_or_zero())
        } else if b.distance(p) < eps {
            Some((b - a).normalize_or_zero())
        } else {
            None
        }
    };
    for e in ents {
        match *e {
            Entity::Line { a, b } => out.extend(line(a, b)),
            Entity::Rect { .. } | Entity::Circle { .. } | Entity::Axis { .. } => {}
            Entity::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let seg = Segment::Arc {
                    center,
                    radius,
                    start,
                    sweep,
                };
                let (s, t) = (seg.start_point(), seg.end_point());
                let dir = |from: DVec2| {
                    let r = (from - center).normalize_or_zero();
                    let ccw = DVec2::new(-r.y, r.x);
                    if sweep > 0.0 {
                        ccw
                    } else {
                        -ccw
                    }
                };
                if t.distance(p) < eps {
                    out.push(dir(t)); // leaving the end of the arc, continuing its travel
                } else if s.distance(p) < eps {
                    out.push(-dir(s));
                }
            }
        }
    }
    out.into_iter().filter(|d| *d != DVec2::ZERO).collect()
}

/// The point on the arc that leaves `p` along `t` and ends at `q` (the "point on the arc" of a three-point
/// arc), or `None` if `q` lies on the tangent line (a straight line is needed).
pub fn tangent_arc_mid(p: DVec2, t: DVec2, q: DVec2) -> Option<DVec2> {
    let n = DVec2::new(-t.y, t.x);
    let den = 2.0 * n.dot(q - p);
    if den.abs() < 1e-9 {
        return None;
    }
    let k = (q - p).length_squared() / den;
    let c = p + n * k;
    let r = c.distance(p);
    let chord_mid = (p + q) * 0.5;
    let u = (chord_mid - c).normalize_or_zero();
    if u == DVec2::ZERO {
        return None;
    }
    // the arc bulges the way it leaves p: pick the circle point on that side of the chord
    let m1 = c + u * r;
    let m2 = c - u * r;
    let side = |m: DVec2| (m - p).dot(t);
    Some(if side(m1) >= side(m2) { m1 } else { m2 })
}

/// Circle through three points: centre (None if they are in a line).
pub fn circle_centre(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<[f64; 3]> {
    use glam::DVec3;
    let (a, b, c) = (DVec3::from(a), DVec3::from(b), DVec3::from(c));
    let ab = b - a;
    let ac = c - a;
    let n = ab.cross(ac);
    let n2 = n.length_squared();
    if n2 < 1e-12 {
        return None;
    }
    let ctr = a + (n.cross(ab) * ac.length_squared() + ac.cross(n) * ab.length_squared()) / (2.0 * n2);
    Some(ctr.to_array())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inp<'a>(raw: DVec2, refs: &'a [DVec2], segs: &'a [(DVec2, DVec2)]) -> GuideInput<'a> {
        GuideInput {
            raw,
            mm_per_px: 0.1,
            refs,
            segments: segs,
            line_start: None,
            helpers: &[],
        }
    }

    #[test]
    fn horizontal_and_vertical_alignment_meet_in_a_snap_point() {
        let refs = [DVec2::new(10.0, 2.0), DVec2::new(3.0, 20.0)];
        // near x = 3 and y = 2: both guides, the crossing is the snap point
        let s = guided_snap(&inp(DVec2::new(3.2, 2.3), &refs, &[])).unwrap();
        assert!((s.pos - DVec2::new(3.0, 2.0)).length() < 1e-9);
        assert_eq!(s.label, SnapLabel::Alignment);
        assert_eq!(s.guides.len(), 2);
        // only one guide: the other coordinate stays free
        let s = guided_snap(&inp(DVec2::new(3.2, 9.0), &refs, &[])).unwrap();
        assert!((s.pos - DVec2::new(3.0, 9.0)).length() < 1e-9);
        assert!(guided_snap(&inp(DVec2::new(5.0, 9.0), &refs, &[])).is_none());
    }

    #[test]
    fn extension_of_a_segment_and_crossing_of_curves() {
        let segs = [(DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0))];
        // beyond the end of the segment, on its line
        let s = guided_snap(&inp(DVec2::new(15.0, 0.3), &[], &segs)).unwrap();
        assert_eq!(s.label, SnapLabel::Extension);
        assert!((s.pos - DVec2::new(15.0, 0.0)).length() < 1e-9);
        // on the piece itself: no extension
        assert!(guided_snap(&inp(DVec2::new(5.0, 0.3), &[], &segs)).is_none());
        // crossing of two pieces
        let segs = [
            (DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0)),
            (DVec2::new(0.0, 10.0), DVec2::new(10.0, 0.0)),
        ];
        let s = guided_snap(&inp(DVec2::new(5.4, 4.8), &[], &segs)).unwrap();
        assert_eq!(s.label, SnapLabel::Intersection);
        assert!((s.pos - DVec2::new(5.0, 5.0)).length() < 1e-9);
        // two extensions meeting beyond their ends
        let segs = [
            (DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)),
            (DVec2::new(10.0, 5.0), DVec2::new(10.0, 9.0)),
        ];
        let s = guided_snap(&inp(DVec2::new(10.2, 0.2), &[], &segs)).unwrap();
        assert!((s.pos - DVec2::new(10.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn parallel_and_perpendicular_while_drawing_a_line() {
        let segs = [(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0))];
        let mut i = inp(DVec2::new(20.0, 10.3), &[], &segs);
        i.line_start = Some(DVec2::new(10.0, 0.0));
        let s = guided_snap(&i).unwrap();
        assert_eq!(s.label, SnapLabel::Parallel);
        let d = (s.pos - DVec2::new(10.0, 0.0)).normalize();
        assert!((d.dot(DVec2::new(1.0, 1.0).normalize()) - 1.0).abs() < 1e-9);
        let mut i = inp(DVec2::new(10.0 - 10.2, 10.1), &[], &segs);
        i.line_start = Some(DVec2::new(10.0, 0.0));
        i.raw = DVec2::new(0.0, 10.0) + DVec2::new(0.1, -0.05);
        let s = guided_snap(&i).unwrap();
        assert_eq!(s.label, SnapLabel::Perpendicular);
        assert!(((s.pos - DVec2::new(10.0, 0.0)).dot(DVec2::new(1.0, 1.0))).abs() < 1e-9);
        // far from any direction: nothing
        i.raw = DVec2::new(20.0, 3.0);
        assert!(guided_snap(&i).is_none() || guided_snap(&i).unwrap().label != SnapLabel::Parallel);
    }

    #[test]
    fn mirrored_points_of_a_rectangular_face() {
        let rect = [
            (DVec2::new(0.0, 0.0), DVec2::new(40.0, 0.0)),
            (DVec2::new(40.0, 0.0), DVec2::new(40.0, 30.0)),
            (DVec2::new(40.0, 30.0), DVec2::new(0.0, 30.0)),
            (DVec2::new(0.0, 30.0), DVec2::new(0.0, 0.0)),
        ];
        let c = rectangle_centre(&rect).unwrap();
        assert_eq!(c, DVec2::new(20.0, 15.0));
        let m = mirrored_points(&[DVec2::new(5.0, 5.0)], c);
        assert_eq!(m.len(), 3);
        for want in [
            DVec2::new(35.0, 5.0),
            DVec2::new(5.0, 25.0),
            DVec2::new(35.0, 25.0),
        ] {
            assert!(m.iter().any(|p| p.distance(want) < 1e-9), "{want:?}");
        }
        // a point on an axis has fewer distinct mirrors; a sloped outline is no rectangle
        assert!(mirrored_points(&[c], c).is_empty());
        let tri = [(DVec2::ZERO, DVec2::new(1.0, 1.0)); 4];
        assert!(rectangle_centre(&tri).is_none());
        // helper points can be snapped to
        let helpers = m.clone();
        let mut i = inp(DVec2::new(34.8, 5.2), &[], &[]);
        i.helpers = &helpers;
        let s = guided_snap(&i).unwrap();
        assert_eq!(s.label, SnapLabel::Symmetric);
        assert!((s.pos - DVec2::new(35.0, 5.0)).length() < 1e-9);
    }

    #[test]
    fn tangent_arc_continues_a_line() {
        use rolidium_doc::rolidium_sketch::Entity;
        let ents = [Entity::Line {
            a: DVec2::new(0.0, 0.0),
            b: DVec2::new(10.0, 0.0),
        }];
        let d = tangent_dirs(DVec2::new(10.0, 0.0), &ents);
        assert_eq!(d.len(), 1);
        assert!((d[0] - DVec2::X).length() < 1e-9);
        assert!(tangent_dirs(DVec2::new(5.0, 0.0), &ents).is_empty());
        // an arc from (10,0) leaving along +X and ending at (15,5): a quarter circle of radius 5
        let m = tangent_arc_mid(DVec2::new(10.0, 0.0), DVec2::X, DVec2::new(15.0, 5.0)).unwrap();
        assert!(
            (m - (DVec2::new(10.0, 5.0) + DVec2::new(5.0, -5.0).normalize() * 5.0)).length() < 1e-9,
            "{m:?}"
        );
        let e = Entity::arc_through(DVec2::new(10.0, 0.0), m, DVec2::new(15.0, 5.0)).unwrap();
        // the arc really is tangent: its start tangent is +X
        if let Entity::Arc { center, .. } = e {
            assert!(((DVec2::new(10.0, 0.0) - center).dot(DVec2::X)).abs() < 1e-9);
        }
        assert!(tangent_arc_mid(DVec2::ZERO, DVec2::X, DVec2::new(5.0, 0.0)).is_none());
        // the end of an arc continues along its travel
        let arc =
            Entity::arc_through(DVec2::new(5.0, 0.0), DVec2::new(0.0, 5.0), DVec2::new(-5.0, 0.0)).unwrap();
        let d = tangent_dirs(DVec2::new(-5.0, 0.0), &[arc]);
        assert!((d[0] - DVec2::new(0.0, -1.0)).length() < 1e-9, "{d:?}");
    }

    #[test]
    fn symmetry_of_circles_rings_and_rectangles_with_holes() {
        use std::f64::consts::TAU;
        let circle = |c: DVec2, r: f64, n: usize| -> Vec<(DVec2, DVec2)> {
            (0..n)
                .map(|i| {
                    let (a, b) = (
                        i as f64 / n as f64 * TAU + 0.3,
                        (i + 1) as f64 / n as f64 * TAU + 0.3,
                    );
                    (
                        c + DVec2::new(a.cos(), a.sin()) * r,
                        c + DVec2::new(b.cos(), b.sin()) * r,
                    )
                })
                .collect()
        };
        // a disc polygon sampled with an arbitrary phase is still symmetric within the tolerance
        let (c, fx, fy) = symmetry_axes(&circle(DVec2::new(5.0, 3.0), 20.0, 96)).unwrap();
        assert!(c.distance(DVec2::new(5.0, 3.0)) < 0.2 && fx && fy);
        // ring: two concentric circles
        let mut ring = circle(DVec2::ZERO, 20.0, 96);
        ring.extend(circle(DVec2::ZERO, 12.0, 64));
        assert!(matches!(symmetry_axes(&ring), Some((_, true, true))));
        // rectangle with a hole in the middle
        let mut r = vec![
            (DVec2::new(0.0, 0.0), DVec2::new(40.0, 0.0)),
            (DVec2::new(40.0, 0.0), DVec2::new(40.0, 30.0)),
            (DVec2::new(40.0, 30.0), DVec2::new(0.0, 30.0)),
            (DVec2::new(0.0, 30.0), DVec2::new(0.0, 0.0)),
        ];
        r.extend(circle(DVec2::new(20.0, 15.0), 5.0, 48));
        assert!(matches!(symmetry_axes(&r), Some((_, true, true))));
        // a hole off to one side keeps only... the vertical axis is lost, the outer rectangle alone has both
        let mut r2 = r[..4].to_vec();
        r2.extend(circle(DVec2::new(10.0, 15.0), 5.0, 48));
        let (_, fx, fy) = symmetry_axes(&r2).unwrap();
        assert!(!fx && fy);
        // an L shape has no symmetry
        let l = [
            (DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0)),
            (DVec2::new(10.0, 0.0), DVec2::new(10.0, 4.0)),
            (DVec2::new(10.0, 4.0), DVec2::new(4.0, 4.0)),
            (DVec2::new(4.0, 4.0), DVec2::new(4.0, 10.0)),
            (DVec2::new(4.0, 10.0), DVec2::new(0.0, 10.0)),
            (DVec2::new(0.0, 10.0), DVec2::new(0.0, 0.0)),
        ];
        assert!(symmetry_axes(&l).is_none());
        let m = mirrored_over(&[DVec2::new(30.0, 20.0)], DVec2::new(20.0, 15.0), true, false);
        assert_eq!(m, vec![DVec2::new(10.0, 20.0)]);
    }

    #[test]
    fn circle_centre_from_three_points() {
        let c = circle_centre([10.0, 0.0, 3.0], [0.0, 10.0, 3.0], [-10.0, 0.0, 3.0]).unwrap();
        assert!((c[0]).abs() < 1e-9 && (c[1]).abs() < 1e-9 && (c[2] - 3.0).abs() < 1e-9);
        assert!(circle_centre([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]).is_none());
    }
}
