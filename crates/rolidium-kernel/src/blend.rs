// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Rounds and chamfers: our own chain of smoothly continuing edges and the check of the result.
//!
//! Open CASCADE follows only edges whose neighbouring faces continue tangentially within a tight tolerance. The edges of
//! a loft are split into several pieces and their faces are different surfaces, so the kernel does not see them as one
//! chain and a round of one piece ends in a damaged surface. [`Solid::smooth_chain`] collects the whole smooth chain
//! with a tolerance of our own, and [`Solid::check_blend`] refuses a result that is damaged.

use crate::{nat as native, DVec3, EdgeId, KernelError, Result, Solid};
use std::collections::HashMap;

/// The largest size below `size` (a failing size of a round or chamfer) for which `works` still succeeds, found by halving the
/// interval between a size that works and `size`. `None` when even a tiny size fails (the edges cannot be treated at any
/// size). The result is rounded down (to 0.1 mm, or to 0.01 mm below 1 mm), so it is a size that works and one a person can type.
pub fn largest_working_size(size: f64, mut works: impl FnMut(f64) -> bool) -> Option<f64> {
    if !(size.is_finite() && size > 0.0) {
        return None;
    }
    // look downwards for a size that works (halving each time, down to 1/128 of the failing size or 0.1 mm), then narrow the gap
    let (mut lo, mut hi) = (0.0, size);
    let mut v = size;
    for _ in 0..7 {
        v *= 0.5;
        if v < 0.1 {
            break;
        }
        if works(v) {
            lo = v;
            break;
        }
        hi = v;
    }
    if lo == 0.0 {
        return None;
    }
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        if works(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let step = if lo >= 1.0 { 0.1 } else { 0.01 };
    let nice = (lo / step).floor() * step;
    // the rounded-down value was never tried itself; it lies below a size that worked, so it works as well
    Some(if nice > 0.0 { nice } else { lo })
}

/// Angle (degrees) up to which two edges meeting at a vertex count as one smooth chain.
pub const CHAIN_ANGLE_DEG: f64 = 20.0;

/// Start point, start tangent, end point and end tangent of every edge, which faces every edge belongs to,
/// and the vertices by position.
pub(crate) struct ChainIndex {
    ends: Vec<[DVec3; 4]>,
    faces_of: Vec<Vec<u32>>,
    by_cell: HashMap<[i64; 3], Vec<(u32, u8)>>,
}

const CELL: f64 = 1e-3;

fn cell(p: DVec3) -> [i64; 3] {
    [
        (p.x / CELL).floor() as i64,
        (p.y / CELL).floor() as i64,
        (p.z / CELL).floor() as i64,
    ]
}

impl Solid {
    fn chain_index(&self) -> &ChainIndex {
        self.chain_index.get_or_init(|| {
            let n = self.edge_count();
            let mut ends = vec![[DVec3::ZERO; 4]; n];
            let mut by_cell: HashMap<[i64; 3], Vec<(u32, u8)>> = HashMap::new();
            for (i, slot) in ends.iter_mut().enumerate() {
                if let Ok(v) = native::edge_ends(self.raw(), i as u32) {
                    if v.len() == 12 {
                        let e = [
                            DVec3::new(v[0], v[1], v[2]),
                            DVec3::new(v[3], v[4], v[5]),
                            DVec3::new(v[6], v[7], v[8]),
                            DVec3::new(v[9], v[10], v[11]),
                        ];
                        *slot = e;
                        by_cell.entry(cell(e[0])).or_default().push((i as u32, 0));
                        by_cell.entry(cell(e[2])).or_default().push((i as u32, 1));
                    }
                }
            }
            let mut faces_of = vec![Vec::new(); n];
            for f in self.faces() {
                if let Ok(es) = self.face_edges(f) {
                    for e in es {
                        if let Some(v) = faces_of.get_mut(e.0 as usize) {
                            v.push(f.0);
                        }
                    }
                }
            }
            ChainIndex {
                ends,
                faces_of,
                by_cell,
            }
        })
    }

    /// The edges of `seed` plus every edge that continues one of them smoothly (the tangents at the shared vertex differ
    /// by less than `angle_deg`) and lies on one of the same faces. Edges are compared by position, so the pieces of a
    /// split outline are found even when the kernel does not consider them tangent.
    pub fn smooth_chain(&self, seed: &[EdgeId], angle_deg: f64) -> Vec<EdgeId> {
        let ix = self.chain_index();
        let limit = (180.0 - angle_deg).to_radians().cos(); // the two outgoing directions must be nearly opposite
        let mut chain: Vec<u32> = Vec::new();
        let mut queue: Vec<u32> = Vec::new();
        for e in seed {
            if (e.0 as usize) < ix.ends.len() && !chain.contains(&e.0) {
                chain.push(e.0);
                queue.push(e.0);
            }
        }
        while let Some(e) = queue.pop() {
            let ends = ix.ends[e as usize];
            for end in 0..2usize {
                let p = ends[end * 2];
                // direction pointing from the vertex into this edge
                let into1 = if end == 0 { ends[1] } else { -ends[3] };
                let c = cell(p);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            let Some(list) = ix.by_cell.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else {
                                continue;
                            };
                            for &(f, fend) in list {
                                if f == e || chain.contains(&f) {
                                    continue;
                                }
                                let fe = ix.ends[f as usize];
                                if (fe[fend as usize * 2] - p).length() > 1e-4 {
                                    continue;
                                }
                                let into2 = if fend == 0 { fe[1] } else { -fe[3] };
                                // smooth: the two directions into the edges are opposite
                                if into1.dot(into2) > limit {
                                    continue;
                                }
                                let shares_face = ix.faces_of[e as usize]
                                    .iter()
                                    .any(|x| ix.faces_of[f as usize].contains(x));
                                if shares_face {
                                    chain.push(f);
                                    queue.push(f);
                                }
                            }
                        }
                    }
                }
            }
        }
        chain.sort_unstable();
        chain.into_iter().map(EdgeId).collect()
    }

    /// Refuses a damaged result of a round or chamfer of `size` mm along edges of `length` mm (in total, 0 when
    /// unknown): the solid must be valid, its faces must not cut through each other, the outer size must not grow
    /// and the volume must change by a plausible amount.
    pub(crate) fn check_blend(&self, after: &Solid, size: f64, length: f64, what: &str) -> Result<()> {
        // The reason (not valid, cuts through itself, sticks out, implausible volume) is not part of the message: it is a
        // fixed sentence that can be translated; the reasons are told apart in the tests.
        let bad = |_why: &str| {
            KernelError::Invalid(
                if what == "chamfer" {
                    "The chamfer could not be made cleanly, so the model was not changed. Try a smaller size or select the whole edge loop."
                } else {
                    "The round could not be made cleanly, so the model was not changed. Try a smaller size or select the whole edge loop."
                }
                .into(),
            )
        };
        if !after.is_valid() {
            return Err(bad("the result is not a valid solid"));
        }
        if native::has_self_intersections(after.raw()) {
            return Err(bad("its surfaces cut through each other"));
        }
        let (a, b) = (self.props()?, after.props()?);
        let diag = (a.max - a.min).length();
        let tol = 0.02 * size + 1e-4 * diag + 1e-3;
        let grew = (b.min.x < a.min.x - tol)
            || (b.min.y < a.min.y - tol)
            || (b.min.z < a.min.z - tol)
            || (b.max.x > a.max.x + tol)
            || (b.max.y > a.max.y + tol)
            || (b.max.z > a.max.z + tol);
        if grew {
            return Err(bad("the surface sticks out of the body"));
        }
        if b.volume.is_nan() || b.volume <= 0.0 {
            return Err(bad("the volume is not positive"));
        }
        if length > 0.0 {
            let limit = 12.0 * size * size * length + 1e-6 * a.volume + 1e-6;
            if (b.volume - a.volume).abs() > limit {
                return Err(bad("the volume changed by an implausible amount"));
            }
        }
        Ok(())
    }

    /// Total length of the edges (mm).
    pub(crate) fn edges_length(&self, edges: &[EdgeId]) -> f64 {
        edges
            .iter()
            .filter_map(|e| self.edge_info(*e).ok())
            .map(|i| i.length)
            .sum()
    }

    pub(crate) fn chain_map(&self, edges: &[(EdgeId, f64)]) -> Vec<(EdgeId, f64)> {
        // every edge of a chain gets the size of the selected edge it is connected to
        let mut out: Vec<(EdgeId, f64)> = Vec::new();
        for (e, size) in edges {
            for c in self.smooth_chain(&[*e], CHAIN_ANGLE_DEG) {
                if !out.iter().any(|(x, _)| *x == c) {
                    out.push((c, *size));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoftInput, Profile, ProfileSegment};

    fn rounded_rect(cx: f64, cy: f64, w: f64, h: f64, r: f64) -> Profile {
        let (hw, hh) = (w / 2.0 - r, h / 2.0 - r);
        let s = std::f64::consts::FRAC_1_SQRT_2 * r;
        let mut segs = Vec::new();
        let corner = |c: [f64; 2], a: [f64; 2], mid: [f64; 2], b: [f64; 2]| {
            let _ = c;
            ProfileSegment::Arc { a, mid, b }
        };
        segs.push(ProfileSegment::Line {
            a: [cx - hw, cy - hh - r],
            b: [cx + hw, cy - hh - r],
        });
        segs.push(corner(
            [cx + hw, cy - hh],
            [cx + hw, cy - hh - r],
            [cx + hw + s, cy - hh - s],
            [cx + hw + r, cy - hh],
        ));
        segs.push(ProfileSegment::Line {
            a: [cx + hw + r, cy - hh],
            b: [cx + hw + r, cy + hh],
        });
        segs.push(corner(
            [cx + hw, cy + hh],
            [cx + hw + r, cy + hh],
            [cx + hw + s, cy + hh + s],
            [cx + hw, cy + hh + r],
        ));
        segs.push(ProfileSegment::Line {
            a: [cx + hw, cy + hh + r],
            b: [cx - hw, cy + hh + r],
        });
        segs.push(corner(
            [cx - hw, cy + hh],
            [cx - hw, cy + hh + r],
            [cx - hw - s, cy + hh + s],
            [cx - hw - r, cy + hh],
        ));
        segs.push(ProfileSegment::Line {
            a: [cx - hw - r, cy + hh],
            b: [cx - hw - r, cy - hh],
        });
        segs.push(corner(
            [cx - hw, cy - hh],
            [cx - hw - r, cy - hh],
            [cx - hw - s, cy - hh - s],
            [cx - hw, cy - hh - r],
        ));
        Profile { loops: vec![segs] }
    }

    fn circle(r: f64) -> Profile {
        Profile {
            loops: vec![vec![ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: r,
            }]],
        }
    }

    fn at(p: &Profile, z: f64) -> LoftInput<'_> {
        LoftInput::Profile {
            profile: p,
            origin: DVec3::new(0.0, 0.0, z),
            normal: DVec3::Z,
            x_dir: DVec3::X,
        }
    }

    fn loft_circle_to_rounded_rect() -> Solid {
        let (c, rr) = (circle(15.0), rounded_rect(25.0, 0.0, 60.0, 40.0, 8.0));
        Solid::loft(&[at(&c, 0.0), at(&rr, 30.0)], false).unwrap()
    }

    fn rim(s: &Solid, z: f64) -> Vec<EdgeId> {
        s.edges()
            .filter(|e| {
                s.edge_info(*e)
                    .map(|i| (i.start.z - z).abs() < 1e-6 && (i.end.z - z).abs() < 1e-6)
                    .unwrap_or(false)
            })
            .collect()
    }

    #[test]
    fn the_rim_of_a_loft_is_found_as_one_smooth_chain_from_any_of_its_edges() {
        let s = loft_circle_to_rounded_rect();
        let top = rim(&s, 30.0);
        assert!(top.len() >= 8, "the rim is split into {} edges", top.len());
        for e in &top {
            let chain = s.smooth_chain(&[*e], CHAIN_ANGLE_DEG);
            assert_eq!(chain.len(), top.len(), "from edge {e:?}");
        }
        // the edge between the top face and nothing else must not leak onto the lower rim
        assert!(!rim(&s, 0.0)
            .iter()
            .any(|b| s.smooth_chain(&[top[0]], CHAIN_ANGLE_DEG).contains(b)));
    }

    #[test]
    fn rounding_one_rim_edge_gives_the_same_clean_result_as_rounding_all_of_them() {
        let s = loft_circle_to_rounded_rect();
        let top = rim(&s, 30.0);
        let one = s.fillet(&top[..1], 2.0).expect("one edge");
        let all = s.fillet(&top, 2.0).expect("all edges");
        assert!(one.solid.is_valid() && all.solid.is_valid());
        assert!((one.solid.volume().unwrap() - all.solid.volume().unwrap()).abs() < 1e-6);
        assert_eq!(one.solid.face_count(), all.solid.face_count());
        let (a, b) = (s.props().unwrap(), one.solid.props().unwrap());
        assert!(b.volume < a.volume && a.volume - b.volume < 2.0 * 2.0 * 200.0 * 12.0);
        assert!(b.max.z <= a.max.z + 1e-6 && b.max.x <= a.max.x + 1e-3);
    }

    #[test]
    fn chamfering_one_rim_edge_follows_the_chain_too() {
        let s = loft_circle_to_rounded_rect();
        let top = rim(&s, 30.0);
        let one = s.chamfer(&top[..1], 2.0).expect("symmetric");
        let all = s.chamfer(&top, 2.0).expect("all");
        assert!((one.solid.volume().unwrap() - all.solid.volume().unwrap()).abs() < 1e-6);
        let angled = s.chamfer_angled(&top[..1], 2.0, 30.0, false).expect("angled");
        assert!(angled.solid.is_valid());
    }

    #[test]
    fn the_inner_rim_of_a_lofted_ring_can_be_rounded() {
        // a tube whose wall is lofted between two rings: round the top edge of the bore
        let (outer_a, outer_b) = (circle(20.0), rounded_rect(10.0, 0.0, 50.0, 44.0, 10.0));
        let hole_a = circle(12.0);
        let hole_b = rounded_rect(10.0, 0.0, 30.0, 24.0, 6.0);
        let ring = |outer: &Profile, hole: &Profile| Profile {
            loops: vec![outer.loops[0].clone(), hole.loops[0].clone()],
        };
        let (ra, rb) = (ring(&outer_a, &hole_a), ring(&outer_b, &hole_b));
        let s = Solid::loft(&[at(&ra, 0.0), at(&rb, 30.0)], false).expect("ring loft");
        let top = rim(&s, 30.0);
        // the inner rim edges are the ones nearer the axis of the hole
        let inner: Vec<EdgeId> = top
            .iter()
            .copied()
            .filter(|e| {
                let i = s.edge_info(*e).unwrap();
                ((i.start + i.end) * 0.5 - DVec3::new(10.0, 0.0, 30.0)).length() < 17.0
            })
            .collect();
        assert!(!inner.is_empty());
        let r = s.fillet(&inner[..1], 1.5).expect("inner rim");
        assert!(r.solid.is_valid());
        assert!(r.solid.volume().unwrap() < s.volume().unwrap());
    }

    #[test]
    fn a_damaged_result_is_refused_with_a_clear_message() {
        let a = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
        let bigger = Solid::cuboid(DVec3::ZERO, DVec3::new(60.0, 30.0, 20.0)).unwrap();
        let err = a
            .check_blend(&bigger, 2.0, 40.0, "round")
            .unwrap_err()
            .to_string();
        assert!(err.contains("could not be made cleanly"), "{err}");
        let much_smaller = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 5.0)).unwrap();
        let err = a
            .check_blend(&much_smaller, 2.0, 40.0, "round")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("could not be made cleanly") && err.contains("round"),
            "{err}"
        );
        let err = a
            .check_blend(&much_smaller, 2.0, 40.0, "chamfer")
            .unwrap_err()
            .to_string();
        assert!(err.contains("chamfer"), "{err}");
        // an honest round passes
        let ok = a.fillet(&[a.edges().next().unwrap()], 2.0).unwrap();
        assert!(a.check_blend(&ok.solid, 2.0, 40.0, "round").is_ok());
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    #[test]
    fn checking_a_simple_round_is_fast() {
        let a = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
        let e = a.edges().next().unwrap();
        let t = std::time::Instant::now();
        let r = a.fillet(&[e], 2.0).unwrap();
        let total = t.elapsed();
        let t2 = std::time::Instant::now();
        a.check_blend(&r.solid, 2.0, 40.0, "round").unwrap();
        let check = t2.elapsed();
        eprintln!("fillet with check {total:?}, check alone {check:?}");
        assert!(check.as_millis() < 300, "the check takes {check:?}");
    }
}
