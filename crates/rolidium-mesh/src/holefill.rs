// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Patching of holes: a hole is a closed loop of open edges (edges that belong to one triangle only). Small and flat holes are patched by a triangulation of
//! the rim in its best-fit plane (ear clipping, no new vertices); a large hole or one with a curved rim is left alone and the reason is told, because
//! filling it would invent a surface. The new triangles run the way the neighbours demand, so a closed mesh with a hole stays consistent.

use crate::check::{runs, sorted_edges};
use crate::{cross, dot, len, mul, sub, unit, TriMesh, V3};
use std::collections::{HashMap, HashSet};

/// What is known about one hole.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HoleInfo {
    /// The number of vertices (and of open edges) of the loop.
    pub vertices: usize,
    /// The length of the rim in mm.
    pub perimeter: f64,
    /// The area of the rim projected on its best-fit plane, in mm^2 (zero for a rim that folds over itself).
    pub area: f64,
    /// The largest distance of a rim vertex from the best-fit plane, in mm.
    pub planarity: f64,
    /// The diameter of a circle (a sphere) that holds the whole rim, in mm.
    pub diameter: f64,
}

/// The limits of the automatic patching.
#[derive(Debug, Clone, PartialEq)]
pub struct FillOptions {
    /// A hole whose rim is wider than this (mm) is left.
    pub max_diameter: f64,
    /// A rim may be this far (mm) from a plane ...
    pub max_planarity: f64,
    /// ... or this part of its diameter, whichever allows more (a bigger hole may bend a little more).
    pub max_planarity_ratio: f64,
    /// A loop of more vertices is left.
    pub max_vertices: usize,
}

impl Default for FillOptions {
    fn default() -> Self {
        FillOptions {
            max_diameter: 10.0,
            max_planarity: 0.05,
            max_planarity_ratio: 0.01,
            max_vertices: 200,
        }
    }
}

/// What a call of [`fill_holes`] or [`fill_all`] did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillReport {
    /// The number of holes patched.
    pub filled: usize,
    /// The number of triangles added to the mesh.
    pub added_triangles: usize,
    /// The holes left alone, each with a plain-English reason.
    pub left: Vec<(HoleInfo, String)>,
}

/// Where a rim lies: its centre, the unit normal of the best-fit plane (turned to the side the patch has to face) and two unit vectors in the plane.
struct Frame {
    centre: V3,
    n: V3,
    u: V3,
    v: V3,
}

struct Rim {
    ids: Vec<u32>,
    info: HoleInfo,
    frame: Frame,
    /// the rim vertices in the plane of the frame
    flat: Vec<[f64; 2]>,
    /// a vertex occurs twice in the loop (the rim touches itself)
    pinched: bool,
}

/// The loops of open edges, each as vertex ids in the order a patch triangle runs along the rim (counter-clockwise seen from the outside, so the neighbouring
/// triangle runs the other way), with what is known about the hole. A mesh without holes returns nothing; an open surface has one loop only if its rim closes.
pub fn find_holes(m: &TriMesh) -> Vec<(Vec<u32>, HoleInfo)> {
    boundary_loops(m)
        .into_iter()
        .map(|ids| {
            let r = analyse(m, ids);
            (r.ids, r.info)
        })
        .collect()
}

/// Patches the small flat holes of the mesh in place and reports what it did and what it left. See [`FillOptions`] for what is small and flat.
pub fn fill_holes(m: &mut TriMesh, o: &FillOptions) -> FillReport {
    fill_with(m, o, false)
}

/// Patches every hole whose rim can be patched at all, however big or curved it is: the rim is triangulated flat in its best-fit plane, or, where that is
/// not possible (the rim folds over itself in the plane), by a fan from a new vertex in the middle. Holes that touch themselves, lie inside another hole
/// (islands) or have no area are still left, with the reason.
pub fn fill_all(m: &mut TriMesh) -> FillReport {
    let o = FillOptions {
        max_diameter: f64::INFINITY,
        max_planarity: f64::INFINITY,
        max_planarity_ratio: f64::INFINITY,
        max_vertices: usize::MAX,
    };
    fill_with(m, &o, true)
}

fn fill_with(m: &mut TriMesh, o: &FillOptions, fan: bool) -> FillReport {
    let mut report = FillReport::default();
    let rims: Vec<Rim> = boundary_loops(m).into_iter().map(|ids| analyse(m, ids)).collect();
    if rims.is_empty() {
        return report;
    }
    // why not: the reason a hole is left, by the limits
    let mut why: Vec<Option<String>> = rims.iter().map(|r| reason_to_leave(r, o)).collect();
    // islands: a candidate whose plane holds another loop inside its rim
    for i in 0..rims.len() {
        if why[i].is_some() {
            continue;
        }
        for j in 0..rims.len() {
            if i != j && nested(&rims[i], &rims[j]) {
                why[i] = Some("another hole lies inside its rim (an island of the surface is left in the middle), so a patch would cover the island".into());
                if why[j].is_none() {
                    why[j] = Some("it lies inside the rim of another hole in the same plane (it is an island), a patch would double the surface there".into());
                }
            }
        }
    }
    // the edges between the rim vertices that the mesh already has: a diagonal along one would make an edge of three triangles
    let existing = edges_among_rims(m, &rims, &why);
    let mut new_triangles: Vec<[u32; 3]> = Vec::new();
    let mut new_points: Vec<[f32; 3]> = Vec::new();
    let base = m.positions.len() as u32;
    for (r, w) in rims.iter().zip(why) {
        if let Some(text) = w {
            report.left.push((r.info.clone(), text));
            continue;
        }
        let tris = match triangulate(r, &existing) {
            Some(t) => t,
            None if fan => {
                // a fan from the middle: a new vertex
                let c = r.ids.iter().fold([0.0; 3], |s, &i| crate::add(s, m.vertex(i)));
                let c = mul(c, 1.0 / r.ids.len() as f64);
                let id = base + new_points.len() as u32;
                new_points.push([c[0] as f32, c[1] as f32, c[2] as f32]);
                (0..r.ids.len())
                    .map(|k| [r.ids[k], r.ids[(k + 1) % r.ids.len()], id])
                    .collect()
            }
            None => {
                report.left.push((
                    r.info.clone(),
                    "its rim could not be triangulated cleanly (it folds over itself or touches existing edges in its plane)".into(),
                ));
                continue;
            }
        };
        report.filled += 1;
        report.added_triangles += tris.len();
        new_triangles.extend(tris);
    }
    m.positions.extend(new_points);
    m.triangles.extend(new_triangles);
    report
}

