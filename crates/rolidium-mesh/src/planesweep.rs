// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! "Stretch across a plane": the part of a closed mesh on the positive side of a plane moves by a distance along the normal and the gap is filled with the
//! cross-section of the mesh at the plane, swept along the normal. A box grows through its flat walls while the holes, pockets and rounded corners on each
//! side keep their size; an organic mesh grows by a straight piece cut out of its middle. A negative distance does the opposite: the layer of that thickness
//! below the plane is cut out and the two sides are joined (this needs the layer to be a straight slab, e.g. the part that was made by a stretch).
//!
//! The method: the vertices are classified by their signed distance to the plane (those within a hair of it are "on" the plane and are snapped to it; a triangle
//! that lies in the plane belongs to the positive side), every triangle that crosses the plane is split exactly along it (new vertices on the plane are shared
//! by both neighbours through an edge map), the boundary of the lower part in the plane is the cross-section, its vertices get a lower copy (stays) and an upper
//! copy (moves with the positive side) and every edge of the cross-section is joined by a quad of two triangles. Everything is computed in f64.

use crate::{add, check, dot, len, mul, sub, unit, TriMesh, V3};
use std::collections::HashMap;
use std::time::Instant;

/// The result of a stretch.
#[derive(Debug, Clone)]
pub struct SweepResult {
    pub mesh: TriMesh,
    /// The closed loops of the cross-section at the plane (for a negative distance: at the plane that is moved by the distance).
    pub loops: usize,
    /// How many more triangles the mesh has than before (0 when material was removed and the mesh got smaller).
    pub added_triangles: usize,
    pub seconds: f64,
}

/// The part may not reach farther than this (mm) from the origin: the f32 positions would lose their precision.
const MAX_COORD: f64 = 1e5;

const NO_CUT: &str = "The plane does not cut the mesh";
const OPEN: &str = "The mesh is not closed along the plane (open edges at the cut), repair it first";
const NOT_STRAIGHT: &str = "The cross-section changes inside the removed layer (it is not a straight slab), so the two sides cannot be joined";
const TOUCHING: &str = "The cross-section touches itself in a point, which this operation does not handle";

enum CutError {
    NoCut,
    Open,
}

impl CutError {
    fn text(&self) -> String {
        match self {
            CutError::NoCut => NO_CUT.into(),
            CutError::Open => OPEN.into(),
        }
    }
}

struct Plane {
    p: V3,
    n: V3,
    /// Vertices closer to the plane than this are on it.
    eps: f64,
}

fn vf(p: [f32; 3]) -> V3 {
    [p[0] as f64, p[1] as f64, p[2] as f64]
}

fn pf(v: V3) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

fn code(a: u32, b: u32) -> u64 {
    ((a as u64) << 32) | b as u64
}

fn rev(x: u64) -> u64 {
    x.rotate_right(32)
}

/// The cut of the mesh by one plane: what the triangles were split into and the edges that lie in the plane.
struct Cut {
    /// The number of vertices before the cut: the later ones are the new vertices on the plane.
    n0: usize,
    /// -1, 0 (on the plane) or 1 for each of the first `n0` vertices.
    sg: Vec<i8>,
    /// The edges (as the triangles of the lower side run them) in the plane that have no neighbour on the lower side: the cross-section.
    lower_b: Vec<(u32, u32)>,
    /// The same as seen from the upper side.
    upper_b: Vec<(u32, u32)>,
}

impl Cut {
    fn is_loop(&self, v: u32) -> bool {
        v as usize >= self.n0 || self.sg[v as usize] == 0
    }
}

fn cut_vertex(pos: &mut Vec<[f32; 3]>, cuts: &mut HashMap<u64, u32>, s: &[f64], a: u32, b: u32) -> u32 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    let key = code(lo, hi);
    if let Some(&i) = cuts.get(&key) {
        return i;
    }
    let (sa, sb) = (s[lo as usize], s[hi as usize]);
    let (pa, pb) = (vf(pos[lo as usize]), vf(pos[hi as usize]));
    let p = add(pa, mul(sub(pb, pa), sa / (sa - sb)));
    pos.push(pf(p));
    let id = (pos.len() - 1) as u32;
    cuts.insert(key, id);
    id
}

fn record(t: [u32; 3], is_loop: &impl Fn(u32) -> bool, out: &mut Vec<u64>) {
    for k in 0..3 {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        if is_loop(a) && is_loop(b) {
            out.push(code(a, b));
        }
    }
}

/// The directed edges that have no reverse among the edges: sorted.
fn boundary(mut e: Vec<u64>) -> Vec<u64> {
    e.sort_unstable();
    e.iter()
        .copied()
        .filter(|&x| e.binary_search(&rev(x)).is_err())
        .collect()
}

