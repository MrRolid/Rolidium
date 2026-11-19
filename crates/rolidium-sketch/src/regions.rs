// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Closed-region detection (planar arrangement of lines and circles).

use crate::{Entity, EntityId};
use rolidium_path2d::{norm_angle, Contour, DVec2, Segment};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

const TOL: f64 = 1e-6;

/// A closed region: outer boundary (counter-clockwise), holes (clockwise).
#[derive(Debug, Clone)]
pub struct Region {
    pub outer: Contour,
    pub holes: Vec<Contour>,
    /// Entities that contribute boundary curves to this region.
    pub border_entities: Vec<EntityId>,
    /// Entities that border this region and no other region: they disappear when the region is used.
    pub exclusive_entities: Vec<EntityId>,
}

impl Region {
    pub fn area(&self) -> f64 {
        self.outer.signed_area() + self.holes.iter().map(Contour::signed_area).sum::<f64>()
    }

    pub fn contains(&self, p: DVec2) -> bool {
        self.outer.contains(p) && !self.holes.iter().any(|h| h.contains(p))
    }

    /// Triangles (in sketch coordinates) covering the region, arcs approximated to `max_step` radians.
    pub fn triangulate(&self, max_step: f64) -> Vec<[DVec2; 3]> {
        let mut coords: Vec<f64> = Vec::new();
        let mut holes: Vec<usize> = Vec::new();
        let mut pts: Vec<DVec2> = Vec::new();
        let push = |poly: Vec<DVec2>, pts: &mut Vec<DVec2>, coords: &mut Vec<f64>| {
            for p in poly {
                pts.push(p);
                coords.push(p.x);
                coords.push(p.y);
            }
        };
        push(self.outer.polyline(max_step), &mut pts, &mut coords);
        for h in &self.holes {
            holes.push(pts.len());
            push(h.polyline(max_step), &mut pts, &mut coords);
        }
        match earcutr::earcut(&coords, &holes, 2) {
            Ok(idx) => idx
                .chunks_exact(3)
                .map(|t| [pts[t[0]], pts[t[1]], pts[t[2]]])
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Prim {
    Line {
        a: DVec2,
        b: DVec2,
    },
    Circle {
        c: DVec2,
        r: f64,
    },
    /// Counter-clockwise arc `start .. start + sweep` (sweep > 0).
    Arc {
        c: DVec2,
        r: f64,
        start: f64,
        sweep: f64,
    },
}

fn prims_of(entities: &[(EntityId, Entity)]) -> Vec<(EntityId, Prim)> {
    let mut v = Vec::new();
    for (id, e) in entities {
        match *e {
            Entity::Line { a, b } => v.push((*id, Prim::Line { a, b })),
            Entity::Rect { a, b } => {
                let c = Entity::rect_corners(a, b);
                for i in 0..4 {
                    v.push((
                        *id,
                        Prim::Line {
                            a: c[i],
                            b: c[(i + 1) % 4],
                        },
                    ));
                }
            }
            Entity::Circle { center, radius } => v.push((*id, Prim::Circle { c: center, r: radius })),
            Entity::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let (s0, sw) = if sweep < 0.0 {
                    (start + sweep, -sweep)
                } else {
                    (start, sweep)
                };
                v.push((
                    *id,
                    Prim::Arc {
                        c: center,
                        r: radius,
                        start: s0,
                        sweep: sw,
                    },
                ));
            }
        }
    }
    v
}

fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// An angle lies on a counter-clockwise arc.
fn in_arc(ang: f64, r: f64, start: f64, sweep: f64) -> bool {
    let u = norm_angle(ang - start);
    u <= sweep + TOL / r || TAU - u <= TOL / r
}

/// Parameters of intersection points: for a line the `t` in 0..1, for a circle the angle, for an arc the angle.
fn intersect(p: &Prim, q: &Prim) -> (Vec<f64>, Vec<f64>) {
    let as_circle = |x: &Prim| match *x {
        Prim::Arc { c, r, .. } => Prim::Circle { c, r },
        o => o,
    };
    let (pc, qc) = (as_circle(p), as_circle(q));
    if matches!(p, Prim::Arc { .. }) || matches!(q, Prim::Arc { .. }) {
        let (a, b) = intersect_basic(&pc, &qc);
        // keep a point only if it lies on every arc involved (the lists are aligned for these cases)
        let n = a.len().min(b.len());
        let (mut ra, mut rb) = (vec![], vec![]);
        for i in 0..n {
            let ok_p = match p {
                Prim::Arc { r, start, sweep, .. } => in_arc(a[i], *r, *start, *sweep),
                _ => true,
            };
            let ok_q = match q {
                Prim::Arc { r, start, sweep, .. } => in_arc(b[i], *r, *start, *sweep),
                _ => true,
            };
            if ok_p && ok_q {
                ra.push(a[i]);
                rb.push(b[i]);
            }
        }
        return (ra, rb);
    }
    intersect_basic(p, q)
}

fn intersect_basic(p: &Prim, q: &Prim) -> (Vec<f64>, Vec<f64>) {
    match (*p, *q) {
        (Prim::Line { a: p0, b: p1 }, Prim::Line { a: q0, b: q1 }) => {
            let (r, s) = (p1 - p0, q1 - q0);
            let (lr, ls) = (r.length(), s.length());
            let den = cross(r, s);
            if den.abs() > 1e-12 * lr * ls {
                let t = cross(q0 - p0, s) / den;
                let u = cross(q0 - p0, r) / den;
                let (et, eu) = (TOL / lr, TOL / ls);
                if t >= -et && t <= 1.0 + et && u >= -eu && u <= 1.0 + eu {
                    return (vec![t.clamp(0.0, 1.0)], vec![u.clamp(0.0, 1.0)]);
                }
                return (vec![], vec![]);
            }
            // parallel: collinear overlap?
            if (cross(q0 - p0, r) / lr).abs() > TOL {
                return (vec![], vec![]);
            }
            let (mut ts, mut us) = (vec![], vec![]);
            for e in [q0, q1] {
                let t = (e - p0).dot(r) / (lr * lr);
                if t >= -TOL / lr && t <= 1.0 + TOL / lr {
                    ts.push(t.clamp(0.0, 1.0));
                }
            }
            for e in [p0, p1] {
                let u = (e - q0).dot(s) / (ls * ls);
                if u >= -TOL / ls && u <= 1.0 + TOL / ls {
                    us.push(u.clamp(0.0, 1.0));
                }
            }
            (ts, us)
        }
        (Prim::Line { a, b }, Prim::Circle { c, r }) => {
            let (ts, angs) = line_circle(a, b, c, r);
            (ts, angs)
        }
        (Prim::Circle { c, r }, Prim::Line { a, b }) => {
            let (ts, angs) = line_circle(a, b, c, r);
            (angs, ts)
        }
        (Prim::Arc { .. }, _) | (_, Prim::Arc { .. }) => (vec![], vec![]),
        (Prim::Circle { c: c1, r: r1 }, Prim::Circle { c: c2, r: r2 }) => {
            let d = c1.distance(c2);
            if d < TOL || d > r1 + r2 + TOL || d < (r1 - r2).abs() - TOL {
                return (vec![], vec![]);
            }
            let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let h2 = r1 * r1 - a * a;
            let ex = (c2 - c1) / d;
            let ey = DVec2::new(-ex.y, ex.x);
            let mid = c1 + ex * a;
            let pts: Vec<DVec2> = if h2 <= (TOL * TOL).max(1e-18)
                || (d - (r1 + r2)).abs() < TOL
                || (d - (r1 - r2).abs()).abs() < TOL
            {
                vec![mid]
            } else {
                let h = h2.sqrt();
                vec![mid + ey * h, mid - ey * h]
            };
            (
                pts.iter().map(|p| (*p - c1).to_angle()).collect(),
                pts.iter().map(|p| (*p - c2).to_angle()).collect(),
            )
        }
    }
}

fn line_circle(a: DVec2, b: DVec2, c: DVec2, r: f64) -> (Vec<f64>, Vec<f64>) {
    let d = b - a;
    let l = d.length();
    let t0 = (c - a).dot(d) / (l * l);
    let h = cross(d, c - a).abs() / l;
    if h > r + TOL {
        return (vec![], vec![]);
    }
    let ts: Vec<f64> = if h >= r - TOL {
        vec![t0]
    } else {
        let dt = (r * r - h * h).sqrt() / l;
        vec![t0 - dt, t0 + dt]
    };
    let et = TOL / l;
    let ts: Vec<f64> = ts
        .into_iter()
        .filter(|t| *t >= -et && *t <= 1.0 + et)
        .map(|t| t.clamp(0.0, 1.0))
        .collect();
    let angs = ts.iter().map(|t| (a + d * *t - c).to_angle()).collect();
    (ts, angs)
}

struct Edge {
    n0: usize,
    n1: usize,
    seg: Segment,
    entities: BTreeSet<EntityId>,
}

struct Nodes {
    pts: Vec<DVec2>,
}

impl Nodes {
    fn get(&mut self, p: DVec2) -> usize {
        if let Some(i) = self.pts.iter().position(|q| q.distance(p) <= 10.0 * TOL) {
            return i;
        }
        self.pts.push(p);
        self.pts.len() - 1
    }
}

fn build_edges(entities: &[(EntityId, Entity)]) -> (Vec<Edge>, Vec<DVec2>) {
    let prims = prims_of(entities);
    let mut splits: Vec<Vec<f64>> = prims
        .iter()
        .map(|(_, p)| match p {
            Prim::Line { .. } => vec![0.0, 1.0],
            Prim::Circle { .. } => vec![0.0],
            Prim::Arc { sweep, .. } => vec![0.0, *sweep],
        })
        .collect();
    for i in 0..prims.len() {
        for j in (i + 1)..prims.len() {
            let (a, b) = intersect(&prims[i].1, &prims[j].1);
            splits[i].extend(a);
            splits[j].extend(b);
        }
    }
    let mut nodes = Nodes { pts: Vec::new() };
    let mut edges: Vec<Edge> = Vec::new();
    let add = |seg: Segment, id: EntityId, nodes: &mut Nodes, edges: &mut Vec<Edge>| {
        let (n0, n1) = (nodes.get(seg.start_point()), nodes.get(seg.end_point()));
        if n0 == n1 && !matches!(seg, Segment::Arc { .. }) {
            return;
        }
        let mid = seg.mid_point();
        for e in edges.iter_mut() {
            let same = e.n0 == n0 && e.n1 == n1 && e.seg.mid_point().distance(mid) <= 10.0 * TOL;
            let rev = e.n0 == n1 && e.n1 == n0 && e.seg.mid_point().distance(mid) <= 10.0 * TOL;
            if same || rev {
                e.entities.insert(id);
                return;
            }
        }
        edges.push(Edge {
            n0,
            n1,
            seg,
            entities: BTreeSet::from([id]),
        });
    };
    for (k, (id, p)) in prims.iter().enumerate() {
        match *p {
            Prim::Line { a, b } => {
                let len = a.distance(b);
                let mut t = splits[k].clone();
                t.sort_by(|x, y| x.partial_cmp(y).unwrap());
                t.dedup_by(|x, y| (*x - *y).abs() * len <= TOL);
                for w in t.windows(2) {
                    add(
                        Segment::Line {
                            a: a + (b - a) * w[0],
                            b: a + (b - a) * w[1],
                        },
                        *id,
                        &mut nodes,
                        &mut edges,
                    );
                }
            }
            Prim::Arc { c, r, start, sweep } => {
                // parameters are measured from the start of the arc
                let mut u: Vec<f64> = splits[k]
                    .iter()
                    .enumerate()
                    .map(|(i, x)| if i < 2 { *x } else { norm_angle(*x - start) })
                    .map(|x| x.min(sweep).max(0.0))
                    .collect();
                u.sort_by(|x, y| x.partial_cmp(y).unwrap());
                u.dedup_by(|x, y| (*x - *y).abs() * r <= TOL);
                for w in u.windows(2) {
                    add(
                        Segment::Arc {
                            center: c,
                            radius: r,
                            start: start + w[0],
                            sweep: w[1] - w[0],
                        },
                        *id,
                        &mut nodes,
                        &mut edges,
                    );
                }
            }
            Prim::Circle { c, r } => {
                let mut a: Vec<f64> = splits[k].iter().map(|x| norm_angle(*x)).collect();
                a.sort_by(|x, y| x.partial_cmp(y).unwrap());
                a.dedup_by(|x, y| (*x - *y).abs() * r <= TOL);
                // an angle just below TAU duplicates the seam at 0
                if a.len() > 1 && (TAU - a[a.len() - 1]) * r <= TOL {
                    a.pop();
                }
                let n = a.len();
                for i in 0..n {
                    let start = a[i];
                    let end = if i + 1 < n { a[i + 1] } else { a[0] + TAU };
                    add(
                        Segment::Arc {
                            center: c,
                            radius: r,
                            start,
                            sweep: end - start,
                        },
                        *id,
                        &mut nodes,
                        &mut edges,
                    );
                }
            }
        }
    }
    (edges, nodes.pts)
}

/// Marks edges that are bridges (including dangling chains); they bound no region.
fn find_bridges(n_nodes: usize, edges: &[Edge]) -> Vec<bool> {
    let mut adj: Vec<Vec<(usize, usize)>> = vec![vec![]; n_nodes];
    for (i, e) in edges.iter().enumerate() {
        if e.n0 != e.n1 {
            adj[e.n0].push((e.n1, i));
            adj[e.n1].push((e.n0, i));
        }
    }
    let mut disc = vec![usize::MAX; n_nodes];
    let mut low = vec![0; n_nodes];
    let mut bridge = vec![false; edges.len()];
    let mut timer = 0;
    for root in 0..n_nodes {
        if disc[root] != usize::MAX {
            continue;
        }
        // iterative DFS: (node, parent edge, next neighbour index)
        let mut stack = vec![(root, usize::MAX, 0usize)];
        disc[root] = timer;
        low[root] = timer;
        timer += 1;
        while let Some(&mut (v, pe, ref mut k)) = stack.last_mut() {
            if *k < adj[v].len() {
                let (w, ei) = adj[v][*k];
                *k += 1;
                if ei == pe {
                    continue;
                }
                if disc[w] == usize::MAX {
                    disc[w] = timer;
                    low[w] = timer;
                    timer += 1;
                    stack.push((w, ei, 0));
                } else {
                    low[v] = low[v].min(disc[w]);
                }
            } else {
                stack.pop();
                if let Some(&(p, _, _)) = stack.last() {
                    low[p] = low[p].min(low[v]);
                    if low[v] > disc[p] {
                        bridge[pe] = true;
                    }
                }
            }
        }
    }
    bridge
}

pub(crate) fn find_regions(entities: &[(EntityId, Entity)]) -> Vec<Region> {
    let (edges, pts) = build_edges(entities);
    let n_nodes = pts.len();
    let bridge = find_bridges(n_nodes, &edges);
    let alive: Vec<usize> = (0..edges.len()).filter(|i| !bridge[*i]).collect();
    if alive.is_empty() {
        return Vec::new();
    }
    // half-edge h = 2*e + dir
    let seg_of = |h: usize| {
        if h % 2 == 0 {
            edges[h / 2].seg
        } else {
            edges[h / 2].seg.reversed()
        }
    };
    let origin = |h: usize| {
        if h % 2 == 0 {
            edges[h / 2].n0
        } else {
            edges[h / 2].n1
        }
    };
    let dest = |h: usize| {
        if h % 2 == 0 {
            edges[h / 2].n1
        } else {
            edges[h / 2].n0
        }
    };

    let mut out: Vec<Vec<usize>> = vec![vec![]; n_nodes];
    for &e in &alive {
        out[edges[e].n0].push(2 * e);
        out[edges[e].n1].push(2 * e + 1);
    }
    for list in out.iter_mut() {
        list.sort_by(|&a, &b| {
            let (sa, sb) = (seg_of(a), seg_of(b));
            let (ta, tb) = (norm_angle(sa.start_tangent()), norm_angle(sb.start_tangent()));
            let d = ta - tb;
            if d.abs() > 1e-9 && (TAU - d.abs()) > 1e-9 {
                ta.partial_cmp(&tb).unwrap()
            } else {
                sa.curvature().partial_cmp(&sb.curvature()).unwrap()
            }
        });
    }
    let next = |h: usize| -> usize {
        let v = dest(h);
        let tw = h ^ 1;
        let list = &out[v];
        let i = list
            .iter()
            .position(|x| *x == tw)
            .expect("twin must leave the node");
        list[(i + list.len() - 1) % list.len()]
    };

    // union-find for components
    let mut parent: Vec<usize> = (0..n_nodes).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let n = p[c];
            p[c] = r;
            c = n;
        }
        r
    }
    for &e in &alive {
        let (a, b) = (find(&mut parent, edges[e].n0), find(&mut parent, edges[e].n1));
        parent[a] = b;
    }