fn reason_to_leave(r: &Rim, o: &FillOptions) -> Option<String> {
    let i = &r.info;
    if r.pinched {
        return Some(
            "its rim touches itself at a vertex (the hole is pinched), so it is not clear how to patch it"
                .into(),
        );
    }
    if i.vertices < 3 {
        return Some("its rim has fewer than three vertices".into());
    }
    if i.area <= 1e-12 * (i.perimeter * i.perimeter).max(1e-300) {
        return Some(
            "its rim encloses no area in its best-fit plane (a slit, or a rim folded over itself), so there is nothing to patch with".into(),
        );
    }
    let curved = i.planarity > o.max_planarity.max(o.max_planarity_ratio * i.diameter);
    let wide = i.diameter > o.max_diameter;
    if curved {
        let mut s = format!("its rim is curved ({:.2} mm off a plane)", i.planarity);
        if wide {
            s += &format!(" and {:.0} mm wide", i.diameter);
        }
        s += "; filling it would invent a surface";
        return Some(s);
    }
    if wide {
        return Some(format!(
            "it is {:.1} mm wide, more than the {:.1} mm that are patched by themselves; filling it would invent a surface",
            i.diameter, o.max_diameter
        ));
    }
    if i.vertices > o.max_vertices {
        return Some(format!(
            "its rim has {} vertices, more than the {} that are patched by themselves",
            i.vertices, o.max_vertices
        ));
    }
    None
}

/// Does the loop `inner` lie in the plane of `outer` and inside its rim?
fn nested(outer: &Rim, inner: &Rim) -> bool {
    if outer.pinched || inner.ids.is_empty() {
        return false;
    }
    let f = &outer.frame;
    let tol = outer.info.planarity + 1e-3 * outer.info.diameter + 1e-6;
    // the loops far apart cannot be nested: the centre of the inner one against the radius of the outer one
    let d = len(sub(inner.frame.centre, f.centre));
    if d > 0.5 * outer.info.diameter + 0.5 * inner.info.diameter + tol {
        return false;
    }
    // all the inner points have to be in the plane and inside
    inner_points(outer, inner)
        .iter()
        .all(|(h, p)| h.abs() <= tol && inside_polygon(&outer.flat, *p))
}

/// The points of `inner` as (height over the plane of `outer`, position in it).
fn inner_points(outer: &Rim, inner: &Rim) -> Vec<(f64, [f64; 2])> {
    // the inner loop is stored in its own frame; rebuild the 3D points from it
    let g = &inner.frame;
    inner
        .flat
        .iter()
        .map(|p| {
            let q = [
                g.centre[0] + g.u[0] * p[0] + g.v[0] * p[1],
                g.centre[1] + g.u[1] * p[0] + g.v[1] * p[1],
                g.centre[2] + g.u[2] * p[0] + g.v[2] * p[1],
            ];
            let d = sub(q, outer.frame.centre);
            (
                dot(d, outer.frame.n),
                [dot(d, outer.frame.u), dot(d, outer.frame.v)],
            )
        })
        .collect()
}

fn inside_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The undirected edges of the mesh that join two vertices of rims that are going to be patched.
fn edges_among_rims(m: &TriMesh, rims: &[Rim], why: &[Option<String>]) -> HashSet<u64> {
    let mut mark: HashSet<u32> = HashSet::new();
    for (r, w) in rims.iter().zip(why) {
        if w.is_none() {
            mark.extend(r.ids.iter().copied());
        }
    }
    let mut set = HashSet::new();
    if mark.is_empty() {
        return set;
    }
    for t in &m.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if mark.contains(&a) && mark.contains(&b) {
                set.insert(edge_key(a, b));
            }
        }
    }
    set
}

fn edge_key(a: u32, b: u32) -> u64 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    ((lo as u64) << 32) | hi as u64
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// the loops
// ---------------------------------------------------------------------------------------------------------------------------------------------------------

