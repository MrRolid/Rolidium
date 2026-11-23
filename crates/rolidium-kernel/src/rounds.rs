// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Rounds and chamfers recognised from the geometry of a solid (not from the history of operations, so it also works for a
//! body read from a STEP file), and the operations that keep them: changing their size and moving a face next to them.
//!
//! A *round* is a cylindrical or toroidal face of constant radius that continues two neighbouring faces tangentially. The
//! transition faces where several rounds meet at a corner (a sphere, or a freeform patch whose whole boundary continues
//! rounds tangentially) belong to the same *component*. A *chamfer* is a narrow planar strip that meets both neighbours at an
//! angle between 10 and 80 degrees on the same side (both edges convex or both concave); a strip that is not clearly narrower
//! than its neighbours (a side of a hexagonal prism) is not taken for a chamfer. Chamfers are therefore recognised by a
//! heuristic; rounds are exact.
//!
//! To change anything next to a round the whole component is removed (Open CASCADE defeaturing), the sharp solid is
//! changed and every round is made again on the sharp edges it came from, with its own size. The edges are found again by
//! the pair of faces the round separated.

use crate::{nat as native, DVec3, EdgeId, FaceId, FaceKind, KernelError, OpResult, Result, Solid};
use std::collections::HashMap;

/// Angle (rad) under which two faces count as tangent.
const TANGENT: f64 = 0.035;
/// A chamfer strip meets its neighbours at least at 10 and at most at 80 degrees.
const CHAMFER_MIN: f64 = 0.1745;
const CHAMFER_MAX: f64 = 1.396;

/// One round or chamfer as the user sees it: faces that belong to one edge (or a chain of edges that continue each other)
/// and have one size.
#[derive(Debug, Clone, PartialEq)]
pub struct Rounding {
    pub chamfer: bool,
    /// Radius of a round; the distance on the reference face (`pairs[..].0`) of a chamfer.
    pub size: f64,
    /// Chamfer: angle (degrees) whose tangent is the other distance divided by `size` (45: symmetric); 45 for a round.
    pub angle_deg: f64,
    /// The faces of the round itself (cylinders, tori) or the strips of the chamfer.
    pub faces: Vec<FaceId>,
    /// Corner faces next to the round.
    pub corners: Vec<FaceId>,
    /// For every face of the round the two sharp faces it separates (the reference face of a chamfer first), when they are known.
    pub pairs: Vec<(FaceId, FaceId)>,
    /// Chamfer: both distances for every pair (on the first and on the second face).
    pub legs: Vec<(f64, f64)>,
}

/// Rounds and chamfers that touch each other (through corner faces or directly).
#[derive(Debug, Clone, PartialEq)]
pub struct RoundComponent {
    /// Every face of the component, corner faces included.
    pub faces: Vec<FaceId>,
    pub roundings: Vec<Rounding>,
}

#[derive(Debug, Default)]
pub struct BlendMap {
    pub components: Vec<RoundComponent>,
    /// Neighbours of every face (through an edge shared with exactly one other face).
    adj: Vec<Vec<u32>>,
}

impl BlendMap {
    /// The component a face belongs to (as a round, a chamfer or a corner).
    pub fn component_of(&self, face: FaceId) -> Option<usize> {
        self.components.iter().position(|c| c.faces.contains(&face))
    }

    /// Components that have a face next to `face`.
    pub fn touching(&self, face: FaceId) -> Vec<usize> {
        let Some(n) = self.adj.get(face.0 as usize) else {
            return Vec::new();
        };
        (0..self.components.len())
            .filter(|i| {
                !self.components[*i].faces.contains(&face)
                    && n.iter().any(|f| self.components[*i].faces.contains(&FaceId(*f)))
            })
            .collect()
    }

    /// The rounding a face belongs to: as one of its faces or, for a corner face, the first one next to it.
    pub fn rounding_of(&self, face: FaceId) -> Option<(usize, usize)> {
        let ci = self.component_of(face)?;
        let c = &self.components[ci];
        if let Some(ri) = c.roundings.iter().position(|r| r.faces.contains(&face)) {
            return Some((ci, ri));
        }
        c.roundings
            .iter()
            .position(|r| r.corners.contains(&face))
            .map(|ri| (ci, ri))
    }
}

