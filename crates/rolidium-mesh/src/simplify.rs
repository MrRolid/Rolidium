// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Simplify mesh: fewer triangles for the same shape. Edge collapse driven by quadric error metrics (Garland and Heckbert, 1997) with a priority queue
//! whose entries are checked lazily, an indexed adjacency (no work proportional to the size of the mesh per collapse) and the guards that keep the
//! result a valid mesh: the link condition (the topology and so the genus do not change, no new non-manifold edge), no flipped and no degenerate triangle.
//!
//! With `keep_features` the sharp edges (dihedral angle over a limit), the borders of the surface and the rims of holes never leave their line: their
//! vertices are collapsed only along the line and only while the line stays straight within a tolerance, so a big flat face loses almost all its
//! triangles while its outline stays where it is. The calculation is done in `f64` on coordinates shifted to the middle of the box of the mesh.

use crate::check::{runs, sorted_edges};
use crate::{add, cross, dot, len, mul, sub, unit, TriMesh, V3};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

/// How far to simplify.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// Keep this percentage of the triangles (1 to 100).
    Percent(f64),
    /// Keep at most this number of triangles.
    Triangles(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub target: Target,
    /// Sharp edges, borders and the rims of holes stay on their line (default true).
    pub keep_features: bool,
    /// An edge is sharp when its two triangles meet at more than this angle (default 30 degrees).
    pub feature_angle_deg: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            target: Target::Percent(50.0),
            keep_features: true,
            feature_angle_deg: 30.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Simplified {
    pub mesh: TriMesh,
    pub triangles_before: usize,
    pub triangles_after: usize,
    /// The target was reached (`triangles_after` is not more than the number asked for; once in a while one less, because a collapse removes two triangles).
    pub reached: bool,
    /// Empty when the target was reached; otherwise plain English why the simplification stopped sooner.
    pub reason: String,
    /// The largest distance of any original vertex from the new surface in mm (measured exactly, point to triangle).
    pub max_deviation: f64,
    /// The mean of the same distance over all original vertices (those that stayed in place count as zero).
    pub mean_deviation: f64,
    pub seconds: f64,
}

/// Vertices with more triangles round them than this are not collapsed (the work of one collapse is proportional to them).
const MAX_VALENCE: usize = 20_000;
/// Ratio of the weight of a boundary quadric to the area of the edge squared.
const BORDER_WEIGHT: f64 = 4.0;
/// A new triangle must keep at least this cosine between its old and its new normal.
const MIN_NORMAL_COS: f64 = 0.1;
/// A new triangle must have at least this quality (4 sqrt(3) area / sum of squared edges; 1 is equilateral), or as much as it had.
const MIN_QUALITY: f64 = 1e-5;
/// The sharpness of a vertex is also measured with these (relative to the size of the box of the mesh).
const LINE_TOLERANCE: f64 = 1e-5;
const ZERO_ERROR: f64 = 1e-13;
const MAX_PASSES: usize = 6;

// ---------------------------------------------------------------------------------------------------------------------------------------------------
// Quadrics

/// The sum of squared distances from planes (each weighted) as a quadratic form: `p A p + 2 b p + c`.
#[derive(Clone, Copy, Default)]
struct Quadric {
    /// xx, xy, xz, yy, yz, zz
    a: [f64; 6],
    b: [f64; 3],
    c: f64,
}

impl Quadric {
    /// The plane `n . x + d = 0` (`n` a unit vector) counted `w` times.
    fn plane(n: V3, d: f64, w: f64) -> Quadric {
        Quadric {
            a: [
                w * n[0] * n[0],
                w * n[0] * n[1],
                w * n[0] * n[2],
                w * n[1] * n[1],
                w * n[1] * n[2],
                w * n[2] * n[2],
            ],
            b: [w * d * n[0], w * d * n[1], w * d * n[2]],
            c: w * d * d,
        }
    }

    fn add(&mut self, o: &Quadric) {
        for i in 0..6 {
            self.a[i] += o.a[i];
        }
        for i in 0..3 {
            self.b[i] += o.b[i];
        }
        self.c += o.c;
    }

    fn sum(&self, o: &Quadric) -> Quadric {
        let mut s = *self;
        s.add(o);
        s
    }

    fn eval(&self, p: V3) -> f64 {
        let a = &self.a;
        let v = p[0] * (a[0] * p[0] + 2.0 * a[1] * p[1] + 2.0 * a[2] * p[2])
            + p[1] * (a[3] * p[1] + 2.0 * a[4] * p[2])
            + a[5] * p[2] * p[2]
            + 2.0 * (self.b[0] * p[0] + self.b[1] * p[1] + self.b[2] * p[2])
            + self.c;
        v.max(0.0)
    }

    /// The point of the least error, pulled a little towards `mid` so that a flat or straight neighbourhood (where the minimum is a plane or a line) gives
    /// the point of it nearest to `mid`.
    fn optimum(&self, mid: V3) -> Option<V3> {
        let a = &self.a;
        let tr = (a[0] + a[3] + a[5]) / 3.0;
        if tr <= 0.0 {
            return None;
        }
        let lambda = 1e-6 * tr;
        let mut m = [
            [a[0] + lambda, a[1], a[2], lambda * mid[0] - self.b[0]],
            [a[1], a[3] + lambda, a[4], lambda * mid[1] - self.b[1]],
            [a[2], a[4], a[5] + lambda, lambda * mid[2] - self.b[2]],
        ];
        for i in 0..3 {
            let mut piv = i;
            for r in i + 1..3 {
                if m[r][i].abs() > m[piv][i].abs() {
                    piv = r;
                }
            }
            if m[piv][i].abs() < 1e-12 * tr {
                return None;
            }
            m.swap(i, piv);
            for r in i + 1..3 {
                let f = m[r][i] / m[i][i];
                for c in i..4 {
                    m[r][c] -= f * m[i][c];
                }
            }
        }
        let mut x = [0.0; 3];
        for i in (0..3).rev() {
            let mut s = m[i][3];
            for c in i + 1..3 {
                s -= m[i][c] * x[c];
            }
            x[i] = s / m[i][i];
        }
        x.iter().all(|v| v.is_finite()).then_some(x)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------
// Geometry helpers

fn dist_point_segment(p: V3, a: V3, b: V3) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    len(sub(p, add(a, mul(ab, t))))
}

/// The closest point of the triangle `a b c` to `p`.
fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, mul(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, mul(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, mul(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    add(add(a, mul(ab, vb * denom)), mul(ac, vc * denom))
}

fn dist2_point_triangle(p: V3, t: &[V3; 3]) -> f64 {
    let q = closest_on_triangle(p, t[0], t[1], t[2]);
    let d = sub(p, q);
    dot(d, d)
}

/// Twice the area, and the quality (1 for an equilateral triangle, 0 for a flat one), of a triangle given by its corners.
fn quality(c: &[V3; 3]) -> (V3, f64) {
    let n = cross(sub(c[1], c[0]), sub(c[2], c[0]));
    let (e0, e1, e2) = (sub(c[1], c[0]), sub(c[2], c[1]), sub(c[0], c[2]));
    let s = dot(e0, e0) + dot(e1, e1) + dot(e2, e2);
    let q = if s > 0.0 {
        2.0 * 3f64.sqrt() * len(n) / s
    } else {
        0.0
    };
    (n, q)
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------
// A bounding volume hierarchy of the result, for the exact deviation

struct Node {
    lo: V3,
    hi: V3,
    /// A leaf: the first triangle; an inner node: the right child (the left one follows the node).
    first: u32,
    /// The number of triangles of a leaf, 0 for an inner node.
    count: u32,
}

struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<[V3; 3]>,
}

impl Bvh {
    fn build(tris: Vec<[V3; 3]>) -> Bvh {
        let n = tris.len();
        let mut idx: Vec<u32> = (0..n as u32).collect();
        let cent: Vec<V3> = tris
            .iter()
            .map(|t| {
                [
                    (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                    (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                    (t[0][2] + t[1][2] + t[2][2]) / 3.0,
                ]
            })
            .collect();
        let mut nodes = Vec::with_capacity(n / 2 + 1);
        if n > 0 {
            Self::split(&mut nodes, &tris, &cent, &mut idx, 0, n);
        }
        let ordered = idx.iter().map(|&i| tris[i as usize]).collect();
        Bvh { nodes, tris: ordered }
    }

    fn split(
        nodes: &mut Vec<Node>,
        tris: &[[V3; 3]],
        cent: &[V3],
        idx: &mut [u32],
        lo: usize,
        hi: usize,
    ) -> u32 {
        let me = nodes.len() as u32;
        let (mut bl, mut bh) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let (mut cl, mut ch) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for &i in &idx[lo..hi] {
            for c in &tris[i as usize] {
                for k in 0..3 {
                    bl[k] = bl[k].min(c[k]);
                    bh[k] = bh[k].max(c[k]);
                }
            }
            for k in 0..3 {
                cl[k] = cl[k].min(cent[i as usize][k]);
                ch[k] = ch[k].max(cent[i as usize][k]);
            }
        }
        let axis = (0..3)
            .max_by(|&a, &b| (ch[a] - cl[a]).total_cmp(&(ch[b] - cl[b])))
            .unwrap_or(0);
        if hi - lo <= 4 || ch[axis] <= cl[axis] {
            nodes.push(Node {
                lo: bl,
                hi: bh,
                first: lo as u32,
                count: (hi - lo) as u32,
            });
            return me;
        }
        nodes.push(Node {
            lo: bl,
            hi: bh,
            first: 0,
            count: 0,
        });
        let mid = (lo + hi) / 2;
        idx[lo..hi].select_nth_unstable_by(mid - lo, |&a, &b| {
            cent[a as usize][axis].total_cmp(&cent[b as usize][axis])
        });
        Self::split(nodes, tris, cent, idx, lo, mid);
        let right = Self::split(nodes, tris, cent, idx, mid, hi);
        nodes[me as usize].first = right;
        me
    }

    fn box_d2(n: &Node, p: V3) -> f64 {
        let mut d = 0.0;
        for k in 0..3 {
            let e = (n.lo[k] - p[k]).max(p[k] - n.hi[k]).max(0.0);
            d += e * e;
        }
        d
    }

    /// The squared distance of `p` from the nearest triangle.
    fn dist2(&self, p: V3) -> f64 {
        if self.nodes.is_empty() {
            return f64::INFINITY;
        }
        let mut best = f64::INFINITY;
        let mut stack = [0u32; 128];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let i = stack[sp] as usize;
            let n = &self.nodes[i];
            if Self::box_d2(n, p) >= best {
                continue;
            }
            if n.count > 0 {
                for t in &self.tris[n.first as usize..(n.first + n.count) as usize] {
                    best = best.min(dist2_point_triangle(p, t));
                }
            } else {
                let (l, r) = (i + 1, n.first as usize);
                let (dl, dr) = (Self::box_d2(&self.nodes[l], p), Self::box_d2(&self.nodes[r], p));
                let (far, near) = if dl <= dr { (r, l) } else { (l, r) };
                if sp + 2 <= stack.len() {
                    stack[sp] = far as u32;
                    stack[sp + 1] = near as u32;
                    sp += 2;
                }
            }
        }
        best
    }
}

/// The largest and the mean distance (and the number of distances) of the points from the surface of `bvh`; `None` when stopped.
fn deviation(bvh: &Bvh, points: &[V3], stop: &dyn Fn() -> bool) -> Option<(f64, f64)> {
    let cancel = AtomicBool::new(false);
    let next = AtomicUsize::new(0);
    const CHUNK: usize = 2048;
    let work = || {
        let (mut mx, mut sum) = (0.0f64, 0.0f64);
        loop {
            if cancel.load(AtomicOrdering::Relaxed) {
                return (mx, sum);
            }
            let s = next.fetch_add(CHUNK, AtomicOrdering::Relaxed);
            if s >= points.len() {
                return (mx, sum);
            }
            for p in &points[s..(s + CHUNK).min(points.len())] {
                let d = bvh.dist2(*p).sqrt();
                mx = mx.max(d);
                sum += d;
            }
        }
    };
    let threads = if points.len() < 4 * CHUNK {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(16)
    };
    let parts: Vec<(f64, f64)> = if threads == 1 {
        let mut out = Vec::new();
        // single thread: poll between chunks by working in the loop above through a tiny wrapper
        let mut done = false;
        while !done {
            if stop() {
                return None;
            }
            // one chunk at a time
            let s = next.fetch_add(CHUNK, AtomicOrdering::Relaxed);
            if s >= points.len() {
                done = true;
            } else {
                let (mut mx, mut sum) = (0.0f64, 0.0f64);
                for p in &points[s..(s + CHUNK).min(points.len())] {
                    let d = bvh.dist2(*p).sqrt();
                    mx = mx.max(d);
                    sum += d;
                }
                out.push((mx, sum));
            }
        }
        out
    } else {
        std::thread::scope(|sc| {
            let hs: Vec<_> = (0..threads).map(|_| sc.spawn(work)).collect();
            while !hs.iter().all(|h| h.is_finished()) {
                if stop() {
                    cancel.store(true, AtomicOrdering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            hs.into_iter().filter_map(|h| h.join().ok()).collect()
        })
    };
    if cancel.load(AtomicOrdering::Relaxed) {
        return None;
    }
    Some(
        parts
            .iter()
            .fold((0.0, 0.0), |(m, s), &(pm, ps)| (m.max(pm), s + ps)),
    )
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------
// The working mesh

/// One candidate of the queue: an edge and its priority, the smallest error first, among equal errors (flat places) the shortest edge. The priority is
/// the bits of two non-negative floats (they sort like integers). An entry whose edge has changed since is found by making the priority again.
#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    u: u32,
    v: u32,
}

fn make_key(cost: f64, len2: f64) -> u64 {
    ((cost as f32).to_bits() as u64) << 32 | (len2 as f32).to_bits() as u64
}

/// A 4-ary min-heap (shallower than a binary one: fewer cache misses, and the four children share a cache line).
struct Heap {
    a: Vec<Entry>,
}

impl Heap {
    fn from_vec(a: Vec<Entry>) -> Heap {
        let mut h = Heap { a };
        let n = h.a.len();
        if n > 1 {
            for i in (0..=(n - 2) / 4).rev() {
                h.sift_down(i);
            }
        }
        h
    }

    fn push(&mut self, e: Entry) {
        self.a.push(e);
        let mut i = self.a.len() - 1;
        while i > 0 {
            let parent = (i - 1) / 4;
            if self.a[parent].key <= e.key {
                break;
            }
            self.a[i] = self.a[parent];
            i = parent;
        }
        self.a[i] = e;
    }

    fn pop(&mut self) -> Option<Entry> {
        let last = self.a.pop()?;
        if self.a.is_empty() {
            return Some(last);
        }
        let top = std::mem::replace(&mut self.a[0], last);
        self.sift_down(0);
        Some(top)
    }

    fn sift_down(&mut self, mut i: usize) {
        let n = self.a.len();
        let e = self.a[i];
        loop {
            let first = 4 * i + 1;
            if first >= n {
                break;
            }
            let mut best = first;
            for c in first + 1..(first + 4).min(n) {
                if self.a[c].key < self.a[best].key {
                    best = c;
                }
            }
            if self.a[best].key >= e.key {
                break;
            }
            self.a[i] = self.a[best];
            i = best;
        }
        self.a[i] = e;
    }
}

/// Buffers of `validate`, kept between the calls.
#[derive(Default)]
struct Scratch {
    opp: Vec<u32>,
    nr: Vec<u32>,
    ns: Vec<u32>,
    common: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Plan {
    /// The vertex that stays.
    s: u32,
    /// The vertex that is joined to it.
    r: u32,
    p: V3,
    cost: f64,
}

/// Everything one vertex needs in one place (two cache lines: the queue touches vertices all over the mesh).
#[derive(Clone)]
#[repr(align(64))]
struct Vert {
    pos: V3,
    q: Quadric,
    /// The sum of the weights of the quadrics: the error is compared with it (a mean squared distance).
    qw: f64,
    /// The number of feature neighbours (the length of `flist`).
    nfeat: u16,
    alive: bool,
    moved: bool,
    boundary: bool,
    /// Never touched: vertices of edges of more than two triangles or of two that disagree about the outside.
    frozen: bool,
}

struct Work {
    v: Vec<Vert>,
    tris: Vec<[u32; 3]>,
    tri_alive: Vec<bool>,
    alive_tris: usize,
    vt: Vec<Vec<u32>>,
    /// Where triangle `t` stands in the list `vt` of each of its corners (so that it is taken out at once, however long the list).
    slot: Vec<[u32; 3]>,
    /// The feature neighbours of a vertex (with `keep_features`): none = free, two = on a line, any other number = a corner.
    flist: Vec<Vec<u32>>,
    keep: bool,
    line_tol: f64,
    zero_err: f64,
}

impl Work {
    fn neighbours(&self, x: u32, out: &mut Vec<u32>) {
        out.clear();
        for &t in &self.vt[x as usize] {
            for &c in &self.tris[t as usize] {
                if c != x {
                    out.push(c);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    /// The best way to join `u` and `v` (which vertex stays, where it goes, the error), or `None` when the features forbid it.
    fn plan(&self, u: u32, v: u32) -> Option<Plan> {
        let (ui, vi) = (u as usize, v as usize);
        if self.v[ui].frozen || self.v[vi].frozen {
            return None;
        }
        let q = self.v[ui].q.sum(&self.v[vi].q);
        let zero = self.zero_err * (self.v[ui].qw + self.v[vi].qw);
        let (cu, cv) = (self.v[ui].nfeat, self.v[vi].nfeat);
        let mut best: Option<Plan> = None;
        let mut consider = |s: u32, r: u32, p: V3| {
            let mut cost = q.eval(p);
            if cost <= zero {
                cost = 0.0;
            }
            if best.is_none_or(|b| cost < b.cost) {
                best = Some(Plan { s, r, p, cost });
            }
        };
        let (pu, pv) = (self.v[ui].pos, self.v[vi].pos);
        match (cu, cv) {
            (0, 0) => {
                let mid = mul(add(pu, pv), 0.5);
                if let Some(o) = q.optimum(mid).filter(|o| len(sub(*o, mid)) <= len(sub(pu, pv))) {
                    consider(u, v, o);
                }
                consider(u, v, mid);
                consider(u, v, pu);
                consider(v, u, pv);
            }
            (0, _) => consider(v, u, pv),
            (_, 0) => consider(u, v, pu),
            _ => {
                // two feature vertices: only along a feature edge, and only a vertex of a straight line may go
                if !self.flist[ui].contains(&v) {
                    return None;
                }
                for (s, r) in [(u, v), (v, u)] {
                    let fl = &self.flist[r as usize];
                    if fl.len() != 2 {
                        continue;
                    }
                    let b = if fl[0] == s { fl[1] } else { fl[0] };
                    if dist_point_segment(
                        self.v[r as usize].pos,
                        self.v[s as usize].pos,
                        self.v[b as usize].pos,
                    ) <= self.line_tol
                    {
                        consider(s, r, self.v[s as usize].pos);
                    }
                }
            }
        }
        best
    }

    /// Checks everything that can go wrong when `pl.r` is joined to `pl.s`: the link condition, the borders, the shape of the new triangles.
    /// Returns the number of triangles that disappear.
    fn validate(&self, pl: &Plan, sc: &mut Scratch) -> Option<usize> {
        let (r, s) = (pl.r, pl.s);
        let (ri, si) = (r as usize, s as usize);
        if self.vt[ri].len() + self.vt[si].len() > MAX_VALENCE {
            return None;
        }
        sc.opp.clear();
        for &t in &self.vt[ri] {
            let tri = self.tris[t as usize];
            if tri.contains(&s) {
                sc.opp.push(tri.iter().copied().find(|&c| c != r && c != s)?);
                if sc.opp.len() > 2 {
                    return None;
                }
            }
        }
        let adj = sc.opp.len();
        if adj == 0 {
            return None;
        }
        sc.opp.sort_unstable();
        self.neighbours(r, &mut sc.nr);
        self.neighbours(s, &mut sc.ns);
        // the link condition: the vertices next to both are exactly the ones opposite the edge
        sc.common.clear();
        for &c in &sc.nr {
            if c != s && sc.ns.binary_search(&c).is_ok() {
                sc.common.push(c);
            }
        }
        if sc.common != sc.opp {
            return None;
        }
        let (br, bs) = (self.v[ri].boundary, self.v[si].boundary);
        if adj == 2 && br && bs {
            return None;
        }
        if adj == 1 && !(br && bs) {
            return None;
        }
        let k = self.vt[ri].len() + self.vt[si].len() - 2 * adj;
        let min_k = if br || bs { 1 } else { 3 };
        if k < min_k {
            return None;
        }
        // flipped and degenerate triangles
        for (x, other) in [(r, s), (s, r)] {
            for &t in &self.vt[x as usize] {
                let tri = self.tris[t as usize];
                if tri.contains(&other) {
                    continue;
                }
                let old = tri.map(|i| self.v[i as usize].pos);
                let new = tri.map(|i| if i == x { pl.p } else { self.v[i as usize].pos });
                let (n_old, q_old) = quality(&old);
                let (n_new, q_new) = quality(&new);
                if q_new < MIN_QUALITY.min(q_old) {
                    return None;
                }
                let (lo, ln) = (len(n_old), len(n_new));
                if lo > 0.0 && ln > 0.0 && dot(n_old, n_new) < MIN_NORMAL_COS * lo * ln {
                    return None;
                }
            }
        }
        Some(adj)
    }

    /// Takes triangle `t` (whose corner `k` is `x`) out of the list of `x`.
    fn unlink(&mut self, x: u32, t: u32, k: usize) {
        let i = self.slot[t as usize][k] as usize;
        let list = &mut self.vt[x as usize];
        list.swap_remove(i);
        if let Some(&moved) = list.get(i) {
            if let Some(mk) = self.tris[moved as usize].iter().position(|&c| c == x) {
                self.slot[moved as usize][mk] = i as u32;
            }
        }
    }

    /// Joins `pl.r` to `pl.s` at `pl.p`.
    fn apply(&mut self, pl: &Plan) {
        let (r, s) = (pl.r, pl.s);
        let (ri, si) = (r as usize, s as usize);
        let tris_r = std::mem::take(&mut self.vt[ri]);
        for &t in &tris_r {
            let tri = self.tris[t as usize];
            if tri.contains(&s) {
                self.tri_alive[t as usize] = false;
                self.alive_tris -= 1;
                for (k, &x) in tri.iter().enumerate() {
                    if x != r {
                        self.unlink(x, t, k);
                    }
                }
            } else if let Some(k) = tri.iter().position(|&c| c == r) {
                self.tris[t as usize][k] = s;
                self.slot[t as usize][k] = self.vt[si].len() as u32;
                self.vt[si].push(t);
            }
        }
        // the feature line goes on from `s` to the other neighbour of `r`
        let fr = std::mem::take(&mut self.flist[ri]);
        if let Some(&b) = fr.iter().find(|&&x| x != s) {
            self.flist[si].retain(|&x| x != r);
            if !self.flist[si].contains(&b) {
                self.flist[si].push(b);
            }
            let fb = &mut self.flist[b as usize];
            fb.retain(|&x| x != r);
            if !fb.contains(&s) {
                fb.push(s);
            }
            self.v[b as usize].nfeat = self.flist[b as usize].len() as u16;
            self.v[si].nfeat = self.flist[si].len() as u16;
        }
        self.v[ri].nfeat = 0;
        let qr = self.v[ri].q;
        self.v[si].q.add(&qr);
        self.v[si].qw += self.v[ri].qw;
        if pl.p != self.v[si].pos {
            self.v[si].pos = pl.p;
            self.v[si].moved = true;
        }
        self.v[si].boundary |= self.v[ri].boundary;
        self.v[ri].alive = false;
    }

    /// The priority of the edge `u` `v` and the plan for it; `None` when the edge may not be collapsed.
    fn keyed(&self, u: u32, v: u32) -> Option<(u64, Plan)> {
        let pl = self.plan(u, v)?;
        let d = sub(self.v[u as usize].pos, self.v[v as usize].pos);
        Some((make_key(pl.cost, dot(d, d)), pl))
    }

    fn push_edge(&self, heap: &mut Heap, u: u32, v: u32) {
        if let Some((key, _)) = self.keyed(u, v) {
            heap.push(Entry { key, u, v });
        }
    }

    /// The edges of the alive triangles, every one once.
    fn all_edges(&self) -> Vec<u64> {
        let mut e: Vec<u64> = Vec::with_capacity(self.alive_tris * 3 / 2 + 8);
        for (t, tri) in self.tris.iter().enumerate() {
            if !self.tri_alive[t] {
                continue;
            }
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                e.push(((lo as u64) << 32) | hi as u64);
            }
        }
        e.sort_unstable();
        e.dedup();
        e
    }

    /// Collapses edges until `target` triangles are left or nothing more can be collapsed. `false` when stopped.
    fn run(&mut self, target: usize, stop: &dyn Fn() -> bool) -> bool {
        let mut polls = 0usize;
        let mut nb: Vec<u32> = Vec::new();
        let mut scratch = Scratch::default();
        for _pass in 0..MAX_PASSES {
            if self.alive_tris <= target {
                break;
            }
            let before = self.alive_tris;
            let edges = self.all_edges();
            let mut entries = Vec::with_capacity(edges.len());
            for (i, key) in edges.iter().enumerate() {
                if i & 0xfff == 0 && stop() {
                    return false;
                }
                let (u, v) = ((key >> 32) as u32, (key & 0xffff_ffff) as u32);
                if let Some((key, _)) = self.keyed(u, v) {
                    entries.push(Entry { key, u, v });
                }
            }
            drop(edges);
            let mut heap = Heap::from_vec(entries);
            while let Some(e) = heap.pop() {
                if self.alive_tris <= target {
                    break;
                }
                polls += 1;
                if polls & 0xff == 0 && stop() {
                    return false;
                }
                if !self.v[e.u as usize].alive || !self.v[e.v as usize].alive {
                    continue;
                }
                // an entry made for an older state of the edge has another priority now
                let Some((key, pl)) = self.keyed(e.u, e.v) else {
                    continue;
                };
                if key != e.key || self.validate(&pl, &mut scratch).is_none() {
                    continue;
                }
                self.apply(&pl);
                self.neighbours(pl.s, &mut nb);
                for i in 0..nb.len() {
                    self.push_edge(&mut heap, pl.s, nb[i]);
                }
            }
            if self.alive_tris <= target || self.alive_tris == before {
                break;
            }
        }
        true
    }
}

/// Spaces between the thousands: 1 200 000.
fn spaced(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Makes `m` simpler, see the module description. `stop` is called now and then; when it returns `true` the work is abandoned and `None` is returned.
pub fn simplify(m: &TriMesh, o: &Options, stop: &dyn Fn() -> bool) -> Option<Simplified> {
    let started = Instant::now();
    let before = m.triangles.len();
    let target = match o.target {
        Target::Percent(p) => {
            let p = if p.is_finite() { p.clamp(0.0, 100.0) } else { 100.0 };
            ((before as f64) * p / 100.0).round() as usize
        }
        Target::Triangles(n) => n,
    };
    let nv = m.positions.len();
    // the middle of the box: all the work is done relative to it
    let (centre, diag) = match m.bounding_box() {
        Some((lo, hi)) => (mul(add(lo, hi), 0.5), len(sub(hi, lo))),
        None => ([0.0; 3], 0.0),
    };
    let diag = if diag > 0.0 { diag } else { 1.0 };
    // triangles with a repeated or missing vertex are dropped
    let tris: Vec<[u32; 3]> = m
        .triangles
        .iter()
        .copied()
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2] && t.iter().all(|&i| (i as usize) < nv))
        .collect();
    let mut used = vec![false; nv];
    for t in &tris {
        for &i in t {
            used[i as usize] = true;
        }
    }
    let v: Vec<Vert> = m
        .positions
        .iter()
        .map(|p| Vert {
            pos: sub([p[0] as f64, p[1] as f64, p[2] as f64], centre),
            q: Quadric::default(),
            qw: 0.0,
            nfeat: 0,
            alive: true,
            moved: false,
            boundary: false,
            frozen: false,
        })
        .collect();
    let mut w = Work {
        v,
        alive_tris: tris.len(),
        tri_alive: vec![true; tris.len()],
        vt: vec![Vec::new(); nv],
        slot: vec![[0; 3]; tris.len()],
        flist: vec![Vec::new(); nv],
        keep: o.keep_features,
        line_tol: LINE_TOLERANCE * diag,
        zero_err: ZERO_ERROR * diag * diag,
        tris,
    };
    let normals: Vec<(V3, f64)> = (0..w.tris.len())
        .map(|t| {
            let [a, b, c] = w.tris[t].map(|i| w.v[i as usize].pos);
            let n = cross(sub(b, a), sub(c, a));
            let l = len(n);
            if l > 0.0 {
                (mul(n, 1.0 / l), l * 0.5)
            } else {
                ([0.0; 3], 0.0)
            }
        })
        .collect();
    for (t, tri) in w.tris.iter().enumerate() {
        let (n, area) = normals[t];
        for (k, &i) in tri.iter().enumerate() {
            w.slot[t][k] = w.vt[i as usize].len() as u32;
            w.vt[i as usize].push(t as u32);
            if area > 0.0 {
                let d = -dot(n, w.v[i as usize].pos);
                let qd = Quadric::plane(n, d, area);
                w.v[i as usize].q.add(&qd);
                w.v[i as usize].qw += area;
            }
        }
    }
    if stop() {
        return None;
    }
    // edges: borders, sharp edges, edges nobody may touch
    let cos_limit = o.feature_angle_deg.to_radians().cos();
    {
        let carrier = TriMesh {
            positions: Vec::new(),
            triangles: w.tris.clone(),
        };
        let edges = sorted_edges(&carrier);
        if stop() {
            return None;
        }
        for run in runs(&edges) {
            let key = run[0].0;
            let (a, b) = ((key >> 32) as u32, (key & 0xffff_ffff) as u32);
            match run.len() {
                1 => {
                    let t = (run[0].1 >> 1) as usize;
                    w.v[a as usize].boundary = true;
                    w.v[b as usize].boundary = true;
                    if w.keep {
                        w.flist[a as usize].push(b);
                        w.flist[b as usize].push(a);
                    }
                    let (n, area) = normals[t];
                    if area > 0.0 {
                        let e = sub(w.v[b as usize].pos, w.v[a as usize].pos);
                        let m = unit(cross(e, n));
                        let weight = BORDER_WEIGHT * dot(e, e);
                        let d = -dot(m, w.v[a as usize].pos);
                        let qd = Quadric::plane(m, d, weight);
                        for v in [a, b] {
                            w.v[v as usize].q.add(&qd);
                            w.v[v as usize].qw += weight;
                        }
                    }
                }
                2 => {
                    if (run[0].1 & 1) == (run[1].1 & 1) {
                        w.v[a as usize].frozen = true;
                        w.v[b as usize].frozen = true;
                    } else if w.keep {
                        let (n1, a1) = normals[(run[0].1 >> 1) as usize];
                        let (n2, a2) = normals[(run[1].1 >> 1) as usize];
                        if a1 > 0.0 && a2 > 0.0 && dot(n1, n2) < cos_limit {
                            w.flist[a as usize].push(b);
                            w.flist[b as usize].push(a);
                        }
                    }
                }
                _ => {
                    w.v[a as usize].frozen = true;
                    w.v[b as usize].frozen = true;
                }
            }
        }
    }
    drop(normals);
    for (v, f) in w.v.iter_mut().zip(&w.flist) {
        v.nfeat = f.len().min(u16::MAX as usize) as u16;
    }
    if !w.run(target, stop) {
        return None;
    }
    // the result
    let mut map = vec![u32::MAX; nv];
    let mut out = TriMesh::default();
    for (t, tri) in w.tris.iter().enumerate() {
        if !w.tri_alive[t] {
            continue;
        }
        let mut n = [0u32; 3];
        for k in 0..3 {
            let v = tri[k] as usize;
            if map[v] == u32::MAX {
                map[v] = out.positions.len() as u32;
                if w.v[v].moved {
                    let p = add(w.v[v].pos, centre);
                    out.positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
                } else {
                    // a vertex that never moved keeps its exact coordinates
                    out.positions.push(m.positions[v]);
                }
            }
            n[k] = map[v];
        }
        out.triangles.push(n);
    }
    if stop() {
        return None;
    }
    // the exact deviation of the original vertices that did not stay where they were
    let bvh = Bvh::build((0..out.triangles.len()).map(|t| out.corners(t)).collect());
    let mut points = Vec::new();
    let mut count = 0usize;
    for v in 0..nv {
        if used[v] {
            count += 1;
            if !w.v[v].alive || w.v[v].moved {
                points.push(m.vertex(v as u32));
            }
        }
    }
    let (max_deviation, sum) = deviation(&bvh, &points, stop)?;
    let after = out.triangles.len();
    let reached = after <= target;
    let reason = if reached {
        String::new()
    } else if o.keep_features {
        format!(
            "{} triangles are not possible: the sharp edges and the borders of holes cannot be removed (or the surface would fold over); the smallest possible is {}",
            spaced(target),
            spaced(after)
        )
    } else {
        format!(
            "{} triangles are not possible: no more triangles can be removed without folding the surface over or changing the shape of the solid; the smallest possible is {}",
            spaced(target),
            spaced(after)
        )
    };
    Some(Simplified {
        mesh: out,
        triangles_before: before,
        triangles_after: after,
        reached,
        reason,
        max_deviation,
        mean_deviation: if count > 0 { sum / count as f64 } else { 0.0 },
        seconds: started.elapsed().as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::check;
    use crate::gen;

    fn never() -> bool {
        false
    }

    fn run(m: &TriMesh, target: Target, keep: bool) -> Simplified {
        simplify(
            m,
            &Options {
                target,
                keep_features: keep,
                feature_angle_deg: 30.0,
            },
            &never,
        )
        .expect("not stopped")
    }

    fn has_vertex(m: &TriMesh, p: [f32; 3]) -> bool {
        m.positions.contains(&p)
    }

    /// Distance of a point from a triangle by another method than the library uses: the plane when the foot lies inside, else the three segments.
    fn brute_dist(p: V3, t: &[V3; 3]) -> f64 {
        let n = unit(cross(sub(t[1], t[0]), sub(t[2], t[0])));
        let h = dot(sub(p, t[0]), n);
        let foot = sub(p, mul(n, h));
        let inside = (0..3).all(|i| {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            dot(cross(sub(b, a), sub(foot, a)), n) >= -1e-12
        });
        if inside {
            return h.abs();
        }
        (0..3)
            .map(|i| dist_point_segment(p, t[i], t[(i + 1) % 3]))
            .fold(f64::INFINITY, f64::min)
    }

    fn brute_deviation(orig: &TriMesh, new: &TriMesh) -> (f64, f64) {
        let tris: Vec<[V3; 3]> = (0..new.triangles.len()).map(|t| new.corners(t)).collect();
        let mut used = vec![false; orig.positions.len()];
        for t in &orig.triangles {
            for &i in t {
                used[i as usize] = true;
            }
        }
        let (mut mx, mut sum, mut n) = (0.0f64, 0.0f64, 0usize);
        for v in 0..orig.positions.len() {
            if !used[v] {
                continue;
            }
            let p = orig.vertex(v as u32);
            let d = tris
                .iter()
                .map(|t| brute_dist(p, t))
                .fold(f64::INFINITY, f64::min);
            mx = mx.max(d);
            sum += d;
            n += 1;
        }
        (mx, sum / n as f64)
    }

    /// Every triangle is a proper one and neighbours agree about the outside.
    fn assert_valid(m: &TriMesh) {
        let r = check(m);
        assert_eq!(r.degenerate, 0, "{r:?}");
        assert_eq!(r.non_manifold_edges, 0, "{r:?}");
        assert_eq!(r.inconsistent_edges, 0, "{r:?}");
        assert_eq!(r.unused_vertices, 0, "{r:?}");
    }

    fn euler(m: &TriMesh) -> i64 {
        let mut e = std::collections::HashSet::new();
        for t in &m.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                e.insert((a.min(b), a.max(b)));
            }
        }
        m.positions.len() as i64 - e.len() as i64 + m.triangles.len() as i64
    }

    #[test]
    fn flat_plate_goes_down_to_a_box() {
        let m = gen::box_mesh(64.0, 32.0, 8.0, 32);
        assert_eq!(m.triangles.len(), 6 * 32 * 32 * 2);
        let s = run(&m, Target::Triangles(12), true);
        assert!(s.reached, "{}", s.reason);
        assert!(s.reason.is_empty());
        assert_eq!(s.triangles_before, 12288);
        assert!(s.triangles_after <= 12, "{}", s.triangles_after);
        assert_valid(&s.mesh);
        let r = check(&s.mesh);
        assert!(r.is_closed() && r.signed_volume > 0.0);
        // the same box
        assert!((s.mesh.signed_volume() - 64.0 * 32.0 * 8.0).abs() < 1e-3);
        assert_eq!(s.mesh.bounding_box(), m.bounding_box());
        for x in [0.0, 64.0] {
            for y in [0.0, 32.0] {
                for z in [0.0, 8.0] {
                    assert!(has_vertex(&s.mesh, [x, y, z]));
                }
            }
        }
        assert!(s.max_deviation < 1e-4, "{}", s.max_deviation);
    }

    #[test]
    fn plate_with_holes_keeps_rims_and_edges() {
        let m = gen::plate_with_holes(3, 3, 10.0, 2.0, 3.0, 32);
        let before = m.triangles.len();
        let s = run(&m, Target::Percent(10.0), true);
        assert_valid(&s.mesh);
        assert!(s.triangles_after < before);
        // every vertex of the rim of a hole and every corner of the outline is still there, unmoved
        for j in 0..3 {
            for i in 0..3 {
                let (cx, cy) = (5.0 + 10.0 * i as f64, 5.0 + 10.0 * j as f64);
                for p in &m.positions {
                    let d = ((p[0] as f64 - cx).powi(2) + (p[1] as f64 - cy).powi(2)).sqrt();
                    if (d - 3.0).abs() < 1e-3 {
                        assert!(has_vertex(&s.mesh, *p), "a vertex of a hole rim was lost");
                    }
                }
            }
        }
        for x in [0.0, 30.0] {
            for y in [0.0, 30.0] {
                for z in [0.0, 2.0] {
                    let corner: Vec<[f32; 3]> = m
                        .positions
                        .iter()
                        .copied()
                        .filter(|p| {
                            (p[0] as f64 - x).abs() < 1e-3
                                && (p[1] as f64 - y).abs() < 1e-3
                                && (p[2] as f64 - z).abs() < 1e-3
                        })
                        .collect();
                    assert_eq!(corner.len(), 1);
                    assert!(has_vertex(&s.mesh, corner[0]));
                }
            }
        }
        assert!(s.max_deviation < 1e-3, "{}", s.max_deviation);
        let r = check(&s.mesh);
        assert!(r.is_closed());
        assert_eq!(euler(&s.mesh), euler(&m));
        assert!((s.mesh.signed_volume() - m.signed_volume()).abs() < 1e-3 * m.signed_volume());
    }

    #[test]
    fn sphere_to_ten_percent() {
        let m = gen::sphere(10.0, 20000);
        let s = run(&m, Target::Percent(10.0), true);
        assert!(s.reached, "{}", s.reason);
        let want = (m.triangles.len() as f64 * 0.1).round() as usize;
        assert!(
            s.triangles_after <= want && s.triangles_after + 2 >= want,
            "{}",
            s.triangles_after
        );
        assert_valid(&s.mesh);
        let r = check(&s.mesh);
        assert!(r.is_closed());
        assert_eq!(r.components, 1);
        assert_eq!(euler(&s.mesh), 2);
        assert!(r.signed_volume > 0.0);
        assert!(s.max_deviation < 0.05, "{}", s.max_deviation);
        assert!(s.mean_deviation <= s.max_deviation);
        // the volume is close to the sphere
        let v = 4.0 / 3.0 * std::f64::consts::PI * 1000.0;
        assert!((r.signed_volume - v).abs() < 0.05 * v);
    }

    #[test]
    fn no_flipped_triangles() {
        // the sphere is star-shaped round the origin: every triangle must look outwards
        let m = gen::sphere(10.0, 5000);
        for pct in [50.0, 10.0, 2.0] {
            let s = run(&m, Target::Percent(pct), false);
            for t in 0..s.mesh.triangles.len() {
                let (n, a) = s.mesh.normal_area(t);
                assert!(a > 0.0);
                let c = s.mesh.corners(t);
                let mid = mul(add(add(c[0], c[1]), c[2]), 1.0 / 3.0);
                assert!(dot(n, unit(mid)) > 0.3, "triangle {t} at {pct} %");
            }
            assert_valid(&s.mesh);
            assert!(s.mesh.signed_volume() > 0.0);
        }
    }

    #[test]
    fn targets() {
        let m = gen::sphere(5.0, 3000);
        let a = run(&m, Target::Percent(25.0), true);
        assert!(a.reached);
        let want = (m.triangles.len() as f64 * 0.25).round() as usize;
        assert!(a.triangles_after <= want && a.triangles_after + 2 >= want);
        let b = run(&m, Target::Triangles(500), true);
        assert!(b.reached);
        assert!(
            b.triangles_after <= 500 && b.triangles_after >= 498,
            "{}",
            b.triangles_after
        );
        // nothing to do
        let c = run(&m, Target::Percent(100.0), true);
        assert!(c.reached);
        assert_eq!(c.triangles_after, m.triangles.len());
        assert_eq!(c.max_deviation, 0.0);
        let d = run(&m, Target::Triangles(10_000_000), true);
        assert!(d.reached);
        assert_eq!(d.triangles_after, m.triangles.len());
    }

    #[test]
    fn impossible_target_is_reported() {
        let m = gen::cylinder(10.0, 20.0, 64);
        let s = run(&m, Target::Triangles(10), true);
        assert!(!s.reached);
        assert!(s.reason.contains("smallest possible"), "{}", s.reason);
        assert!(s.reason.contains(&s.triangles_after.to_string()), "{}", s.reason);
        assert!(s.triangles_after > 10 && s.triangles_after <= m.triangles.len());
        assert_valid(&s.mesh);
        // the outline of the cylinder is untouched: both rims have all their vertices
        for p in &m.positions {
            if p[2] == 0.0 || p[2] == 20.0 {
                let on_rim = (p[0] * p[0] + p[1] * p[1]).sqrt() > 5.0;
                if on_rim {
                    assert!(has_vertex(&s.mesh, *p));
                }
            }
        }
        assert!(s.max_deviation < 1e-6, "{}", s.max_deviation);
        // a closed mesh never goes under a tetrahedron
        let t = run(&gen::sphere(5.0, 200), Target::Triangles(0), false);
        assert!(!t.reached);
        assert!(t.triangles_after >= 4);
        assert_valid(&t.mesh);
        assert_eq!(euler(&t.mesh), 2);
    }

    #[test]
    fn stop_is_polled() {
        let m = gen::sphere(5.0, 20000);
        let o = Options {
            target: Target::Percent(10.0),
            ..Options::default()
        };
        assert!(simplify(&m, &o, &|| true).is_none());
        let calls = std::cell::Cell::new(0usize);
        let r = simplify(&m, &o, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 20
        });
        assert!(r.is_none());
        assert!(calls.get() > 20);
    }

    #[test]
    fn deviation_matches_brute_force() {
        for (m, pct) in [(gen::sphere(7.0, 600), 20.0), (gen::cylinder(6.0, 9.0, 24), 50.0)] {
            for keep in [true, false] {
                let s = run(&m, Target::Percent(pct), keep);
                let (mx, mean) = brute_deviation(&m, &s.mesh);
                assert!(
                    (s.max_deviation - mx).abs() <= 1e-9 * (1.0 + mx),
                    "{} {}",
                    s.max_deviation,
                    mx
                );
                assert!(
                    (s.mean_deviation - mean).abs() <= 1e-9 * (1.0 + mean),
                    "{} {}",
                    s.mean_deviation,
                    mean
                );
            }
        }
    }

    #[test]
    fn features_off_may_round_everything() {
        let m = gen::box_mesh(10.0, 10.0, 10.0, 3);
        let a = run(&m, Target::Triangles(12), false);
        assert!(a.reached, "{}", a.reason);
        assert_valid(&a.mesh);
        let b = run(&gen::sphere(5.0, 400), Target::Triangles(20), false);
        assert!(b.reached, "{}", b.reason);
        assert_valid(&b.mesh);
        assert_eq!(euler(&b.mesh), 2);
    }

    /// A flat open grid: its outline and its area stay, the borders are kept straight.
    #[test]
    fn open_plate_keeps_its_outline() {
        let n = 30usize;
        let mut m = TriMesh::default();
        for j in 0..=n {
            for i in 0..=n {
                m.positions.push([i as f32 * 2.0, j as f32 * 1.0, 3.0]);
            }
        }
        let id = |i: usize, j: usize| (j * (n + 1) + i) as u32;
        for j in 0..n {
            for i in 0..n {
                m.triangles.push([id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                m.triangles.push([id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        let s = run(&m, Target::Triangles(2), true);
        assert!(s.reached, "{}", s.reason);
        assert_eq!(s.triangles_after, 2);
        let area: f64 = (0..2).map(|t| s.mesh.normal_area(t).1).sum();
        assert!((area - 60.0 * 30.0).abs() < 1e-6);
        for p in [
            [0.0, 0.0, 3.0],
            [60.0, 0.0, 3.0],
            [60.0, 30.0, 3.0],
            [0.0, 30.0, 3.0],
        ] {
            assert!(has_vertex(&s.mesh, p));
        }
        assert!(s.max_deviation < 1e-6);
        // every triangle still looks up
        for t in 0..2 {
            assert!(s.mesh.normal_area(t).0[2] > 0.99);
        }
    }

    #[test]
    fn frustum_and_degenerate_input() {
        let m = gen::frustum(20.0, 10.0, 8.0);
        let s = run(&m, Target::Percent(50.0), true);
        assert_valid(&s.mesh);
        assert!(check(&s.mesh).is_closed());
        // a mesh with a degenerate triangle does not break it
        let mut d = gen::sphere(5.0, 300);
        d.triangles.push([0, 0, 1]);
        let s = run(&d, Target::Percent(50.0), true);
        assert!(s.reached);
        assert_valid(&s.mesh);
        // an empty mesh
        let e = run(&TriMesh::default(), Target::Percent(50.0), true);
        assert!(e.reached);
        assert_eq!(e.triangles_after, 0);
    }

    /// Whatever the target and the mode: a valid closed mesh of the same genus, outwards, as long as nothing is said to be reached.
    #[test]
    fn stays_a_valid_solid_at_every_target() {
        let models = [
            gen::plate_with_holes(3, 2, 10.0, 2.0, 3.0, 24),
            gen::cylinder(8.0, 12.0, 48),
            gen::frustum(20.0, 8.0, 10.0),
            gen::sphere(6.0, 1500),
        ];
        for m in &models {
            let genus_euler = euler(m);
            let volume = m.signed_volume();
            for keep in [true, false] {
                for pct in [90.0, 60.0, 30.0, 10.0, 1.0] {
                    let s = run(m, Target::Percent(pct), keep);
                    assert_valid(&s.mesh);
                    let r = check(&s.mesh);
                    assert!(r.is_closed(), "{keep} {pct}");
                    assert_eq!(euler(&s.mesh), genus_euler, "{keep} {pct}");
                    assert!(
                        r.signed_volume > 0.0 && r.signed_volume < 1.5 * volume,
                        "{keep} {pct}"
                    );
                    assert_eq!(
                        s.reached,
                        s.triangles_after <= (m.triangles.len() as f64 * pct / 100.0).round() as usize
                    );
                    assert_eq!(s.reached, s.reason.is_empty());
                }
            }
        }
    }

    #[test]
    fn spacing_of_numbers() {
        assert_eq!(spaced(5), "5");
        assert_eq!(spaced(1200), "1 200");
        assert_eq!(spaced(1234567), "1 234 567");
        assert_eq!(spaced(100), "100");
    }

    #[test]
    #[ignore = "benchmark: cargo test -p rolidium-mesh --release simplify::tests::benchmark -- --ignored --nocapture"]
    fn benchmark() {
        fn go(name: &str, m: &TriMesh, target: Target, keep: bool) {
            let t = Instant::now();
            let s = run(m, target, keep);
            println!(
                "{name}: {} -> {} triangles in {:.2} s (simplify {:.2} s), reached {}, max deviation {:.5} mm, mean {:.6} mm {}",
                s.triangles_before,
                s.triangles_after,
                t.elapsed().as_secs_f64(),
                s.seconds,
                s.reached,
                s.max_deviation,
                s.mean_deviation,
                s.reason
            );
        }
        let g = Instant::now();
        let sphere5 = gen::sphere(50.0, 500_000);
        let sphere2 = gen::sphere(50.0, 2_000_000);
        println!("generated in {:.2} s", g.elapsed().as_secs_f64());
        go("sphere 500k to 10 %", &sphere5, Target::Percent(10.0), true);
        go("sphere 2M to 10 %", &sphere2, Target::Percent(10.0), true);
        go("sphere 2M to 1 %", &sphere2, Target::Percent(1.0), false);
        let g = Instant::now();
        // (the steps of the box must be exact in f32, or the faces do not weld: a power of two)
        let plate = gen::box_mesh(256.0, 128.0, 16.0, 256);
        println!("plates generated in {:.2} s", g.elapsed().as_secs_f64());
        go(
            "flat plate 786k to 50 triangles",
            &plate,
            Target::Triangles(50),
            true,
        );
        let plate = gen::box_mesh(256.0, 128.0, 16.0, 512);
        go(
            "flat plate 3.1M to 50 triangles",
            &plate,
            Target::Triangles(50),
            true,
        );
        let holes = gen::plate_with_holes(40, 40, 10.0, 3.0, 3.0, 64);
        go(
            "plate with 1600 holes to 10 %",
            &holes,
            Target::Percent(10.0),
            true,
        );
        let cyl = gen::cylinder(20.0, 50.0, 200_000);
        go(
            "cylinder 800k, outline kept, to 10 %",
            &cyl,
            Target::Percent(10.0),
            true,
        );
    }
}