/// Splits the triangles that cross the plane. The upper pieces (and the triangles on the positive side) get the bit `bit` in `tags`.
fn cut_plane(
    pos: &mut Vec<[f32; 3]>,
    tris: &mut Vec<[u32; 3]>,
    tags: &mut Vec<u8>,
    bit: u8,
    pl: &Plane,
) -> Result<Cut, CutError> {
    let n0 = pos.len();
    let mut s = Vec::with_capacity(n0);
    let mut sg: Vec<i8> = Vec::with_capacity(n0);
    for p in pos.iter() {
        let v = dot(sub(vf(*p), pl.p), pl.n);
        s.push(v);
        sg.push(if v > pl.eps {
            1
        } else if v < -pl.eps {
            -1
        } else {
            0
        });
    }
    // the vertices on the plane are put exactly on it
    for v in 0..n0 {
        if sg[v] == 0 {
            pos[v] = pf(sub(vf(pos[v]), mul(pl.n, s[v])));
        }
    }
    let is_loop = |v: u32| v as usize >= n0 || sg[v as usize] == 0;
    let mut cuts: HashMap<u64, u32> = HashMap::new();
    let (mut lo, mut up): (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
    let nt = tris.len();
    for t in 0..nt {
        let tri = tris[t];
        let c = [sg[tri[0] as usize], sg[tri[1] as usize], sg[tri[2] as usize]];
        let (mn, mx) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
        if mn >= 0 || mx <= 0 {
            // the triangle is on one side (a triangle that lies in the plane is on the upper one)
            let upper = mn >= 0;
            if upper {
                tags[t] |= bit;
            }
            if c.iter().filter(|&&x| x == 0).count() >= 2 {
                record(tri, &is_loop, if upper { &mut up } else { &mut lo });
            }
            continue;
        }
        let mut pieces: [([u32; 3], bool); 3] = [([0; 3], false); 3];
        let np;
        if let Some(i) = c.iter().position(|&x| x == 0) {
            // one corner on the plane: the opposite edge is cut and the triangle is split through the corner
            let (vo, x, y) = (tri[i], tri[(i + 1) % 3], tri[(i + 2) % 3]);
            let m = cut_vertex(pos, &mut cuts, &s, x, y);
            pieces[0] = ([vo, x, m], c[(i + 1) % 3] > 0);
            pieces[1] = ([vo, m, y], c[(i + 2) % 3] > 0);
            np = 2;
        } else {
            // one corner alone on its side: a triangle for it and a quad (two triangles) for the other two
            let i = if c[0] == c[1] {
                2
            } else if c[0] == c[2] {
                1
            } else {
                0
            };
            let (a, b, d) = (tri[i], tri[(i + 1) % 3], tri[(i + 2) % 3]);
            let m1 = cut_vertex(pos, &mut cuts, &s, a, b);
            let m2 = cut_vertex(pos, &mut cuts, &s, d, a);
            let other = c[(i + 1) % 3] > 0;
            pieces[0] = ([a, m1, m2], c[i] > 0);
            pieces[1] = ([m1, b, d], other);
            pieces[2] = ([m1, d, m2], other);
            np = 3;
        }
        let old = tags[t];
        for (k, (pt, upper)) in pieces.iter().take(np).enumerate() {
            let tag = if *upper { old | bit } else { old };
            if k == 0 {
                tris[t] = *pt;
                tags[t] = tag;
            } else {
                tris.push(*pt);
                tags.push(tag);
            }
            record(*pt, &is_loop, if *upper { &mut up } else { &mut lo });
        }
    }
    let lb = boundary(lo);
    let ub = boundary(up);
    if lb.is_empty() && ub.is_empty() {
        return Err(CutError::NoCut);
    }
    let mut ur: Vec<u64> = ub.iter().map(|&x| rev(x)).collect();
    ur.sort_unstable();
    if ur != lb {
        return Err(CutError::Open);
    }
    // every point of the cross-section must be left as often as it is entered, or the loop is cut open (e.g. a missing triangle that the plane crosses)
    let mut from: Vec<u64> = lb.iter().map(|x| x >> 32).collect();
    let mut to: Vec<u64> = lb.iter().map(|x| x & 0xffff_ffff).collect();
    from.sort_unstable();
    to.sort_unstable();
    if from != to {
        return Err(CutError::Open);
    }
    let pair = |x: &u64| ((*x >> 32) as u32, *x as u32);
    Ok(Cut {
        n0,
        sg,
        lower_b: lb.iter().map(pair).collect(),
        upper_b: ub.iter().map(pair).collect(),
    })
}

/// The number of connected loops that the edges make.
fn count_loops(edges: &[(u32, u32)]) -> usize {
    let mut verts: Vec<u32> = edges.iter().flat_map(|e| [e.0, e.1]).collect();
    verts.sort_unstable();
    verts.dedup();
    let mut parent: Vec<usize> = (0..verts.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let idx = |v: u32| verts.binary_search(&v).unwrap_or(0);
    for &(a, b) in edges {
        let (ra, rb) = (find(&mut parent, idx(a)), find(&mut parent, idx(b)));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    (0..verts.len()).filter(|&i| find(&mut parent, i) == i).count()
}

fn make_plane(m: &TriMesh, point: V3, normal: V3) -> Result<Plane, String> {
    if point.iter().chain(normal.iter()).any(|x| !x.is_finite()) {
        return Err("The plane is not valid (a coordinate is not a number)".into());
    }
    if len(normal) < 1e-12 {
        return Err("The normal of the plane is zero".into());
    }
    let (lo, hi) = m
        .bounding_box()
        .filter(|_| !m.triangles.is_empty())
        .ok_or("The mesh is empty")?;
    let scale = (0..3)
        .map(|k| lo[k].abs().max(hi[k].abs()).max(point[k].abs()))
        .fold(1.0, f64::max);
    Ok(Plane {
        p: point,
        n: unit(normal),
        eps: 2e-7 * scale,
    })
}

/// Checks the arguments and the reach of the result.
fn prepare(m: &TriMesh, point: V3, normal: V3, distance: f64) -> Result<Plane, String> {
    if !distance.is_finite() {
        return Err("The distance is not a number".into());
    }
    let pl = make_plane(m, point, normal)?;
    if distance == 0.0 {
        return Err("The distance is zero, there is nothing to stretch".into());
    }
    let (lo, hi) = m.bounding_box().unwrap_or_default();
    let reach = (0..3).map(|k| lo[k].abs().max(hi[k].abs())).fold(0.0, f64::max);
    if reach + distance.abs() > MAX_COORD {
        return Err(
            "The distance is too large: the part would reach farther than 100 m from the origin".into(),
        );
    }
    Ok(pl)
}

/// Stretches the mesh across the plane through `point` with the normal `normal`: the part on the side the normal points to moves by `distance` along it, and the
/// gap is filled with the cross-section of the mesh at the plane swept along the normal. A negative `distance` cuts out the layer between the plane and the
/// plane moved by `distance` (it must be a straight slab) and joins the two sides.
///
/// The mesh must be closed along the plane. The refusals are plain English reasons.
pub fn sweep_in_plane(
    m: &TriMesh,
    point: [f64; 3],
    normal: [f64; 3],
    distance: f64,
) -> Result<SweepResult, String> {
    let t0 = Instant::now();
    let pl = prepare(m, point, normal, distance)?;
    let (mesh, loops) = if distance > 0.0 {
        positive(m, &pl, distance)?
    } else {
        negative(m, &pl, distance)?
    };
    Ok(SweepResult {
        added_triangles: mesh.triangles.len().saturating_sub(m.triangles.len()),
        mesh,
        loops,
        seconds: t0.elapsed().as_secs_f64(),
    })
}

/// A cheap check that a stretch is possible, without building the result (for a preview of the plane): the number of loops of the cross-section, or the
/// reason for the refusal (the same as `sweep_in_plane` gives). For a negative distance the stretch is made and thrown away.
pub fn preview_ok(m: &TriMesh, point: [f64; 3], normal: [f64; 3], distance: f64) -> Result<usize, String> {
    let pl = prepare(m, point, normal, distance)?;
    if distance > 0.0 {
        let mut pos = m.positions.clone();
        let mut tris = m.triangles.clone();
        let mut tags = vec![0u8; tris.len()];
        let cut = cut_plane(&mut pos, &mut tris, &mut tags, 1, &pl).map_err(|e| e.text())?;
        Ok(count_loops(&cut.lower_b))
    } else {
        negative(m, &pl, distance).map(|r| r.1)
    }
}

fn positive(m: &TriMesh, pl: &Plane, d: f64) -> Result<(TriMesh, usize), String> {
    let mut pos = m.positions.clone();
    let mut tris = m.triangles.clone();
    let mut tags = vec![0u8; tris.len()];
    let cut = cut_plane(&mut pos, &mut tris, &mut tags, 1, pl).map_err(|e| e.text())?;
    let dv = mul(pl.n, d);
    // the upper copies of the vertices of the cross-section, for the triangles of the upper side
    let mut up: HashMap<u32, u32> = HashMap::new();
    for (t, tri) in tris.iter_mut().enumerate() {
        if tags[t] & 1 == 0 {
            continue;
        }
        for v in tri.iter_mut() {
            if cut.is_loop(*v) {
                let next = pos.len() as u32;
                let u = *up.entry(*v).or_insert(next);
                if u == next {
                    pos.push(pf(add(vf(pos[*v as usize]), dv)));
                }
                *v = u;
            }
        }
    }
    for v in 0..cut.n0 {
        if cut.sg[v] > 0 {
            pos[v] = pf(add(vf(pos[v]), dv));
        }
    }
    // the swept wall: for the edge a -> b of the lower side the quad runs b -> a below and a -> b above
    for &(a, b) in &cut.lower_b {
        let (ua, ub) = match (up.get(&a), up.get(&b)) {
            (Some(&x), Some(&y)) => (x, y),
            _ => return Err(OPEN.into()),
        };
        tris.push([b, a, ua]);
        tris.push([b, ua, ub]);
    }
    let mut out = TriMesh {
        positions: pos,
        triangles: tris,
    };
    check::compact(&mut out);
    Ok((out, count_loops(&cut.lower_b)))
}

/// The edges as one map from vertex to the next vertex, and the closed loops they make.
fn cycles(edges: &[(u32, u32)]) -> Result<Vec<Vec<u32>>, String> {
    let mut next: HashMap<u32, u32> = HashMap::new();
    for &(a, b) in edges {
        if next.insert(a, b).is_some() {
            return Err(TOUCHING.into());
        }
    }
    let mut starts: Vec<u32> = next.keys().copied().collect();
    starts.sort_unstable();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in starts {
        if seen.contains(&s) {
            continue;
        }
        let mut c = vec![s];
        seen.insert(s);
        let mut v = s;
        loop {
            v = *next.get(&v).ok_or(OPEN)?;
            if v == s {
                break;
            }
            if !seen.insert(v) {
                return Err(TOUCHING.into());
            }
            c.push(v);
        }
        out.push(c);
    }
    Ok(out)
}

/// Is `p` within `tol` of the segment a-b (and not beyond its ends)?
fn on_segment(p: V3, a: V3, b: V3, tol: f64) -> bool {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    if l2 < 1e-24 {
        return false;
    }
    let t = dot(sub(p, a), ab) / l2;
    if !(-1e-9..=1.0 + 1e-9).contains(&t) {
        return false;
    }
    len(sub(p, add(a, mul(ab, t)))) <= tol
}

/// Replaces the triangles that have an edge with extra points on it (the points are in the order of the edge) by triangles that use the points.
fn expand(
    pos: &mut Vec<[f32; 3]>,
    tri: [u32; 3],
    ins: &HashMap<(u32, u32), Vec<u32>>,
    out: &mut Vec<[u32; 3]>,
) -> bool {
    let mut poly: Vec<u32> = Vec::new();
    let mut edges_with = 0;
    let mut which = 0;
    for k in 0..3 {
        poly.push(tri[k]);
        if let Some(list) = ins.get(&(tri[k], tri[(k + 1) % 3])) {
            poly.extend(list);
            edges_with += 1;
            which = k;
        }
    }
    if edges_with == 0 {
        return false;
    }
    if edges_with == 1 {
        // a fan from the corner opposite to the edge
        let opp = tri[(which + 2) % 3];
        let mut ring = vec![tri[which]];
        ring.extend(&ins[&(tri[which], tri[(which + 1) % 3])]);
        ring.push(tri[(which + 1) % 3]);
        for w in ring.windows(2) {
            out.push([w[0], w[1], opp]);
        }
    } else {
        // several edges: a fan from the middle
        let mid = poly.iter().fold([0.0; 3], |s, &v| add(s, vf(pos[v as usize])));
        pos.push(pf(mul(mid, 1.0 / poly.len() as f64)));
        let c = (pos.len() - 1) as u32;
        for k in 0..poly.len() {
            out.push([c, poly[k], poly[(k + 1) % poly.len()]]);
        }
    }
    true
}

fn negative(m: &TriMesh, pl: &Plane, d: f64) -> Result<(TriMesh, usize), String> {
    // the plane moved by the distance (below the plane) and the plane itself: the layer between them is cut out
    let low = Plane {
        p: add(pl.p, mul(pl.n, d)),
        n: pl.n,
        eps: pl.eps,
    };
    let mut pos = m.positions.clone();
    let mut tris = m.triangles.clone();
    let mut tags = vec![0u8; tris.len()];
    let top = cut_plane(&mut pos, &mut tris, &mut tags, 2, pl).map_err(|e| e.text())?;
    let bottom = cut_plane(&mut pos, &mut tris, &mut tags, 1, &low).map_err(|e| match e {
        CutError::NoCut => {
            "The removed layer reaches beyond the mesh (the plane moved by the distance does not cut it)"
                .to_string()
        }
        e => e.text(),
    })?;
    // tags: 0 below the layer, 1 inside it, 3 above it
    let dv = mul(pl.n, d);
    let mut moved = vec![false; pos.len()];
    for (t, tri) in tris.iter().enumerate() {
        if tags[t] & 2 != 0 {
            for &v in tri {
                moved[v as usize] = true;
            }
        }
    }
    for (v, mv) in moved.iter().enumerate() {
        if *mv {
            pos[v] = pf(add(vf(pos[v]), dv));
        }
    }
    // the loop of the lower side at the lower plane and the loop of the upper side at the upper plane (run the other way) must be the same curves
    let a_cycles = cycles(&bottom.lower_b)?;
    let rb: Vec<(u32, u32)> = top.upper_b.iter().map(|&(a, b)| (b, a)).collect();
    let b_cycles = cycles(&rb)?;
    let (lo, hi) = m.bounding_box().unwrap_or_default();
    let tol = 2e-5 * len(sub(hi, lo)).max(10.0);
    let cell = 2.0 * tol;
    let key = |p: V3| {
        [
            (p[0] / cell).floor() as i64,
            (p[1] / cell).floor() as i64,
            (p[2] / cell).floor() as i64,
        ]
    };
    let mut grid: HashMap<[i64; 3], Vec<(usize, usize)>> = HashMap::new();
    for (j, c) in b_cycles.iter().enumerate() {
        for (q, &v) in c.iter().enumerate() {
            grid.entry(key(vf(pos[v as usize]))).or_default().push((j, q));
        }
    }
    let find_b = |p: V3, used: &[bool]| -> Option<(usize, usize)> {
        let k = key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(l) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        for &(j, q) in l {
                            if !used[j] && len(sub(p, vf(pos[b_cycles[j][q] as usize]))) <= tol {
                                return Some((j, q));
                            }
                        }
                    }
                }
            }
        }
        None
    };
    if a_cycles.len() != b_cycles.len() {
        return Err(NOT_STRAIGHT.into());
    }
    let mut used = vec![false; b_cycles.len()];
    let mut weld: HashMap<u32, u32> = HashMap::new();
    // points to put on the edges of the lower side (a -> b as the triangle runs it) and of the upper side
    let mut ins_a: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    let mut ins_b: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for ac in &a_cycles {
        let k = ac.len();
        let (i, j, q) = (0..k)
            .find_map(|i| find_b(vf(pos[ac[i] as usize]), &used).map(|(j, q)| (i, j, q)))
            .ok_or(NOT_STRAIGHT)?;
        used[j] = true;
        let bc = &b_cycles[j];
        let mm = bc.len();
        weld.insert(bc[q], ac[i]);
        let (mut ia, mut jb, mut ca, mut cb) = (i, q, 0, 0);
        while ca < k && cb < mm {
            let (a0, a1) = (ac[ia], ac[(ia + 1) % k]);
            let (b0, b1) = (bc[jb], bc[(jb + 1) % mm]);
            let (pa0, pa1) = (vf(pos[a0 as usize]), vf(pos[a1 as usize]));
            let (pb0, pb1) = (vf(pos[b0 as usize]), vf(pos[b1 as usize]));
            if len(sub(pa1, pb1)) <= tol {
                weld.insert(b1, a1);
                ia = (ia + 1) % k;
                jb = (jb + 1) % mm;
                ca += 1;
                cb += 1;
            } else if on_segment(pa1, pb0, pb1, tol) {
                // a point of the lower loop on an edge of the upper loop
                ins_b.entry((b1, b0)).or_default().insert(0, a1);
                ia = (ia + 1) % k;
                ca += 1;
            } else if on_segment(pb1, pa0, pa1, tol) {
                ins_a.entry((a0, a1)).or_default().push(b1);
                jb = (jb + 1) % mm;
                cb += 1;
            } else {
                return Err(NOT_STRAIGHT.into());
            }
        }
        if ca != k || cb != mm {
            return Err(NOT_STRAIGHT.into());
        }
    }
    // the points go into the triangles (an upper edge b1 -> b0 runs the other way than the loop that was walked)
    let keep = |tag: u8| tag == 0 || tag & 2 != 0;
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(tris.len());
    let mut mark = vec![false; pos.len()];
    for (a, b) in ins_a.keys().chain(ins_b.keys()) {
        mark[*a as usize] = true;
        mark[*b as usize] = true;
    }
    let has_ins = !ins_a.is_empty() || !ins_b.is_empty();
    for (t, tri) in tris.iter().enumerate() {
        if !keep(tags[t]) {
            continue;
        }
        if has_ins && tri.iter().filter(|&&v| mark[v as usize]).count() >= 2 {
            let ins = if tags[t] == 0 { &ins_a } else { &ins_b };
            if expand(&mut pos, *tri, ins, &mut out) {
                continue;
            }
        }
        out.push(*tri);
    }
    let mut rename: Vec<u32> = (0..pos.len() as u32).collect();
    for (b, a) in &weld {
        rename[*b as usize] = *a;
    }
    for tri in &mut out {
        for v in tri.iter_mut() {
            *v = rename[*v as usize];
        }
    }
    out.retain(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
    let mut res = TriMesh {
        positions: pos,
        triangles: out,
    };
    check::compact(&mut res);
    Ok((res, a_cycles.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen;

    fn clean(m: &TriMesh) -> check::Report {
        let r = check::check(m);
        assert!(r.is_clean(), "not clean: {:?}", r.lines());
        r
    }

    /// A box whose seams are welded (the generator makes the points of the seams from each face separately, so they can differ in the last bit).
    fn boxm(w: f32, d: f32, h: f32, n: usize) -> TriMesh {
        let mut m = gen::box_mesh(w, d, h, n);
        check::merge_vertices(&mut m, 1e-4);
        m
    }

    fn size(m: &TriMesh) -> V3 {
        let (lo, hi) = m.bounding_box().unwrap();
        sub(hi, lo)
    }

    /// Ear clipping of a counter-clockwise polygon.
    fn triangulate(poly: &[[f64; 2]]) -> Vec<[usize; 3]> {
        let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
            (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
        };
        let mut idx: Vec<usize> = (0..poly.len()).collect();
        let mut out = Vec::new();
        while idx.len() > 3 {
            let n = idx.len();
            let ear = (0..n).find(|&i| {
                let (a, b, c) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
                if cross(poly[a], poly[b], poly[c]) <= 1e-12 {
                    return false;
                }
                !idx.iter().any(|&o| {
                    o != a
                        && o != b
                        && o != c
                        && cross(poly[a], poly[b], poly[o]) >= 0.0
                        && cross(poly[b], poly[c], poly[o]) >= 0.0
                        && cross(poly[c], poly[a], poly[o]) >= 0.0
                })
            });
            let i = ear.expect("no ear");
            out.push([idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]]);
            idx.remove(i);
        }
        out.push([idx[0], idx[1], idx[2]]);
        out
    }

    /// The polygon (counter-clockwise) extruded from z = 0 to z = h.
    fn prism(poly: &[[f64; 2]], h: f64) -> TriMesh {
        let n = poly.len();
        let mut m = TriMesh::default();
        for z in [0.0, h] {
            for p in poly {
                m.positions.push([p[0] as f32, p[1] as f32, z as f32]);
            }
        }
        for t in triangulate(poly) {
            m.triangles.push([t[0] as u32, t[2] as u32, t[1] as u32]);
            m.triangles
                .push([(t[0] + n) as u32, (t[1] + n) as u32, (t[2] + n) as u32]);
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let (bi, bj, ti, tj) = (i as u32, j as u32, (i + n) as u32, (j + n) as u32);
            m.triangles.push([bi, bj, tj]);
            m.triangles.push([bi, tj, ti]);
        }
        m
    }

    fn rounded_rect(w: f64, d: f64, r: f64, per: usize) -> Vec<[f64; 2]> {
        let mut p = Vec::new();
        let corners = [
            (w - r, r, -90.0),
            (w - r, d - r, 0.0),
            (r, d - r, 90.0),
            (r, r, 180.0),
        ];
        for (cx, cy, a0) in corners {
            for k in 0..=per {
                let a = (a0 + 90.0 * k as f64 / per as f64).to_radians();
                p.push([cx + r * a.cos(), cy + r * a.sin()]);
            }
        }
        p
    }

    #[test]
    fn box_grows_through_the_middle() {
        for n in [1, 3] {
            let m = boxm(10.0, 6.0, 4.0, n);
            let v0 = clean(&m).signed_volume;
            // x = 5 is between grid lines for n = 1 (diagonals are cut) and for n = 3; x = 10/3 is a grid line for n = 3 (vertices on the plane)
            for x in [5.0, 10.0 / 3.0] {
                let r = sweep_in_plane(&m, [x, 0.0, 0.0], [1.0, 0.0, 0.0], 5.0).unwrap();
                let rep = clean(&r.mesh);
                assert!(
                    (rep.signed_volume - (v0 + 6.0 * 4.0 * 5.0)).abs() < 1e-3,
                    "n {n} x {x}"
                );
                let s = size(&r.mesh);
                assert!((s[0] - 15.0).abs() < 1e-5 && (s[1] - 6.0).abs() < 1e-5 && (s[2] - 4.0).abs() < 1e-5);
                assert_eq!(r.loops, 1);
                assert_eq!(rep.components, 1);
                assert_eq!(r.added_triangles, r.mesh.triangles.len() - m.triangles.len());
                assert_eq!(preview_ok(&m, [x, 0.0, 0.0], [1.0, 0.0, 0.0], 5.0), Ok(1));
            }
        }
    }

    #[test]
    fn oblique_normal_is_normalised() {
        let m = boxm(10.0, 6.0, 4.0, 2);
        let r = sweep_in_plane(&m, [5.0, 3.0, 2.0], [0.0, 0.0, 7.0], 2.0).unwrap();
        let s = size(&r.mesh);
        assert!((s[2] - 6.0).abs() < 1e-5);
        clean(&r.mesh);
    }

    #[test]
    fn holes_keep_size_and_move_as_a_whole() {
        let (cell, t, rad, seg) = (10.0, 2.0, 3.0, 16);
        let m = gen::plate_with_holes(2, 1, cell, t, rad, seg);
        let v0 = clean(&m).signed_volume;
        let r = sweep_in_plane(&m, [cell, 5.0, 1.0], [1.0, 0.0, 0.0], 4.0).unwrap();
        let rep = clean(&r.mesh);
        assert!((rep.signed_volume - (v0 + cell * t * 4.0)).abs() < 1e-3);
        assert!((size(&r.mesh)[0] - 24.0).abs() < 1e-5);
        // every hole still has all its points on a circle of the same radius, the second one moved by 4
        for (cx, count) in [(5.0, 2 * seg), (19.0, 2 * seg)] {
            let on = r
                .mesh
                .positions
                .iter()
                .filter(|p| {
                    (((p[0] as f64 - cx).powi(2) + (p[1] as f64 - 5.0).powi(2)).sqrt() - rad).abs() < 1e-4
                })
                .count();
            assert_eq!(on, count, "hole at {cx}");
        }
    }

    #[test]
    fn plate_cut_through_a_hole_has_two_loops() {
        let m = gen::plate_with_hole(20.0, 10.0, 2.0, 3.0, 16);
        let v0 = clean(&m).signed_volume;
        let r = sweep_in_plane(&m, [10.0, 0.0, 0.0], [1.0, 0.0, 0.0], 3.0).unwrap();
        let rep = clean(&r.mesh);
        assert_eq!(r.loops, 2);
        // the section is 2 * (5 - 3) mm by 2 mm
        assert!((rep.signed_volume - (v0 + 8.0 * 3.0)).abs() < 1e-3);
        assert_eq!(rep.components, 1);
    }

    #[test]
    fn rounded_plate() {
        let m = prism(&rounded_rect(40.0, 20.0, 5.0, 6), 3.0);
        let v0 = clean(&m).signed_volume;
        for x in [20.0, 13.3] {
            let r = sweep_in_plane(&m, [x, 0.0, 0.0], [1.0, 0.0, 0.0], 7.5).unwrap();
            let rep = clean(&r.mesh);
            assert!(
                (rep.signed_volume - (v0 + 20.0 * 3.0 * 7.5)).abs() < 1e-2,
                "x {x}"
            );
            assert!((size(&r.mesh)[0] - 47.5).abs() < 1e-4);
        }
        // along the corners: the section is shorter there (x = 2 cuts the rounded corner)
        let r = sweep_in_plane(&m, [2.0, 0.0, 0.0], [1.0, 0.0, 0.0], 5.0).unwrap();
        clean(&r.mesh);
        assert!((size(&r.mesh)[0] - 45.0).abs() < 1e-4);
    }

    #[test]
    fn sphere_becomes_a_capsule() {
        let (rad, d) = (10.0, 6.0);
        let m = gen::sphere(rad, 2048);
        let v0 = clean(&m).signed_volume;
        let n = 32.0; // the sphere has n longitude points
        let area = n / 2.0 * rad * rad * (std::f64::consts::TAU / n).sin();
        for z in [0.0, 2.7] {
            let r = sweep_in_plane(&m, [0.0, 0.0, z], [0.0, 0.0, 1.0], d).unwrap();
            let rep = clean(&r.mesh);
            assert_eq!(r.loops, 1);
            assert!(size(&r.mesh)[2] - (2.0 * rad + d) < 1e-4);
            if z == 0.0 {
                assert!(
                    (rep.signed_volume - (v0 + area * d)).abs() < 1e-2,
                    "{} {}",
                    rep.signed_volume - v0,
                    area * d
                );
            }
        }
        // the volume grows by about pi r^2 d at the equator
        let r = sweep_in_plane(&m, [0.0; 3], [0.0, 0.0, 1.0], d).unwrap();
        let grow = r.mesh.signed_volume() - v0;
        assert!((grow / (std::f64::consts::PI * rad * rad * d) - 1.0).abs() < 0.01);
    }

    #[test]
    fn negative_distance_is_the_inverse() {
        let m = boxm(10.0, 6.0, 4.0, 1);
        let v0 = clean(&m).signed_volume;
        // a plain box: cut out a slab of 3 mm
        let r = sweep_in_plane(&m, [6.0, 0.0, 0.0], [1.0, 0.0, 0.0], -3.0).unwrap();
        let rep = clean(&r.mesh);
        assert!((rep.signed_volume - (v0 - 6.0 * 4.0 * 3.0)).abs() < 1e-3);
        assert!((size(&r.mesh)[0] - 7.0).abs() < 1e-5);
        assert_eq!(r.loops, 1);
        // stretch and shrink inside the stretched part (the diagonals of the swept wall are cut at different places)
        let g = boxm(10.0, 6.0, 4.0, 2);
        let s = sweep_in_plane(&g, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 5.0)
            .unwrap()
            .mesh;
        let back = sweep_in_plane(&s, [9.0, 0.0, 0.0], [1.0, 0.0, 0.0], -3.0).unwrap();
        let rep = clean(&back.mesh);
        assert!((rep.signed_volume - (v0 + 6.0 * 4.0 * 2.0)).abs() < 1e-3);
        assert!((size(&back.mesh)[0] - 12.0).abs() < 1e-5);
        // all of the swept part removed gives the original size
        let back = sweep_in_plane(&s, [10.0, 0.0, 0.0], [1.0, 0.0, 0.0], -5.0).unwrap();
        clean(&back.mesh);
        assert!((size(&back.mesh)[0] - 10.0).abs() < 1e-5);
        // plate with holes: the layer between the holes is straight
        let p = gen::plate_with_holes(3, 1, 10.0, 2.0, 3.0, 16);
        let vp = clean(&p).signed_volume;
        let r = sweep_in_plane(&p, [10.0, 5.0, 1.0], [1.0, 0.0, 0.0], -10.0);
        assert!(r.is_err() || clean(&r.unwrap().mesh).signed_volume < vp);
        // a sphere has no straight layers
        let sp = gen::sphere(10.0, 800);
        let e = sweep_in_plane(&sp, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0], -3.0).unwrap_err();
        assert!(e.contains("straight slab"), "{e}");
        // the layer reaches below the mesh
        let e = sweep_in_plane(&m, [3.0, 0.0, 0.0], [1.0, 0.0, 0.0], -5.0).unwrap_err();
        assert!(e.contains("beyond the mesh"), "{e}");
    }

    #[test]
    fn plane_that_misses_the_mesh() {
        let m = boxm(10.0, 6.0, 4.0, 2);
        for p in [[11.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
            let e = sweep_in_plane(&m, p, [1.0, 0.0, 0.0], 2.0).unwrap_err();
            assert!(e.contains("does not cut"), "{e}");
            assert!(preview_ok(&m, p, [1.0, 0.0, 0.0], 2.0).is_err());
        }
    }

    #[test]
    fn plane_through_a_flat_wall() {
        let m = boxm(10.0, 6.0, 4.0, 3);
        let v0 = clean(&m).signed_volume;
        // the wall at x = 10 looks outwards: the box grows
        let r = sweep_in_plane(&m, [10.0, 3.0, 2.0], [1.0, 0.0, 0.0], 3.0).unwrap();
        let rep = clean(&r.mesh);
        assert!((rep.signed_volume - (v0 + 72.0)).abs() < 1e-3);
        assert!((size(&r.mesh)[0] - 13.0).abs() < 1e-5);
        // and the wall at x = 0 with the normal pointing away from the box
        let r = sweep_in_plane(&m, [0.0, 3.0, 2.0], [-1.0, 0.0, 0.0], 2.0).unwrap();
        clean(&r.mesh);
        assert!((size(&r.mesh)[0] - 12.0).abs() < 1e-5);
        let (lo, _) = r.mesh.bounding_box().unwrap();
        assert!((lo[0] + 2.0).abs() < 1e-5);
        // a negative distance at the wall at x = 10 takes a layer off the box
        let r = sweep_in_plane(&m, [10.0, 3.0, 2.0], [1.0, 0.0, 0.0], -4.0).unwrap();
        clean(&r.mesh);
        assert!((size(&r.mesh)[0] - 6.0).abs() < 1e-5);
        // the wall that the normal points into has nothing on the other side
        let e = sweep_in_plane(&m, [0.0, 3.0, 2.0], [1.0, 0.0, 0.0], 2.0).unwrap_err();
        assert!(e.contains("does not cut"), "{e}");
    }

    #[test]
    fn two_loops_for_a_u_shape() {
        let u = [
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 20.0],
            [20.0, 20.0],
            [20.0, 8.0],
            [10.0, 8.0],
            [10.0, 20.0],
            [0.0, 20.0],
        ];
        let m = prism(&u, 5.0);
        let v0 = clean(&m).signed_volume;
        let r = sweep_in_plane(&m, [0.0, 14.0, 0.0], [0.0, 1.0, 0.0], 3.0).unwrap();
        assert_eq!(r.loops, 2);
        let rep = clean(&r.mesh);
        assert!((rep.signed_volume - (v0 + 20.0 * 5.0 * 3.0)).abs() < 1e-3);
        assert!((size(&r.mesh)[1] - 23.0).abs() < 1e-5);
        assert_eq!(preview_ok(&m, [0.0, 14.0, 0.0], [0.0, 1.0, 0.0], 3.0), Ok(2));
        // through the bottom of the U there is one loop
        let r = sweep_in_plane(&m, [0.0, 4.0, 0.0], [0.0, 1.0, 0.0], 3.0).unwrap();
        assert_eq!(r.loops, 1);
        clean(&r.mesh);
        // and back
        let back = sweep_in_plane(&r.mesh, [0.0, 6.0, 0.0], [0.0, 1.0, 0.0], -2.0).unwrap();
        clean(&back.mesh);
        assert!((size(&back.mesh)[1] - 21.0).abs() < 1e-5);
    }

    #[test]
    fn an_open_mesh_is_refused() {
        let mut m = boxm(10.0, 6.0, 4.0, 3);
        // take away a triangle of a side wall that the plane x = 5 crosses
        let t = (0..m.triangles.len())
            .find(|&t| {
                let c = m.corners(t);
                c.iter().all(|p| p[1] == 0.0) && c.iter().any(|p| p[0] < 5.0) && c.iter().any(|p| p[0] > 5.0)
            })
            .unwrap();
        m.triangles.remove(t);
        let e = sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 2.0).unwrap_err();
        assert!(e.contains("not closed"), "{e}");
        assert!(preview_ok(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 2.0).is_err());
        // a hole far from the plane does not matter
        let mut m = boxm(10.0, 6.0, 4.0, 3);
        let t = (0..m.triangles.len())
            .find(|&t| m.corners(t).iter().all(|p| p[0] == 0.0))
            .unwrap();
        m.triangles.remove(t);
        assert!(sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 2.0).is_ok());
    }

    #[test]
    fn huge_and_silly_distances() {
        let m = boxm(10.0, 6.0, 4.0, 1);
        let r = sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 20000.0).unwrap();
        clean(&r.mesh);
        assert!((size(&r.mesh)[0] - 20010.0).abs() < 0.01);
        assert!(sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1e7)
            .unwrap_err()
            .contains("too large"));
        assert!(sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], -1e7).is_err());
        assert!(sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], f64::NAN).is_err());
        assert!(sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.0).is_err());
        assert!(sweep_in_plane(&m, [5.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0).is_err());
        assert!(sweep_in_plane(&TriMesh::default(), [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0).is_err());
        // a tiny distance
        let r = sweep_in_plane(&m, [5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1e-3).unwrap();
        clean(&r.mesh);
    }

    #[test]
    #[ignore]
    fn timing_big_meshes() {
        for n in [204usize, 408] {
            let m = boxm(100.0, 60.0, 40.0, n);
            let tri = m.triangles.len();
            let r = sweep_in_plane(&m, [41.3, 0.0, 0.0], [1.0, 0.0, 0.0], 10.0).unwrap();
            let rep = check::check(&r.mesh);
            assert!(rep.is_clean());
            let back = sweep_in_plane(&r.mesh, [46.0, 0.0, 0.0], [1.0, 0.0, 0.0], -4.0).unwrap();
            assert!(check::check(&back.mesh).is_clean());
            println!(
                "{tri} triangles: stretch {:.3} s, shrink {:.3} s, preview {:.3} s",
                r.seconds,
                back.seconds,
                {
                    let t = Instant::now();
                    preview_ok(&m, [41.3, 0.0, 0.0], [1.0, 0.0, 0.0], 10.0).unwrap();
                    t.elapsed().as_secs_f64()
                }
            );
        }
        let m = gen::sphere(50.0, 2_000_000);
        let r = sweep_in_plane(&m, [0.0, 0.0, 3.0], [0.0, 0.0, 1.0], 20.0).unwrap();
        assert!(check::check(&r.mesh).is_clean());
        println!(
            "sphere {} triangles: stretch {:.3} s",
            m.triangles.len(),
            r.seconds
        );
    }
}