/// An edge of a face: the edge, the face on the other side, the angle between the normals and the convexity.
type Neighbour = (usize, u32, f64, f64);
/// What to make again: the rounding, its size, and for a chamfer the two distances and whether the reference is the second face.
type ChamferSize = Option<(f64, f64, bool)>;
type Item<'a> = (&'a Rounding, f64, ChamferSize);

struct Scan {
    kind: Vec<u8>,
    radius: Vec<f64>,
    /// per edge: faces, angle between the normals, convexity
    edges: Vec<(i64, i64, f64, f64)>,
}

fn same_size(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-3 * a.abs().max(b.abs()) + 1e-6
}

/// Sorts two faces in a fixed geometric order (the same as the chamfer with two distances uses for its reference face).
fn ordered(s: &Solid, a: FaceId, b: FaceId) -> (FaceId, FaceId) {
    let key = |f: &FaceId| {
        let i = s
            .face_info(*f)
            .map(|i| (i.normal, i.origin))
            .unwrap_or((DVec3::ZERO, DVec3::ZERO));
        [i.0.x, i.0.y, i.0.z, i.1.x, i.1.y, i.1.z].map(|v| (v * 1e6).round() as i64)
    };
    if key(&a) <= key(&b) {
        (a, b)
    } else {
        (b, a)
    }
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

impl Solid {
    /// The rounds and chamfers of this solid (built when first needed).
    pub fn blend_map(&self) -> &BlendMap {
        self.blend_map
            .get_or_init(|| self.build_blend_map().unwrap_or_default())
    }

    fn build_blend_map(&self) -> Result<BlendMap> {
        let raw = native::blend_scan(self.raw())?;
        if raw.len() < 2 {
            return Ok(BlendMap::default());
        }
        let (nf, ne) = (raw[0] as usize, raw[1] as usize);
        let mut sc = Scan {
            kind: Vec::with_capacity(nf),
            radius: Vec::with_capacity(nf),
            edges: Vec::with_capacity(ne),
        };
        for i in 0..nf {
            sc.kind.push(raw[2 + 2 * i] as u8);
            sc.radius.push(raw[3 + 2 * i]);
        }
        let base = 2 + 2 * nf;
        for i in 0..ne {
            let o = base + 4 * i;
            sc.edges
                .push((raw[o] as i64, raw[o + 1] as i64, raw[o + 2], raw[o + 3]));
        }
        // neighbours of every face: (edge, other face, angle, convexity)
        let mut nb: Vec<Vec<Neighbour>> = vec![Vec::new(); nf];
        for (e, (a, b, ang, cv)) in sc.edges.iter().enumerate() {
            if *a >= 0 && *b >= 0 && a != b {
                nb[*a as usize].push((e, *b as u32, *ang, *cv));
                nb[*b as usize].push((e, *a as u32, *ang, *cv));
            }
        }
        let adj: Vec<Vec<u32>> = nb.iter().map(|v| v.iter().map(|x| x.1).collect()).collect();
        // rounds: constant radius, tangent to two faces that are not parallel
        let mut role = vec![0u8; nf]; // 0 nothing, 1 round, 2 round corner, 3 chamfer strip, 4 chamfer corner
        for f in 0..nf {
            if !matches!(sc.kind[f], 1 | 4) || sc.radius[f] <= 0.0 {
                continue;
            }
            let tangent: Vec<u32> = nb[f].iter().filter(|x| x.2 < TANGENT).map(|x| x.1).collect();
            if tangent.len() < 2 {
                continue;
            }
            // two planes that are parallel (the end of a slot) are not a round of an edge
            let planes: Vec<u32> = tangent
                .iter()
                .copied()
                .filter(|g| sc.kind[*g as usize] == 0)
                .collect();
            if planes.len() == 2 {
                if let (Ok(a), Ok(b)) = (
                    self.face_info(FaceId(planes[0])),
                    self.face_info(FaceId(planes[1])),
                ) {
                    if a.normal.dot(b.normal).abs() > 0.996 {
                        continue;
                    }
                }
            }
            role[f] = 1;
        }
        // corner faces: not planar, not a round, every edge continues a round or another corner face tangentially
        let mut cand: Vec<bool> = (0..nf)
            .map(|f| {
                role[f] == 0
                    && sc.kind[f] != 0
                    && nb[f].len() >= 3
                    && nb[f].iter().all(|x| x.2 < TANGENT)
                    && nb[f].iter().any(|x| role[x.1 as usize] == 1)
            })
            .collect();
        loop {
            let mut changed = false;
            for f in 0..nf {
                if cand[f]
                    && !nb[f]
                        .iter()
                        .all(|x| role[x.1 as usize] == 1 || cand[x.1 as usize])
                {
                    cand[f] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for f in 0..nf {
            if cand[f] {
                role[f] = 2;
            }
        }
        // chamfer strips
        let mut legs_of: HashMap<usize, (FaceId, FaceId, f64, f64, f64)> = HashMap::new(); // face -> (a, b, da, db, angle)
        let mut width_of: HashMap<usize, f64> = HashMap::new();
        for f in 0..nf {
            if role[f] != 0 || sc.kind[f] != 0 {
                continue;
            }
            let good: Vec<&Neighbour> = nb[f]
                .iter()
                .filter(|x| x.2 > CHAMFER_MIN && x.2 < CHAMFER_MAX && x.3 != 0.0)
                .collect();
            if good.len() < 2 || nb[f].iter().any(|x| x.2 < TANGENT) {
                continue;
            }
            let sign = good[0].3;
            if good.iter().any(|x| x.3 != sign) {
                continue;
            }
            let Ok(fi) = self.face_info(FaceId(f as u32)) else {
                continue;
            };
            let mut long: Vec<(f64, &Neighbour)> = good
                .iter()
                .filter_map(|x| self.edge_info(EdgeId(x.0 as u32)).ok().map(|i| (i.length, *x)))
                .collect();
            long.sort_by(|a, b| b.0.total_cmp(&a.0));
            if long.len() < 2 || long[1].0 < 0.5 * long[0].0 {
                continue;
            }
            let (l1, e1) = long[0];
            let (l2, e2) = long[1];
            // a third edge as long as these two makes a corner face (a triangle) rather than a strip
            if long.len() > 2 && long[2].0 > 0.6 * l2 {
                continue;
            }
            if e1.1 == e2.1 || e1.2 + e2.2 < 0.35 || e1.2 + e2.2 > 2.8 {
                continue;
            }
            let w = fi.area / (0.5 * (l1 + l2));
            // clearly narrower than both neighbours
            let narrow = [(l1, e1), (l2, e2)].iter().all(|(l, e)| {
                self.face_info(FaceId(e.1))
                    .map(|n| n.area / l.max(1e-9) > w / 0.75)
                    .unwrap_or(false)
            });
            if !narrow {
                continue;
            }
            let (a, b) = ordered(self, FaceId(e1.1), FaceId(e2.1));
            let (al_a, al_b) = if a.0 == e1.1 { (e1.2, e2.2) } else { (e2.2, e1.2) };
            let gamma = std::f64::consts::PI - al_a - al_b;
            if gamma < 0.1 {
                continue;
            }
            let da = w * al_b.sin() / gamma.sin();
            let db = w * al_a.sin() / gamma.sin();
            role[f] = 3;
            width_of.insert(f, w);
            let r9 = |v: f64| (v * 1e9).round() / 1e9;
            legs_of.insert(f, (a, b, r9(da), r9(db), r9((db / da).atan().to_degrees())));
        }
        // chamfer corner: a small plane whose every neighbour is a chamfer strip (a big face with chamfers all round is not one)
        for f in 0..nf {
            if role[f] == 0
                && sc.kind[f] == 0
                && nb[f].len() >= 3
                && nb[f].iter().all(|x| role[x.1 as usize] == 3)
            {
                let wmax = nb[f]
                    .iter()
                    .filter_map(|x| width_of.get(&(x.1 as usize)))
                    .fold(0.0f64, |a, b| a.max(*b));
                let longest = nb[f]
                    .iter()
                    .filter_map(|x| self.edge_info(EdgeId(x.0 as u32)).ok())
                    .fold(0.0f64, |a, i| a.max(i.length));
                if longest <= 2.5 * wmax {
                    role[f] = 4;
                }
            }
        }
        // components
        let mut parent: Vec<usize> = (0..nf).collect();
        for (a, b, _ang, _) in &sc.edges {
            if *a < 0 || *b < 0 {
                continue;
            }
            let (a, b) = (*a as usize, *b as usize);
            let round_a = matches!(role[a], 1 | 2);
            let round_b = matches!(role[b], 1 | 2);
            let cham_a = matches!(role[a], 3 | 4);
            let cham_b = matches!(role[b], 3 | 4);
            if (round_a && round_b) || (cham_a && cham_b) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                parent[ra] = rb;
            }
        }
        // roundings: directly neighbouring faces of one size (rounds across a tangent edge, strips across any edge)
        let mut rparent: Vec<usize> = (0..nf).collect();
        let size_of = |f: usize| -> f64 {
            if role[f] == 1 {
                sc.radius[f]
            } else {
                legs_of.get(&f).map(|l| l.2).unwrap_or(0.0)
            }
        };
        for (a, b, ang, _) in &sc.edges {
            if *a < 0 || *b < 0 {
                continue;
            }
            let (a, b) = (*a as usize, *b as usize);
            let both_round = role[a] == 1 && role[b] == 1 && *ang < TANGENT;
            let both_chamfer = role[a] == 3 && role[b] == 3;
            if (both_round || both_chamfer) && same_size(size_of(a), size_of(b)) {
                let (ra, rb) = (find(&mut rparent, a), find(&mut rparent, b));
                rparent[ra] = rb;
            }
        }
        let mut comps: Vec<(usize, RoundComponent)> = Vec::new();
        for (f, _) in role.iter().enumerate().filter(|(_, r)| **r != 0) {
            let root = find(&mut parent, f);
            let ci = match comps.iter().position(|c| c.0 == root) {
                Some(i) => i,
                None => {
                    comps.push((
                        root,
                        RoundComponent {
                            faces: Vec::new(),
                            roundings: Vec::new(),
                        },
                    ));
                    comps.len() - 1
                }
            };
            comps[ci].1.faces.push(FaceId(f as u32));
        }
        let comp_of: HashMap<u32, usize> = comps
            .iter()
            .enumerate()
            .flat_map(|(i, c)| c.1.faces.iter().map(move |f| (f.0, i)))
            .collect();
        for (ci, (_, comp)) in comps.iter_mut().enumerate() {
            let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
            for f in comp.faces.iter().map(|f| f.0 as usize) {
                if matches!(role[f], 1 | 3) {
                    let root = find(&mut rparent, f);
                    match groups.iter_mut().find(|g| g.0 == root) {
                        Some(g) => g.1.push(f),
                        None => groups.push((root, vec![f])),
                    }
                }
            }
            for (_, members) in groups {
                let chamfer = role[members[0]] == 3;
                let mut pairs = Vec::new();
                let mut legs = Vec::new();
                for f in &members {
                    let mut distinct: Vec<u32> = if chamfer {
                        // the two faces the long edges of the strip meet
                        legs_of.get(f).map(|l| vec![l.0 .0, l.1 .0]).unwrap_or_default()
                    } else {
                        nb[*f]
                            .iter()
                            .filter(|x| comp_of.get(&x.1) != Some(&ci) && x.2 < TANGENT)
                            .map(|x| x.1)
                            .collect()
                    };
                    distinct.sort_unstable();
                    distinct.dedup();
                    if distinct.len() == 2 {
                        let (a, b) = ordered(self, FaceId(distinct[0]), FaceId(distinct[1]));
                        if !pairs.contains(&(a, b)) {
                            pairs.push((a, b));
                            if chamfer {
                                let l = &legs_of[f];
                                // the legs are given for the first face of the ordered pair
                                legs.push(if l.0 == a { (l.2, l.3) } else { (l.3, l.2) });
                            }
                        }
                    }
                }
                let size = if chamfer {
                    legs_of[&members[0]].2
                } else {
                    sc.radius[members[0]]
                };
                let angle_deg = if chamfer { legs_of[&members[0]].4 } else { 45.0 };
                let mut corners: Vec<FaceId> = Vec::new();
                for f in &members {
                    for x in &nb[*f] {
                        if matches!(role[x.1 as usize], 2 | 4) && !corners.contains(&FaceId(x.1)) {
                            corners.push(FaceId(x.1));
                        }
                    }
                }
                corners.sort();
                comp.roundings.push(Rounding {
                    chamfer,
                    size,
                    angle_deg,
                    faces: members.iter().map(|f| FaceId(*f as u32)).collect(),
                    corners,
                    pairs,
                    legs,
                });
            }
            comp.faces.sort();
        }
        Ok(BlendMap {
            components: comps
                .into_iter()
                .map(|c| c.1)
                .filter(|c| !c.roundings.is_empty())
                .collect(),
            adj,
        })
    }

    /// Edges shared by the faces `a` and `b`.
    fn edges_between(&self, a: FaceId, b: FaceId) -> Vec<EdgeId> {
        let (Ok(ea), Ok(eb)) = (self.face_edges(a), self.face_edges(b)) else {
            return Vec::new();
        };
        ea.into_iter().filter(|e| eb.contains(e)).collect()
    }

    /// The sharp edges a rounding came from, found again in `sharp` (where its faces were removed) through the faces
    /// `locate` gives for the two faces it separated.
    fn sharp_edges(&self, r: &Rounding, locate: &dyn Fn(FaceId) -> Vec<FaceId>) -> Vec<(EdgeId, usize)> {
        let mut out: Vec<(EdgeId, usize)> = Vec::new();
        for (k, (a, b)) in r.pairs.iter().enumerate() {
            for fa in locate(*a) {
                for fb in locate(*b) {
                    if fa == fb {
                        continue;
                    }
                    for e in self.edges_between(fa, fb) {
                        if !out.iter().any(|(x, _)| *x == e) {
                            out.push((e, k));
                        }
                    }
                }
            }
        }
        out
    }

    /// Makes the rounds and chamfers of `items` (`rounding`, the sizes to make) on the sharp solid `self`, where the faces of
    /// the rounding are found again through `locate`. Rounds with their own radius in one operation.
    fn make_roundings(
        &self,
        items: &[Item],
        locate: &dyn Fn(FaceId) -> Vec<FaceId>,
    ) -> Result<Option<OpResult>> {
        let mut round_edges: Vec<(EdgeId, f64)> = Vec::new();
        let mut chamfers: Vec<(EdgeId, FaceId, f64, f64)> = Vec::new();
        let mut length = 0.0;
        let mut largest: f64 = 0.0;
        for (r, size, cham) in items {
            if *size <= 1e-9 {
                continue;
            }
            let edges = self.sharp_edges(r, locate);
            if edges.is_empty() {
                return Err(KernelError::Invalid(
                    "The sharp edge could not be found again, so the round or chamfer was not changed".into(),
                ));
            }
            // a pair without an edge means the shape no longer has the corner the round was made on
            if r.pairs
                .iter()
                .enumerate()
                .any(|(k, _)| !edges.iter().any(|(_, j)| *j == k))
            {
                return Err(KernelError::Invalid(
                    "The round or chamfer does not fit the new shape (its edge disappeared), so nothing was changed".into(),
                ));
            }
            largest = largest.max(*size);
            for (e, k) in &edges {
                length += self.edge_info(*e).map(|i| i.length).unwrap_or(0.0);
                if r.chamfer {
                    let (a, b) = r.pairs[*k];
                    let (d_ref, d_other, flip) = cham.unwrap_or_else(|| {
                        let k0 = size / r.size.max(1e-12);
                        (r.legs[*k].0 * k0, r.legs[*k].1 * k0, false)
                    });
                    let reference = if flip { b } else { a };
                    let face = locate(reference)
                        .into_iter()
                        .find(|f| self.face_edges(*f).map(|v| v.contains(e)).unwrap_or(false))
                        .ok_or_else(|| {
                            KernelError::Invalid("The face of the chamfer could not be found again".into())
                        })?;
                    chamfers.push((*e, face, d_ref, d_other));
                } else {
                    round_edges.push((*e, *size));
                }
            }
        }
        if !round_edges.is_empty() && !chamfers.is_empty() {
            return Err(KernelError::Invalid(
                "Rounds and chamfers that touch each other cannot be changed together".into(),
            ));
        }
        let what = if chamfers.is_empty() { "round" } else { "chamfer" };
        let r = if !round_edges.is_empty() {
            let r = self.fillet_radii(&round_edges);
            r.map_err(|e| match e {
                KernelError::Native(_) => KernelError::Invalid(format!(
                    "The radius of {largest} mm does not fit here (it is too large for the neighbouring faces)"
                )),
                e => e,
            })?
        } else if !chamfers.is_empty() {
            let d1: Vec<f64> = chamfers.iter().map(|c| c.2).collect();
            let d2: Vec<f64> = chamfers.iter().map(|c| c.3).collect();
            let ids: Vec<u32> = chamfers.iter().map(|c| c.0 .0).collect();
            let faces: Vec<u32> = chamfers.iter().map(|c| c.1 .0).collect();
            let too_large = |e: KernelError| {
                match e {
                KernelError::Native(_) => KernelError::Invalid(format!(
                    "The chamfer of {largest} mm does not fit here (it is too large for the neighbouring faces)"
                )),
                e => e,
            }
            };
            // equal symmetric chamfers are made with the symmetric operation, as the user made them
            let symmetric =
                d1.iter().zip(&d2).all(|(a, b)| same_size(*a, *b)) && d1.iter().all(|a| same_size(*a, d1[0]));
            if symmetric {
                let edges: Vec<EdgeId> = chamfers.iter().map(|c| c.0).collect();
                return self.chamfer(&edges, d1[0]).map(Some).map_err(too_large);
            }
            let r = native::chamfer_edges_asym(self.raw(), &ids, &faces, &d1, &d2)
                .map_err(KernelError::from)
                .and_then(crate::finish)
                .map_err(too_large)?;
            self.check_blend(&r.solid, largest, length, what)?;
            r
        } else {
            return Ok(None);
        };
        Ok(Some(r))
    }

    /// Changes the size of the round or chamfer that `face` belongs to (zero removes it). `whole_chain`: all rounds of the
    /// same size that touch it follow. The faces of the rounds that touch it are made again, so nothing is left behind at a
    /// corner. `chamfer`: the distance on the reference face and the angle (degrees) and whether the reference is the second face.
    pub fn resize_blend(
        &self,
        face: FaceId,
        size: f64,
        whole_chain: bool,
        chamfer: Option<(f64, bool)>,
    ) -> Result<OpResult> {
        if !size.is_finite() || size < 0.0 {
            return Err(KernelError::Invalid(
                "the size must be zero or a positive number".into(),
            ));
        }
        let map = self.blend_map();
        let (ci, ri) = map
            .rounding_of(face)
            .ok_or_else(|| KernelError::Invalid("this face is not a round or a chamfer".into()))?;
        let comp = &map.components[ci];
        let target = &comp.roundings[ri];
        let selected = |i: usize| {
            i == ri
                || (whole_chain
                    && comp.roundings[i].chamfer == target.chamfer
                    && same_size(comp.roundings[i].size, target.size))
        };
        let before = self.volume()?;
        let ids: Vec<u32> = comp.faces.iter().map(|f| f.0).collect();
        let open = crate::finish(native::defeature_faces(self.raw(), &ids)?).map_err(|e| match e {
            KernelError::Native(m) => KernelError::Invalid(format!(
                "The round or chamfer cannot be changed here: {}",
                m.rsplit(": ").next().unwrap_or(&m)
            )),
            e => e,
        })?;
        if (open.solid.volume()? - before).abs() < 1e-9 {
            return Err(KernelError::Invalid(
                "The round or chamfer could not be removed cleanly, so it was not changed".into(),
            ));
        }
        let locate = |f: FaceId| open.history.successors(0, f);
        let items: Vec<Item> = comp
            .roundings
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if selected(i) {
                    let c = chamfer.map(|(angle, flip)| {
                        let d = size;
                        (d, d * angle.to_radians().tan(), flip)
                    });
                    (r, size, c)
                } else {
                    (r, r.size, None)
                }
            })
            .collect();
        let made = open.solid.make_roundings(&items, &locate)?;
        match made {
            None => Ok(open),
            Some(m) => Ok(OpResult {
                history: open.history.compose(&m.history),
                solid: m.solid,
            }),
        }
    }

    /// Moves the planar `face` by `distance` along its outward normal and keeps the rounds and chamfers around it: they are
    /// removed, the face is moved and they are made again on the corresponding edges with the same sizes. Nothing is
    /// changed (an error tells why) when they do not fit the new shape.
    pub(crate) fn pull_face_with_blends(
        &self,
        face: FaceId,
        distance: f64,
        touching: &[usize],
    ) -> Result<OpResult> {
        let map = self.blend_map();
        let info = self.face_info(face)?;
        let n = info.normal.normalize();
        let before_faces = self.face_count();
        let mut ids: Vec<u32> = Vec::new();
        let mut roundings: Vec<&Rounding> = Vec::new();
        for ci in touching {
            let c = &map.components[*ci];
            ids.extend(c.faces.iter().map(|f| f.0));
            roundings.extend(c.roundings.iter());
        }
        ids.sort_unstable();
        ids.dedup();
        let volume = self.volume()?;
        let open = crate::finish(native::defeature_faces(self.raw(), &ids)?).map_err(|e| match e {
            KernelError::Native(m) => KernelError::Invalid(format!(
                "The rounds next to the face cannot be moved with it: {}",
                m.rsplit(": ").next().unwrap_or(&m)
            )),
            e => e,
        })?;
        if (open.solid.volume()? - volume).abs() < 1e-9 && !roundings.is_empty() {
            return Err(KernelError::Invalid(
                "The rounds next to the face could not be removed cleanly, so nothing was changed".into(),
            ));
        }
        let p0 = open
            .history
            .successors(0, face)
            .into_iter()
            .find(|f| {
                open.solid
                    .face_info(*f)
                    .map(|i| i.kind == FaceKind::Plane && i.normal.dot(n) > 0.9999)
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                KernelError::Invalid("The moved face was lost when the rounds were removed".into())
            })?;
        let p0_info = open.solid.face_info(p0)?;
        let offset0 = n.dot(p0_info.origin);
        let moved = open.solid.pull_face_plain(p0, distance)?;
        let sharp = &moved.solid;
        // planar faces are found by their plane (the moved one by the plane it moved to), others through the history
        let plane_faces = |normal: DVec3, offset: f64| -> Vec<FaceId> {
            sharp
                .faces()
                .filter(|f| {
                    sharp
                        .face_info(*f)
                        .map(|i| {
                            i.kind == FaceKind::Plane
                                && i.normal.dot(normal) > 0.9999
                                && (normal.dot(i.origin) - offset).abs() < 1e-6
                        })
                        .unwrap_or(false)
                })
                .collect()
        };
        let locate = |f: FaceId| -> Vec<FaceId> {
            if f == face {
                return plane_faces(n, offset0 + distance);
            }
            let mut v: Vec<FaceId> = open
                .history
                .successors(0, f)
                .into_iter()
                .flat_map(|m| moved.history.successors(0, m))
                .collect();
            if let Ok(i) = self.face_info(f) {
                if i.kind == FaceKind::Plane {
                    v.extend(plane_faces(
                        i.normal.normalize(),
                        i.normal.normalize().dot(i.origin),
                    ));
                }
            }
            v.sort();
            v.dedup();
            v
        };
        let items: Vec<Item> = roundings.iter().map(|r| (*r, r.size, None)).collect();
        let made = sharp.make_roundings(&items, &locate)?;
        let (result, hist) = match made {
            Some(m) => (m.solid, open.history.compose(&moved.history).compose(&m.history)),
            None => (moved.solid.clone(), open.history.compose(&moved.history)),
        };
        let mut hist = hist;
        // the moved face itself
        for f in result.faces() {
            if let Ok(i) = result.face_info(f) {
                if i.kind == FaceKind::Plane
                    && i.normal.dot(n) > 0.9999
                    && (n.dot(i.origin) - (offset0 + distance)).abs() < 1e-6
                    && !hist.links.iter().any(|l| l.0 == 0 && l.1 == face && l.2 == f)
                {
                    hist.links.push((0, face, f));
                }
            }
        }
        let _ = before_faces;
        Ok(OpResult {
            solid: result,
            history: hist,
        })
    }
}