    struct Cycle {
        hs: Vec<usize>,
        area: f64,
        comp: usize,
    }
    let mut seen = vec![false; edges.len() * 2];
    let mut cycles: Vec<Cycle> = Vec::new();
    for &e in &alive {
        for dir in 0..2 {
            let start = 2 * e + dir;
            if seen[start] {
                continue;
            }
            let mut hs = Vec::new();
            let mut h = start;
            loop {
                seen[h] = true;
                hs.push(h);
                h = next(h);
                if h == start || seen[h] {
                    break;
                }
            }
            let area: f64 = hs.iter().map(|h| seg_of(*h).area_term()).sum();
            let comp = find(&mut parent, origin(start));
            cycles.push(Cycle { hs, area, comp });
        }
    }

    let contour = |c: &Cycle| Contour {
        segments: c.hs.iter().map(|h| seg_of(*h)).collect(),
    };
    // bounded faces
    let faces: Vec<usize> = (0..cycles.len()).filter(|i| cycles[*i].area > 1e-12).collect();
    let face_contours: Vec<Contour> = faces.iter().map(|i| contour(&cycles[*i])).collect();
    let face_index_of_cycle: BTreeMap<usize, usize> =
        faces.iter().enumerate().map(|(k, c)| (*c, k)).collect();