/// The loops of open edges as vertex ids in patch order. At a vertex where several holes meet the triangles round the vertex say which edge continues which.
fn boundary_loops(m: &TriMesh) -> Vec<Vec<u32>> {
    let edges = sorted_edges(m);
    // the edge a patch triangle runs: against the one triangle that has the edge
    let mut fill: Vec<(u32, u32)> = Vec::new();
    for run in runs(&edges) {
        if run.len() == 1 {
            let (key, id) = run[0];
            let (lo, hi) = ((key >> 32) as u32, key as u32);
            if lo == hi {
                continue;
            }
            // the triangle runs lo -> hi when the flag is set
            fill.push(if id & 1 == 1 { (hi, lo) } else { (lo, hi) });
        }
    }
    drop(edges);
    if fill.is_empty() {
        return Vec::new();
    }
    fill.sort_unstable();
    let mut outs: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut ins: HashMap<u32, Vec<usize>> = HashMap::new();
    for (e, &(a, b)) in fill.iter().enumerate() {
        outs.entry(a).or_default().push(e);
        ins.entry(b).or_default().push(e);
    }
    let pinch: HashSet<u32> = outs
        .iter()
        .filter(|(v, o)| o.len() > 1 || ins.get(v).is_none_or(|i| i.len() != 1))
        .map(|(v, _)| *v)
        .chain(ins.iter().filter(|(_, i)| i.len() > 1).map(|(v, _)| *v))
        .collect();
    // the triangles round the vertices where it is not clear: (p, q) of the triangle (v, p, q)
    let mut fans: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    if !pinch.is_empty() {
        for t in &m.triangles {
            for k in 0..3 {
                let v = t[k];
                if pinch.contains(&v) {
                    fans.entry(v).or_default().push((t[(k + 1) % 3], t[(k + 2) % 3]));
                }
            }
        }
    }
    let mut succ: Vec<Option<usize>> = vec![None; fill.len()];
    for (e, &(_, v)) in fill.iter().enumerate() {
        if pinch.contains(&v) {
            continue;
        }
        succ[e] = outs.get(&v).and_then(|o| o.first().copied());
    }
    for &v in &pinch {
        let i_list: &[usize] = ins.get(&v).map_or(&[], |x| x.as_slice());
        let o_list: &[usize] = outs.get(&v).map_or(&[], |x| x.as_slice());
        let fan = fans.get(&v).map_or(&[][..], |f| f.as_slice());
        let pairs = pair_at_vertex(m, v, i_list, o_list, fan, &fill).unwrap_or_else(|| {
            // the triangles do not tell (a non-manifold vertex): any pairing
            i_list.iter().zip(o_list).map(|(&i, &o)| (i, o)).collect()
        });
        for (i, o) in pairs {
            succ[i] = Some(o);
        }
    }
    let mut visited = vec![false; fill.len()];
    let mut loops = Vec::new();
    for start in 0..fill.len() {
        if visited[start] {
            continue;
        }
        let mut path = Vec::new();
        let mut cur = start;
        let closed = loop {
            visited[cur] = true;
            path.push(cur);
            match succ[cur] {
                Some(n) if n == start => break true,
                Some(n) if !visited[n] => cur = n,
                _ => break false,
            }
        };
        if closed && path.len() >= 3 {
            loops.push(path.iter().map(|&e| fill[e].0).collect());
        }
    }
    loops
}

/// Which open edge continues which at a vertex `v` where holes meet. The triangles round `v` form sectors; a sector starts with an open edge that ends at
/// `v` (in patch order) and ends with one that starts there. The hole between a sector and the next one (counter-clockwise round `v`) is entered at the start
/// of the next sector and left at the end of the previous one. `fan` holds (p, q) of every triangle (v, p, q). `None` when it does not work out.
fn pair_at_vertex(
    m: &TriMesh,
    v: u32,
    ins: &[usize],
    outs: &[usize],
    fan: &[(u32, u32)],
    fill: &[(u32, u32)],
) -> Option<Vec<(usize, usize)>> {
    let pv = m.vertex(v);
    // the normal at the vertex: the sum of the normals of the triangles round it
    let mut normal = [0.0; 3];
    for &(p, q) in fan {
        let c = cross(sub(m.vertex(p), pv), sub(m.vertex(q), pv));
        normal = crate::add(normal, c);
    }
    let normal = unit(normal);
    let mut sectors: Vec<(f64, usize, usize)> = Vec::new();
    for &e in ins {
        let u = fill[e].0;
        // the triangle of the open edge u -> v runs v -> u, so it is (v, u, w); turn on from the edge v -> w
        let mut q = fan.iter().find(|&&(p, _)| p == u)?.1;
        let mut end = None;
        for _ in 0..=fan.len() {
            match fan.iter().find(|&&(p, _)| p == q) {
                Some(&(_, r)) => q = r,
                None => {
                    end = outs.iter().copied().find(|&o| fill[o].1 == q);
                    break;
                }
            }
        }
        let d = sub(m.vertex(u), pv);
        let reference = if normal[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let x = unit(cross(normal, cross(reference, normal)));
        let y = cross(normal, x);
        sectors.push((dot(d, y).atan2(dot(d, x)), e, end?));
    }
    sectors.sort_by(|a, b| a.0.total_cmp(&b.0));
    let k = sectors.len();
    let pairs: Vec<(usize, usize)> = (0..k)
        .map(|i| (sectors[i].1, sectors[(i + k - 1) % k].2))
        .collect();
    let distinct: HashSet<usize> = pairs.iter().map(|p| p.1).collect();
    (distinct.len() == k && k == outs.len()).then_some(pairs)
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// the shape of a rim
// ---------------------------------------------------------------------------------------------------------------------------------------------------------

fn analyse(m: &TriMesh, ids: Vec<u32>) -> Rim {
    let pts: Vec<V3> = ids.iter().map(|&i| m.vertex(i)).collect();
    let n = pts.len();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    let pinched = sorted.windows(2).any(|w| w[0] == w[1]);
    let centre = mul(
        pts.iter().fold([0.0; 3], |s, &p| crate::add(s, p)),
        1.0 / n as f64,
    );
    // the vector area of the polygon (Newell): its direction is the side the patch faces
    let mut newell = [0.0; 3];
    let mut perimeter = 0.0;
    for k in 0..n {
        let (a, b) = (sub(pts[k], centre), sub(pts[(k + 1) % n], centre));
        let c = cross(a, b);
        for q in 0..3 {
            newell[q] += 0.5 * c[q];
        }
        perimeter += len(sub(pts[(k + 1) % n], pts[k]));
    }
    let mut normal = best_fit_normal(&pts, centre);
    if dot(normal, newell) < 0.0 {
        normal = mul(normal, -1.0);
    }
    if len(normal) < 0.5 {
        normal = unit(newell);
    }
    let axis = {
        let a = [normal[0].abs(), normal[1].abs(), normal[2].abs()];
        if a[0] <= a[1] && a[0] <= a[2] {
            [1.0, 0.0, 0.0]
        } else if a[1] <= a[2] {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        }
    };
    let u = unit(cross(normal, axis));
    let v = cross(normal, u);
    let planarity = pts
        .iter()
        .map(|&p| dot(sub(p, centre), normal).abs())
        .fold(0.0, f64::max);
    let flat: Vec<[f64; 2]> = pts
        .iter()
        .map(|&p| {
            let d = sub(p, centre);
            [dot(d, u), dot(d, v)]
        })
        .collect();
    let area = dot(newell, normal).max(0.0);
    let info = HoleInfo {
        vertices: n,
        perimeter,
        area,
        planarity,
        diameter: 2.0 * bounding_radius(&pts),
    };
    Rim {
        ids,
        info,
        frame: Frame {
            centre,
            n: normal,
            u,
            v,
        },
        flat,
        pinched,
    }
}

/// The unit normal of the least-squares plane through the points (the eigenvector of the smallest eigenvalue of their covariance); zero when there are no points.
fn best_fit_normal(pts: &[V3], centre: V3) -> V3 {
    let mut c = [[0.0f64; 3]; 3];
    for &p in pts {
        let d = sub(p, centre);
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] += d[i] * d[j];
            }
        }
    }
    let (vals, vecs) = jacobi3(c);
    let mut k = 0;
    for i in 1..3 {
        if vals[i] < vals[k] {
            k = i;
        }
    }
    unit([vecs[0][k], vecs[1][k], vecs[2][k]])
}