    // outer boundary of each component, and the face it lies in
    let mut holes_of: Vec<Vec<usize>> = vec![vec![]; faces.len()]; // cycle indices
    let mut parent_face_of_cycle: BTreeMap<usize, usize> = BTreeMap::new();
    for (ci, c) in cycles.iter().enumerate() {
        if c.area >= -1e-12 {
            continue;
        }
        let p = pts[origin(c.hs[0])];
        let mut best: Option<(f64, usize)> = None;
        for (k, fi) in faces.iter().enumerate() {
            if cycles[*fi].comp == c.comp {
                continue;
            }
            if face_contours[k].contains(p) {
                let a = cycles[*fi].area;
                if best.is_none_or(|(ba, _)| a < ba) {
                    best = Some((a, k));
                }
            }
        }
        if let Some((_, k)) = best {
            holes_of[k].push(ci);
            parent_face_of_cycle.insert(ci, k);
        }
    }

    // entity -> regions bordering it
    let mut borders: BTreeMap<EntityId, BTreeSet<usize>> = BTreeMap::new();
    let mut regions: Vec<Region> = Vec::new();
    for (k, fi) in faces.iter().enumerate() {
        let mut ents: BTreeSet<EntityId> = BTreeSet::new();
        for h in &cycles[*fi].hs {
            ents.extend(edges[h / 2].entities.iter().copied());
        }
        let mut holes = Vec::new();
        for ci in &holes_of[k] {
            holes.push(contour(&cycles[*ci]));
            for h in &cycles[*ci].hs {
                ents.extend(edges[h / 2].entities.iter().copied());
            }
        }
        for e in &ents {
            borders.entry(*e).or_default().insert(k);
        }
        regions.push(Region {
            outer: face_contours[k].clone(),
            holes,
            border_entities: ents.into_iter().collect(),
            exclusive_entities: Vec::new(),
        });
    }
    let _ = face_index_of_cycle;
    let _ = parent_face_of_cycle;
    for (k, r) in regions.iter_mut().enumerate() {
        r.exclusive_entities = r
            .border_entities
            .iter()
            .copied()
            .filter(|e| borders[e].len() == 1 && borders[e].contains(&k))
            .collect();
    }
    regions
}