/// Eigenvalues and eigenvectors (as columns) of a symmetric 3 x 3 matrix by the cyclic Jacobi method.
fn jacobi3(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        let diag = a[0][0].abs() + a[1][1].abs() + a[2][2].abs();
        if off <= 1e-15 * diag.max(1e-300) {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for k in 0..3 {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp - s * akq;
                a[k][q] = s * akp + c * akq;
            }
            for k in 0..3 {
                let (apk, aqk) = (a[p][k], a[q][k]);
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for k in 0..3 {
                let (vkp, vkq) = (v[k][p], v[k][q]);
                v[k][p] = c * vkp - s * vkq;
                v[k][q] = s * vkp + c * vkq;
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2]], v)
}

/// The radius of a sphere that holds all the points (Ritter's method: a little bigger than the smallest one at most).
fn bounding_radius(pts: &[V3]) -> f64 {
    let far = |from: V3| {
        pts.iter()
            .copied()
            .max_by(|a, b| len(sub(*a, from)).total_cmp(&len(sub(*b, from))))
            .unwrap_or(from)
    };
    let a = far(pts[0]);
    let b = far(a);
    let mut c = mul(crate::add(a, b), 0.5);
    let mut r = 0.5 * len(sub(a, b));
    for _ in 0..2 {
        for &p in pts {
            let d = len(sub(p, c));
            if d > r {
                let nr = 0.5 * (r + d);
                c = crate::add(c, mul(sub(p, c), (nr - r) / d));
                r = nr;
            }
        }
    }
    r
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// the triangulation
// ---------------------------------------------------------------------------------------------------------------------------------------------------------

/// The sine of the angle at a vertex below which the vertex counts as lying on a straight line with its neighbours (no ear is cut at such a vertex while
/// another is possible, so no sliver is made, but the vertex stays a corner of the patch, as the neighbouring triangles need it).
const FLAT: f64 = 1e-4;

/// Triangulates the rim in its plane by ear clipping, the best ear (the roundest triangle) first. `existing` are the edges the mesh has between rim vertices:
/// no diagonal may repeat one. `None` when no clean triangulation was found. The triangles run along the loop (patch order).
fn triangulate(r: &Rim, existing: &HashSet<u64>) -> Option<Vec<[u32; 3]>> {
    let n = r.flat.len();
    let p = &r.flat;
    let scale2 = {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for q in p {
            for k in 0..2 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2)).max(1e-300)
    };
    let tol = 1e-9 * scale2;
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    let cr = |a: usize, b: usize, c: usize| {
        (p[b][0] - p[a][0]) * (p[c][1] - p[a][1]) - (p[b][1] - p[a][1]) * (p[c][0] - p[a][0])
    };
    let d2 = |a: usize, b: usize| (p[a][0] - p[b][0]).powi(2) + (p[a][1] - p[b][1]).powi(2);
    // the sine of the angle at b (positive when the turn there is to the left, which is convex)
    let sine = |a: usize, b: usize, c: usize| cr(b, c, a) / (d2(a, b) * d2(b, c)).sqrt().max(1e-300);
    let quality = |a: usize, b: usize, c: usize| {
        2.0 * 3f64.sqrt() * cr(a, b, c) / (d2(a, b) + d2(b, c) + d2(c, a)).max(1e-300)
    };
    // is the ear (a, b, c) clean: convex, no other vertex in it, no diagonal along an edge of the mesh
    let ear = |alive: &[bool],
               prev: &[usize],
               next: &[usize],
               left: usize,
               a: usize,
               b: usize,
               c: usize|
     -> Option<f64> {
        if sine(a, b, c) <= FLAT {
            return None;
        }
        if left > 3 && existing.contains(&edge_key(r.ids[a], r.ids[c])) {
            return None;
        }
        for j in 0..n {
            if !alive[j] || j == a || j == b || j == c {
                continue;
            }
            // only a vertex that is not convex can be inside the triangle
            if sine(prev[j], j, next[j]) > FLAT {
                continue;
            }
            if d2(j, a) < tol || d2(j, b) < tol || d2(j, c) < tol {
                continue;
            }
            if cr(a, b, j) >= -tol && cr(b, c, j) >= -tol && cr(c, a, j) >= -tol {
                return None;
            }
        }
        Some(quality(a, b, c))
    };
    let mut score: Vec<Option<f64>> = (0..n)
        .map(|i| ear(&alive, &prev, &next, n, prev[i], i, next[i]))
        .collect();
    let mut left = n;
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(n - 2);
    let mut fresh = true;
    while left > 3 {
        let mut best: Option<usize> = None;
        for i in 0..n {
            if alive[i] && score[i].is_some_and(|s| best.is_none_or(|b| s > score[b].unwrap_or(f64::MIN))) {
                best = Some(i);
            }
        }
        let i = match best {
            Some(i) => i,
            None if !fresh => {
                // the ears may have been freed by the vertices clipped since
                for k in 0..n {
                    score[k] = if alive[k] {
                        ear(&alive, &prev, &next, left, prev[k], k, next[k])
                    } else {
                        None
                    };
                }
                fresh = true;
                continue;
            }
            None => return None,
        };
        let (a, c) = (prev[i], next[i]);
        if cr(a, i, c) <= tol {
            return None;
        }
        out.push([r.ids[a], r.ids[i], r.ids[c]]);
        alive[i] = false;
        score[i] = None;
        next[a] = c;
        prev[c] = a;
        left -= 1;
        if left > 3 {
            for k in [a, c] {
                score[k] = ear(&alive, &prev, &next, left, prev[k], k, next[k]);
            }
        }
        fresh = false;
    }
    let a = (0..n).find(|&i| alive[i])?;
    let (b, c) = (next[a], next[next[a]]);
    if cr(a, b, c) <= tol {
        return None;
    }
    out.push([r.ids[a], r.ids[b], r.ids[c]]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::check;
    use crate::gen::{box_mesh, plate_with_hole, sphere};
    use std::time::Instant;

    /// the area of the triangles that were added (those after `from`), as the sum of their normal vectors' lengths
    fn added_area(m: &TriMesh, from: usize) -> f64 {
        (from..m.triangles.len()).map(|t| m.normal_area(t).1).sum()
    }

    fn opts() -> FillOptions {
        FillOptions {
            max_diameter: 20.0,
            ..Default::default()
        }
    }

    fn remove(m: &mut TriMesh, which: &[usize]) {
        let mut k = 0;
        m.triangles.retain(|_| {
            k += 1;
            !which.contains(&(k - 1))
        });
    }

    /// the triangles of the box face with the given outward normal axis and sign
    fn face_triangles(m: &TriMesh, axis: usize, positive: bool, at: f32) -> Vec<usize> {
        (0..m.triangles.len())
            .filter(|&t| {
                m.triangles[t]
                    .iter()
                    .all(|&v| (m.positions[v as usize][axis] - at).abs() < 1e-6)
                    && (m.normal_area(t).0[axis] > 0.0) == positive
            })
            .collect()
    }

    #[test]
    fn one_triangle_hole() {
        let mut m = box_mesh(10.0, 10.0, 10.0, 1);
        let vol = m.signed_volume();
        let top = face_triangles(&m, 2, true, 10.0);
        remove(&mut m, &top[..1]);
        assert_eq!(check(&m).open_edges, 3);
        let h = find_holes(&m);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0.len(), 3);
        assert!((h[0].1.area - 50.0).abs() < 1e-9);
        assert!(h[0].1.planarity < 1e-9);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!((rep.filled, rep.added_triangles), (1, 1));
        assert!(rep.left.is_empty());
        assert!(check(&m).is_clean());
        assert!((m.signed_volume() - vol).abs() < 1e-6);
    }

    #[test]
    fn missing_square_face_is_filled_with_two_triangles() {
        let mut m = box_mesh(10.0, 8.0, 6.0, 1);
        let vol = m.signed_volume();
        let top = face_triangles(&m, 2, true, 6.0);
        assert_eq!(top.len(), 2);
        remove(&mut m, &top);
        let h = find_holes(&m);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0.len(), 4);
        assert!((h[0].1.area - 80.0).abs() < 1e-9);
        assert!((h[0].1.perimeter - 36.0).abs() < 1e-9);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!((rep.filled, rep.added_triangles), (1, 2));
        let c = check(&m);
        assert!(c.is_clean(), "{:?}", c.lines());
        assert!((m.signed_volume() - vol).abs() < 1e-6);
        assert_eq!(m.triangles.len(), 12);
    }

    #[test]
    fn missing_face_of_a_finer_box_with_many_rim_points() {
        // 4 x 4 squares on the top face removed: a loop of 16 vertices, 12 of them collinear with their neighbours
        let mut m = box_mesh(8.0, 8.0, 8.0, 4);
        let vol = m.signed_volume();
        let top = face_triangles(&m, 2, true, 8.0);
        assert_eq!(top.len(), 32);
        remove(&mut m, &top);
        let h = find_holes(&m);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0.len(), 16);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!((rep.filled, rep.added_triangles), (1, 14));
        let c = check(&m);
        assert!(c.is_clean(), "{:?}", c.lines());
        assert_eq!(c.degenerate, 0);
        assert!((m.signed_volume() - vol).abs() < 1e-6);
    }

    #[test]
    fn round_hole_in_a_flat_wall_is_plugged_flat() {
        // a plate whose round hole lost its wall: two rims of 24 points, the top one faces up and the bottom one down
        let (w, d, t, r, seg) = (30.0, 30.0, 5.0, 3.0, 24);
        let mut m = plate_with_hole(w, d, t, r, seg);
        let full = m.signed_volume();
        // the hole wall: the triangles whose three corners are at the radius r from the axis
        let wall: Vec<usize> = (0..m.triangles.len())
            .filter(|&k| {
                m.triangles[k].iter().all(|&v| {
                    let p = m.positions[v as usize];
                    (((p[0] as f64 - w / 2.0).powi(2) + (p[1] as f64 - d / 2.0).powi(2)).sqrt() - r).abs()
                        < 1e-4
                })
            })
            .collect();
        assert_eq!(wall.len(), 2 * seg);
        remove(&mut m, &wall);
        let holes = find_holes(&m);
        assert_eq!(holes.len(), 2);
        for (ids, info) in &holes {
            assert_eq!(ids.len(), seg);
            assert!(info.planarity < 1e-5);
            assert!((info.diameter - 2.0 * r).abs() < 1e-3, "{}", info.diameter);
            let poly = 0.5 * seg as f64 * r * r * (std::f64::consts::TAU / seg as f64).sin();
            assert!((info.area - poly).abs() < 1e-3);
        }
        let before = m.triangles.len();
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 2);
        assert_eq!(rep.added_triangles, 2 * (seg - 2));
        assert!(rep.left.is_empty());
        let c = check(&m);
        assert!(c.is_clean(), "{:?}", c.lines());
        // the plug is flat: the new triangles lie in the planes z = t and z = 0
        for k in before..m.triangles.len() {
            let (n, a) = m.normal_area(k);
            assert!(a > 1e-6);
            assert!(n[2].abs() > 0.999999);
            let z = m.vertex(m.triangles[k][0])[2];
            assert!(z.abs() < 1e-6 || (z - t).abs() < 1e-6);
            // outward: up on top, down at the bottom
            assert_eq!(n[2] > 0.0, z > 1.0);
        }
        // the plate with the plugs is the full block: the added area is the two discs
        let poly = 0.5 * seg as f64 * r * r * (std::f64::consts::TAU / seg as f64).sin();
        assert!((added_area(&m, before) - 2.0 * poly).abs() < 1e-3);
        let _ = full;
        assert!(
            (m.signed_volume() - w * d * t).abs() < 1e-2,
            "{}",
            m.signed_volume()
        );
    }

    #[test]
    fn the_volume_changes_by_the_plug_when_a_cap_is_added() {
        // a box whose top is open by a round hole: the plug at z = h adds h * area / 3 to the signed volume
        let mut m = plate_with_hole(20.0, 20.0, 4.0, 4.0, 24);
        let wall: Vec<usize> = (0..m.triangles.len())
            .filter(|&k| {
                m.triangles[k].iter().all(|&v| {
                    let p = m.positions[v as usize];
                    (((p[0] as f64 - 10.0).powi(2) + (p[1] as f64 - 10.0).powi(2)).sqrt() - 4.0).abs() < 1e-4
                })
            })
            .collect();
        remove(&mut m, &wall);
        let v0 = m.signed_volume();
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 2);
        // the top plug adds 4 * A / 3, the bottom (z = 0) nothing; together they close the solid
        let poly = 0.5 * 24.0 * 16.0 * (std::f64::consts::TAU / 24.0).sin();
        assert!((m.signed_volume() - v0 - 4.0 * poly / 3.0).abs() < 1e-3);
        assert!((m.signed_volume() - 20.0 * 20.0 * 4.0).abs() < 1e-2);
    }

    #[test]
    fn a_big_curved_hole_is_left_with_a_reason() {
        // the lower half of a ball of 21 mm radius is missing
        let mut m = sphere(21.0, 2000);
        let low: Vec<usize> = (0..m.triangles.len())
            .filter(|&t| {
                let c = m.corners(t);
                (c[0][2] + c[1][2] + c[2][2]) / 3.0 < 0.0
            })
            .collect();
        remove(&mut m, &low);
        // the loose vertices of the lower half do not matter
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        assert_eq!(rep.added_triangles, 0);
        assert_eq!(rep.left.len(), 1);
        let (info, why) = &rep.left[0];
        assert!(info.diameter > 40.0);
        assert!(why.contains("rim is curved") && why.contains("mm wide"), "{why}");
        assert!(why.contains("invent a surface"), "{why}");
        // the same hole is patched when everything is asked for
        let all = fill_all(&mut m);
        assert_eq!(all.filled, 1);
        assert!(check(&m).is_closed());
    }

    #[test]
    fn a_big_flat_hole_is_left_for_its_width() {
        let mut m = box_mesh(40.0, 40.0, 40.0, 1);
        let top = face_triangles(&m, 2, true, 40.0);
        remove(&mut m, &top);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        assert!(rep.left[0].1.contains("wide"), "{}", rep.left[0].1);
        let rep = fill_all(&mut m);
        assert_eq!((rep.filled, rep.added_triangles), (1, 2));
        assert!(check(&m).is_clean());
    }

    #[test]
    fn a_slightly_curved_small_rim_is_filled() {
        // a missing top whose far corners are lifted by 0.04 mm: still a small hole
        let mut m = box_mesh(6.0, 6.0, 6.0, 1);
        let top = face_triangles(&m, 2, true, 6.0);
        remove(&mut m, &top);
        for p in &mut m.positions {
            if p[2] > 5.0 && p[0] > 3.0 && p[1] > 3.0 {
                p[2] += 0.04;
            }
        }
        let h = find_holes(&m);
        assert!(h[0].1.planarity > 0.005 && h[0].1.planarity < 0.05);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 1);
        assert!(check(&m).is_clean());
        // 0.4 mm of curvature is too much for 8 mm
        let mut m = box_mesh(6.0, 6.0, 6.0, 1);
        let top = face_triangles(&m, 2, true, 6.0);
        remove(&mut m, &top);
        for p in &mut m.positions {
            if p[2] > 5.0 && p[0] > 3.0 && p[1] > 3.0 {
                p[2] += 0.6;
            }
        }
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        assert!(rep.left[0].1.contains("curved"), "{}", rep.left[0].1);
    }

    /// a closed prism standing on z = 0 with the given outline (counter-clockwise), height h, the top made of a fan from the first corner
    /// (only for a convex outline) -- the top is left out when `open_top`
    fn prism(outline: &[[f32; 2]], h: f32, open_top: bool, apex: usize) -> TriMesh {
        let n = outline.len() as u32;
        let mut m = TriMesh::default();
        for p in outline {
            m.positions.push([p[0], p[1], 0.0]);
        }
        for p in outline {
            m.positions.push([p[0], p[1], h]);
        }
        for k in 0..n {
            let j = (k + 1) % n;
            m.triangles.push([k, j, n + j]);
            m.triangles.push([k, n + j, n + k]);
        }
        // the bottom (and the top): a fan from the apex corner, which has to see all the others
        for k in 0..n as usize {
            let j = (k + 1) % n as usize;
            if k == apex || j == apex {
                continue;
            }
            m.triangles.push([apex as u32, j as u32, k as u32]);
            if !open_top {
                m.triangles.push([n + apex as u32, n + k as u32, n + j as u32]);
            }
        }
        m
    }

    #[test]
    fn a_rim_with_a_collinear_run() {
        // a hexagon-like outline with three points on the bottom edge and two on the right edge
        let outline = [
            [0.0, 0.0],
            [2.0, 0.0],
            [4.0, 0.0],
            [6.0, 0.0],
            [6.0, 2.0],
            [6.0, 4.0],
            [0.0, 4.0],
        ];
        let mut m = prism(&outline, 3.0, true, 6);
        let full = prism(&outline, 3.0, false, 6);
        let rep_c = check(&m);
        assert_eq!(rep_c.open_edges, 7);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!((rep.filled, rep.added_triangles), (1, 5));
        let c = check(&m);
        assert!(c.is_clean(), "{:?}", c.lines());
        assert_eq!(c.degenerate, 0);
        assert!((m.signed_volume() - full.signed_volume()).abs() < 1e-4);
        assert!((m.signed_volume() - 6.0 * 4.0 * 3.0).abs() < 1e-4);
    }

    #[test]
    fn a_concave_l_shaped_rim() {
        // an L: 6 x 6 with the corner 3 x 3 cut away; the bottom fan from corner 0 sees every corner of the L
        let outline = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 3.0],
            [3.0, 3.0],
            [3.0, 6.0],
            [0.0, 6.0],
        ];
        let mut m = prism(&outline, 2.0, true, 0);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!((rep.filled, rep.added_triangles), (1, 4));
        let c = check(&m);
        assert!(c.is_clean(), "{:?}", c.lines());
        assert!((m.signed_volume() - 27.0 * 2.0).abs() < 1e-4);
        // the patch covers exactly the L: its area is 27 and no triangle lies in the cut-away corner
        let top = &m.triangles[m.triangles.len() - 4..];
        let mut area = 0.0;
        for t in top {
            let c = [m.vertex(t[0]), m.vertex(t[1]), m.vertex(t[2])];
            let n = cross(sub(c[1], c[0]), sub(c[2], c[0]));
            assert!(n[2] > 0.0);
            area += n[2] / 2.0;
            let mid = [
                (c[0][0] + c[1][0] + c[2][0]) / 3.0,
                (c[0][1] + c[1][1] + c[2][1]) / 3.0,
            ];
            assert!(
                !(mid[0] > 3.0 && mid[1] > 3.0),
                "a triangle in the cut-away corner"
            );
        }
        assert!((area - 27.0).abs() < 1e-6);
    }

    #[test]
    fn two_holes_one_filled_and_one_left() {
        let mut m = box_mesh(30.0, 30.0, 30.0, 1);
        let vol = m.signed_volume();
        // the small one: one triangle of the front would give a 3-gon of 30 mm; take a finer box instead
        m = box_mesh(30.0, 30.0, 30.0, 6);
        let top = face_triangles(&m, 2, true, 30.0);
        // one square of the top (5 mm) and the whole bottom face (30 mm)
        let square = [top[0], top[1]];
        remove(&mut m, &square);
        let bottom = face_triangles(&m, 2, false, 0.0);
        assert_eq!(bottom.len(), 72);
        remove(&mut m, &bottom);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 1);
        assert_eq!(rep.added_triangles, 2);
        assert_eq!(rep.left.len(), 1);
        assert!(rep.left[0].0.diameter > 30.0);
        assert!(rep.left[0].1.contains("wide"));
        let c = check(&m);
        assert_eq!(c.open_edges, 24);
        assert_eq!(c.inconsistent_edges, 0);
        // the rest is a patch of the bottom: closes with fill_all
        let rep = fill_all(&mut m);
        assert_eq!(rep.filled, 1);
        assert!(check(&m).is_clean());
        assert!((m.signed_volume() - vol).abs() < 1e-3);
    }

    #[test]
    fn a_mesh_without_hole_loops_is_not_touched() {
        let mut m = TriMesh::default();
        assert_eq!(fill_holes(&mut m, &FillOptions::default()), FillReport::default());
        // a closed box
        let mut m = box_mesh(5.0, 5.0, 5.0, 2);
        let before = m.clone();
        let rep = fill_holes(&mut m, &FillOptions::default());
        assert_eq!(rep, FillReport::default());
        assert_eq!(m, before);
        assert!(find_holes(&m).is_empty());
        // four triangles round one edge (a book with four pages): the shared edge is not open and never part of a rim; the call must not fail
        let mut m = TriMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, -1.0, 0.0],
                [0.0, 0.0, -1.0],
            ],
            triangles: vec![[0, 1, 2], [0, 1, 3], [0, 1, 4], [0, 1, 5]],
        };
        for (ids, _) in find_holes(&m) {
            let on_shared = (0..ids.len()).any(|k| {
                let (a, b) = (ids[k], ids[(k + 1) % ids.len()]);
                a.min(b) == 0 && a.max(b) == 1
            });
            assert!(!on_shared);
        }
        let _ = fill_holes(&mut m, &opts());
    }

    #[test]
    fn pinched_holes_are_split_by_the_triangles_round_the_vertex() {
        // a box top of 2 x 1 squares; remove the two squares that touch at one corner (a diagonal pair): two loops of 4 meeting in one vertex
        let mut m = box_mesh(2.0, 2.0, 2.0, 2);
        let vol = m.signed_volume();
        let top = face_triangles(&m, 2, true, 2.0);
        // squares (i, j) have triangles 2 * (i * 2 + j) and +1 in the generator's order
        let sq = |i: usize, j: usize| [top[2 * (i * 2 + j)], top[2 * (i * 2 + j) + 1]];
        let mut which = Vec::new();
        which.extend(sq(0, 0));
        which.extend(sq(1, 1));
        remove(&mut m, &which);
        let h = find_holes(&m);
        assert_eq!(h.len(), 2, "{h:?}");
        assert!(h.iter().all(|(ids, _)| ids.len() == 4));
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 2);
        let c = check(&m);
        assert!(c.is_clean(), "{:?}", c.lines());
        assert!((m.signed_volume() - vol).abs() < 1e-6);
    }

    #[test]
    fn an_island_in_a_hole_is_left() {
        // a ring of squares removed from a 3 x 3 top leaves the middle square as an island; two loops, nested
        let mut m = box_mesh(9.0, 9.0, 9.0, 3);
        let top = face_triangles(&m, 2, true, 9.0);
        let mut which = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                if (i, j) != (1, 1) {
                    which.push(top[2 * (i * 3 + j)]);
                    which.push(top[2 * (i * 3 + j) + 1]);
                }
            }
        }
        remove(&mut m, &which);
        let h = find_holes(&m);
        assert_eq!(h.len(), 2);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        assert_eq!(rep.left.len(), 2);
        assert!(rep.left.iter().any(|(_, w)| w.contains("island")));
        assert!(
            rep.left.iter().all(|(_, w)| w.contains("island")),
            "{:?}",
            rep.left
        );
        // fill_all leaves them too
        let rep = fill_all(&mut m);
        assert_eq!(rep.filled, 0);
    }

    #[test]
    fn a_loop_that_passes_a_vertex_twice_is_left() {
        // 3 x 3 squares on the top: only (0, 0) and (1, 1) stay; they touch at one vertex, so the hole is one loop that passes that vertex twice
        let mut m = box_mesh(9.0, 9.0, 9.0, 3);
        let top = face_triangles(&m, 2, true, 9.0);
        let mut which = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                if (i, j) != (0, 0) && (i, j) != (1, 1) {
                    which.extend([top[2 * (i * 3 + j)], top[2 * (i * 3 + j) + 1]]);
                }
            }
        }
        remove(&mut m, &which);
        let h = find_holes(&m);
        assert_eq!(h.len(), 1, "{h:?}");
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        assert!(rep.left[0].1.contains("touches itself"), "{}", rep.left[0].1);
        assert_eq!(fill_all(&mut m).filled, 0);
    }

    #[test]
    fn a_bow_tie_of_two_triangles_is_one_pinched_loop() {
        // two triangles that share only the vertex 0: the outside is one hole whose rim passes the vertex twice
        let mut m = TriMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [-1.0, 0.0, 0.0],
                [0.0, -1.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [0, 3, 4]],
        };
        let h = find_holes(&m);
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!(h[0].0.len(), 6);
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        assert!(rep.left[0].1.contains("touches itself"), "{}", rep.left[0].1);
    }

    #[test]
    fn the_orientation_stays_outward_for_every_face() {
        // remove one square from each of the six faces of a box and fill them all: every plug faces outward
        let mut m = box_mesh(12.0, 12.0, 12.0, 3);
        let vol = m.signed_volume();
        let mut which = Vec::new();
        for (axis, at) in [(0, 0.0), (0, 12.0), (1, 0.0), (1, 12.0), (2, 0.0), (2, 12.0)] {
            let f = face_triangles(&m, axis, at > 1.0, at);
            which.extend([f[8], f[9]]);
        }
        remove(&mut m, &which);
        assert_eq!(find_holes(&m).len(), 6);
        let before = m.triangles.len();
        let rep = fill_holes(&mut m, &opts());
        assert_eq!((rep.filled, rep.added_triangles), (6, 12));
        assert!(check(&m).is_clean());
        assert!(m.signed_volume() > 0.0);
        assert!((m.signed_volume() - vol).abs() < 1e-6);
        let centre = [6.0, 6.0, 6.0];
        for k in before..m.triangles.len() {
            let c = m.corners(k);
            let mid = mul(crate::add(crate::add(c[0], c[1]), c[2]), 1.0 / 3.0);
            assert!(dot(m.normal_area(k).0, sub(mid, centre)) > 0.0);
        }
    }

    #[test]
    fn fill_all_fans_a_rim_that_folds_over_itself() {
        // a bent rim whose projection in the best-fit plane is not a simple polygon: a closed "pillow" ring where the points alternate up and down
        // 12 points on a circle, z = +-3 alternating: planar enough in projection (a simple 12-gon), so it is triangulated; check it closes
        let mut m = TriMesh::default();
        let n = 12u32;
        for k in 0..n {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            m.positions.push([
                (5.0 * a.cos()) as f32,
                (5.0 * a.sin()) as f32,
                if k % 2 == 0 { 3.0 } else { -3.0 },
            ]);
        }
        // a tube wall from the zigzag ring down to a ring of 12 lower points
        for k in 0..n {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            m.positions
                .push([(5.0 * a.cos()) as f32, (5.0 * a.sin()) as f32, -10.0]);
        }
        for k in 0..n {
            let j = (k + 1) % n;
            m.triangles.push([k, n + k, n + j]);
            m.triangles.push([k, n + j, j]);
        }
        // the lower ring is closed by a fan from a new point
        m.positions.push([0.0, 0.0, -10.0]);
        let c = 2 * n;
        for k in 0..n {
            m.triangles.push([c, n + (k + 1) % n, n + k]);
        }
        let rep = fill_holes(&mut m, &opts());
        assert_eq!(rep.filled, 0);
        let rep = fill_all(&mut m);
        assert_eq!(rep.filled, 1);
        assert_eq!(rep.added_triangles, 10);
        assert!(check(&m).is_closed());
    }

    #[test]
    #[ignore]
    fn speed_of_a_big_mesh_with_many_small_holes() {
        // 6 * 2 * 204^2 = 499 392 triangles, 100 small holes (two triangles each)
        let mut m = box_mesh(100.0, 100.0, 100.0, 204);
        // the faces of the generator round their seams differently: weld them
        crate::check::merge_vertices(&mut m, 1e-4);
        assert!(check(&m).is_clean());
        let top = face_triangles(&m, 2, true, 100.0);
        let mut which = Vec::new();
        for k in 0..100 {
            let (i, j) = (3 + (k % 10) * 20, 3 + (k / 10) * 20);
            which.extend([top[2 * (i * 204 + j)], top[2 * (i * 204 + j) + 1]]);
        }
        remove(&mut m, &which);
        let n = m.triangles.len();
        let t = Instant::now();
        let h = find_holes(&m);
        let t_find = t.elapsed();
        assert_eq!(h.len(), 100);
        let t = Instant::now();
        let rep = fill_holes(&mut m, &opts());
        let t_fill = t.elapsed();
        assert_eq!(rep.filled, 100);
        assert_eq!(m.triangles.len(), n + 200);
        assert!(check(&m).is_clean());
        eprintln!("{n} triangles, 100 holes: find {t_find:?}, fill {t_fill:?}");
        assert!(t_fill.as_secs_f64() < 1.0, "{t_fill:?}");
    }
}
