// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The second half of the conversion of a mesh to a solid, for technical parts: the triangles are grouped into flat faces and into round faces that are
//! fitted with a cylinder, a cone, a sphere or a torus (the points of a mesh from a CAD program lie on the real surface, the facets lie inside it), the
//! boundaries between neighbouring faces are replaced by lines, circles and ellipses fitted to the points of the boundary, and everything is handed to
//! `rolidium_kernel::Solid::from_surfaces` as shared points, surfaces, edges and faces. A face that is not recognised stays a flat face of its triangles,
//! so the conversion never gives up what the flat conversion (`convert::polyfaces`) could do.

use crate::regions::neighbours;
use crate::{add, cross, dot, len, mul, sub, unit, TriMesh, V3};
use std::collections::HashMap;

/// What the conversion may recognise (a switch for the fall-back when the kernel refuses the result).
#[derive(Debug, Clone, Copy)]
pub struct Allow {
    pub cylinders: bool,
    pub cones: bool,
    pub spheres: bool,
    pub tori: bool,
    /// Ellipses (a flat cut of a cylinder or a cone) as edges.
    pub ellipses: bool,
}

impl Allow {
    pub const ALL: Allow = Allow {
        cylinders: true,
        cones: true,
        spheres: true,
        tori: true,
        ellipses: true,
    };
    pub const CYLINDERS: Allow = Allow {
        cylinders: true,
        cones: false,
        spheres: false,
        tori: false,
        ellipses: false,
    };
}

#[derive(Debug, Clone, Copy)]
pub struct FitOptions {
    /// How far a point may be from the surface of its face (millimetres).
    pub tolerance: f64,
    /// Move coordinates within a tenth of the tolerance of a multiple of 0.01 mm to it.
    pub round: bool,
    pub allow: Allow,
}

#[derive(Debug, Clone, Copy)]
pub struct FEdge {
    /// 0 line, 1 circle, 2 ellipse.
    pub kind: u32,
    pub v: [u32; 2],
    pub params: [f64; 12],
    pub tol: f64,
}

#[derive(Debug, Clone, Default)]
pub struct FFace {
    pub surface: u32,
    /// The area of the triangles of the face.
    pub area: f64,
    /// Loops of `(edge << 1) | reversed`.
    pub loops: Vec<Vec<u32>>,
}

/// What was found (for the report).
#[derive(Debug, Clone, Default)]
pub struct FitStats {
    pub planes: usize,
    pub cylinders: usize,
    pub cones: usize,
    pub spheres: usize,
    pub tori: usize,
    /// Faces that are a single triangle (nothing flat or round was found round them).
    pub singles: usize,
    /// The share (0 to 1) of the surface area that is in single triangles.
    pub single_area: f64,
    /// The largest distance of a point of a face from the surface it was fitted with.
    pub max_deviation: f64,
    pub rounded: usize,
    /// Edges that are lines / circles / ellipses.
    pub lines: usize,
    pub circles: usize,
    pub ellipses: usize,
    /// The share (0 to 1) of the surface area in round faces.
    pub round_area: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Fitted {
    pub points: Vec<V3>,
    pub point_tol: Vec<f64>,
    pub surfaces: Vec<(u32, [f64; 12])>,
    pub edges: Vec<FEdge>,
    pub faces: Vec<FFace>,
    pub stats: FitStats,
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// linear algebra

/// Eigenvalues (ascending) and eigenvectors (rows) of a symmetric n x n matrix (Jacobi rotations).
fn sym_eigen(n: usize, a: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let mut a = a.to_vec();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    for _ in 0..60 {
        let mut off = 0.0;
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    off += a[i * n + j] * a[i * n + j];
                }
            }
        }
        if off < 1e-24 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq.abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k * n + p], a[k * n + q]);
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p * n + k], a[q * n + k]);
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v[k * n + p], v[k * n + q]);
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|x, y| a[*x * n + *x].total_cmp(&a[*y * n + *y]));
    let vals = idx.iter().map(|i| a[*i * n + *i]).collect();
    let vecs = idx
        .iter()
        .map(|i| (0..n).map(|k| v[k * n + *i]).collect())
        .collect();
    (vals, vecs)
}

/// Solves the n x n system (Gaussian elimination with pivoting); `None` when it is singular.
fn solve(n: usize, a: &[f64], b: &[f64]) -> Option<Vec<f64>> {
    let mut m = vec![0.0; n * (n + 1)];
    for i in 0..n {
        for j in 0..n {
            m[i * (n + 1) + j] = a[i * n + j];
        }
        m[i * (n + 1) + n] = b[i];
    }
    for c in 0..n {
        let piv = (c..n).max_by(|x, y| m[*x * (n + 1) + c].abs().total_cmp(&m[*y * (n + 1) + c].abs()))?;
        if m[piv * (n + 1) + c].abs() < 1e-12 {
            return None;
        }
        for k in 0..=n {
            m.swap(c * (n + 1) + k, piv * (n + 1) + k);
        }
        for r in 0..n {
            if r != c {
                let f = m[r * (n + 1) + c] / m[c * (n + 1) + c];
                for k in c..=n {
                    m[r * (n + 1) + k] -= f * m[c * (n + 1) + k];
                }
            }
        }
    }
    Some((0..n).map(|i| m[i * (n + 1) + n] / m[i * (n + 1) + i]).collect())
}

/// Two unit vectors square to `a` and to each other.
fn basis(a: V3) -> (V3, V3) {
    let h = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = unit(cross(a, h));
    (u, cross(a, u))
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// surfaces

#[derive(Debug, Clone, Copy, PartialEq)]
enum Surf {
    Plane { n: V3, off: f64 },
    Cyl { o: V3, a: V3, r: f64 },
    /// A point `o` of the axis where the radius is `r0`; the radius grows by `k` for a unit along the axis.
    Cone { o: V3, a: V3, k: f64, r0: f64 },
    Sphere { c: V3, r: f64 },
    Torus { c: V3, a: V3, big: f64, small: f64 },
}

impl Surf {
    fn dist(&self, p: V3) -> f64 {
        match *self {
            Surf::Plane { n, off } => dot(n, p) - off,
            Surf::Cyl { o, a, r } => {
                let q = sub(p, o);
                let rad = sub(q, mul(a, dot(q, a)));
                len(rad) - r
            }
            Surf::Cone { o, a, k, r0 } => {
                let q = sub(p, o);
                let z = dot(q, a);
                let rad = sub(q, mul(a, z));
                (len(rad) - (r0 + k * z)) / (1.0 + k * k).sqrt()
            }
            Surf::Sphere { c, r } => len(sub(p, c)) - r,
            Surf::Torus { c, a, big, small } => {
                let q = sub(p, c);
                let z = dot(q, a);
                let rad = sub(q, mul(a, z));
                let rho = len(rad);
                (((rho - big) * (rho - big) + z * z).sqrt()) - small
            }
        }
    }

    fn normal(&self, p: V3) -> V3 {
        match *self {
            Surf::Plane { n, .. } => n,
            Surf::Cyl { o, a, .. } => {
                let q = sub(p, o);
                unit(sub(q, mul(a, dot(q, a))))
            }
            Surf::Cone { o, a, k, .. } => {
                let q = sub(p, o);
                let rad = unit(sub(q, mul(a, dot(q, a))));
                unit(sub(rad, mul(a, k)))
            }
            Surf::Sphere { c, .. } => unit(sub(p, c)),
            Surf::Torus { c, a, big, .. } => {
                let q = sub(p, c);
                let z = dot(q, a);
                let rad = unit(sub(q, mul(a, z)));
                let core = add(c, mul(rad, big));
                unit(sub(p, core))
            }
        }
    }

    /// The kind code and 12 parameters for the kernel; `seam` is the direction in which the seam of a periodic surface should lie.
    fn params(&self, seam: V3) -> (u32, [f64; 12]) {
        let mut q = [0.0; 12];
        let put = |q: &mut [f64; 12], at: usize, v: V3| {
            q[at..at + 3].copy_from_slice(&v);
        };
        match *self {
            Surf::Plane { n, off } => {
                put(&mut q, 0, mul(n, off));
                put(&mut q, 3, n);
                (0, q)
            }
            Surf::Cyl { o, a, r } => {
                put(&mut q, 0, o);
                put(&mut q, 3, a);
                q[6] = r;
                put(&mut q, 8, seam_dir(a, seam));
                (1, q)
            }
            Surf::Cone { o, a, k, r0 } => {
                put(&mut q, 0, o);
                put(&mut q, 3, a);
                q[6] = k.atan();
                q[7] = r0;
                put(&mut q, 8, seam_dir(a, seam));
                (2, q)
            }
            Surf::Sphere { c, r } => {
                // the poles are put away from the seam: the axis is square to the direction of the patch (`seam` points away from it)
                let a = unit(cross(seam, basis(seam).0));
                put(&mut q, 0, c);
                put(&mut q, 3, a);
                q[6] = r;
                put(&mut q, 8, seam_dir(a, seam));
                (3, q)
            }
            Surf::Torus { c, a, big, small } => {
                put(&mut q, 0, c);
                put(&mut q, 3, a);
                q[6] = big;
                q[7] = small;
                put(&mut q, 8, seam_dir(a, seam));
                (4, q)
            }
        }
    }
}

/// The part of `d` square to the axis `a`, as the direction of the seam (any direction square to the axis when `d` is along it).
fn seam_dir(a: V3, d: V3) -> V3 {
    let r = sub(d, mul(a, dot(d, a)));
    if len(r) > 1e-9 {
        unit(r)
    } else {
        basis(a).0
    }
}

/// The least-squares circle through 2D points (algebraic fit, refined): centre, radius.
fn fit_circle_2d(p: &[(f64, f64)]) -> Option<((f64, f64), f64)> {
    if p.len() < 3 {
        return None;
    }
    let n = p.len() as f64;
    let (mx, my) = (
        p.iter().map(|q| q.0).sum::<f64>() / n,
        p.iter().map(|q| q.1).sum::<f64>() / n,
    );
    // x^2 + y^2 = 2 a x + 2 b y + c
    let mut ata = [0.0; 9];
    let mut atb = [0.0; 3];
    for q in p {
        let (x, y) = (q.0 - mx, q.1 - my);
        let row = [2.0 * x, 2.0 * y, 1.0];
        let rhs = x * x + y * y;
        for i in 0..3 {
            for j in 0..3 {
                ata[i * 3 + j] += row[i] * row[j];
            }
            atb[i] += row[i] * rhs;
        }
    }
    let s = solve(3, &ata, &atb)?;
    let (mut cx, mut cy) = (s[0], s[1]);
    let mut r = (s[2] + cx * cx + cy * cy).max(0.0).sqrt();
    // Gauss-Newton steps on the distances to the circle
    for _ in 0..8 {
        let mut jtj = [0.0; 9];
        let mut jtr = [0.0; 3];
        for q in p {
            let (dx, dy) = (q.0 - mx - cx, q.1 - my - cy);
            let d = (dx * dx + dy * dy).sqrt().max(1e-300);
            let row = [-dx / d, -dy / d, -1.0];
            let res = d - r;
            for i in 0..3 {
                for j in 0..3 {
                    jtj[i * 3 + j] += row[i] * row[j];
                }
                jtr[i] += row[i] * res;
            }
        }
        let Some(step) = solve(3, &jtj, &[-jtr[0], -jtr[1], -jtr[2]]) else {
            break;
        };
        cx += step[0];
        cy += step[1];
        r += step[2];
        if step.iter().all(|x| x.abs() < 1e-12) {
            break;
        }
    }
    Some(((cx + mx, cy + my), r))
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// the fits of one patch: the points and the facet normals (with their areas)

struct Patch {
    pts: Vec<V3>,
    normals: Vec<(V3, f64)>,
}

fn max_dist(s: &Surf, pts: &[V3]) -> f64 {
    pts.iter().map(|p| s.dist(*p).abs()).fold(0.0, f64::max)
}

fn fit_cylinder(p: &Patch, tol: f64) -> Option<Surf> {
    let mut m = [0.0; 9];
    let mut total = 0.0;
    for (n, w) in &p.normals {
        for i in 0..3 {
            for j in 0..3 {
                m[i * 3 + j] += w * n[i] * n[j];
            }
        }
        total += w;
    }
    if total <= 0.0 {
        return None;
    }
    let (vals, vecs) = sym_eigen(3, &m);
    // the normals are square to the axis, and they turn: at least about 12 degrees of the circle
    if vals[0] > 0.01 * total || vals[1] < 0.012 * total {
        return None;
    }
    let a = unit([vecs[0][0], vecs[0][1], vecs[0][2]]);
    let (u, v) = basis(a);
    let flat: Vec<(f64, f64)> = p.pts.iter().map(|q| (dot(*q, u), dot(*q, v))).collect();
    let ((cx, cy), r) = fit_circle_2d(&flat)?;
    if !(r.is_finite() && r > 1e-6) {
        return None;
    }
    let zs = p.pts.iter().map(|q| dot(*q, a)).sum::<f64>() / p.pts.len() as f64;
    let o = add(add(mul(u, cx), mul(v, cy)), mul(a, zs));
    let s = Surf::Cyl { o, a, r };
    (max_dist(&s, &p.pts) <= tol).then_some(s)
}

fn fit_sphere(p: &Patch, tol: f64) -> Option<Surf> {
    if p.pts.len() < 6 {
        return None;
    }
    // the normals of a sphere turn in two directions: a strip of a cylinder is not one
    if p.normals.len() >= 4 {
        let mut m = [0.0; 9];
        let mut total = 0.0;
        for (n, w) in &p.normals {
            for i in 0..3 {
                for j in 0..3 {
                    m[i * 3 + j] += w * n[i] * n[j];
                }
            }
            total += w;
        }
        let (vals, _) = sym_eigen(3, &m);
        if vals[0] < 0.02 * total {
            return None;
        }
    }
    let n = p.pts.len() as f64;
    let mut mean = [0.0; 3];
    for q in &p.pts {
        mean = add(mean, *q);
    }
    let mean = mul(mean, 1.0 / n);
    // |x|^2 = 2 c . x + k, in coordinates about the mean
    let mut ata = [0.0; 16];
    let mut atb = [0.0; 4];
    for q in &p.pts {
        let x = sub(*q, mean);
        let row = [2.0 * x[0], 2.0 * x[1], 2.0 * x[2], 1.0];
        let rhs = dot(x, x);
        for i in 0..4 {
            for j in 0..4 {
                ata[i * 4 + j] += row[i] * row[j];
            }
            atb[i] += row[i] * rhs;
        }
    }
    let s = solve(4, &ata, &atb)?;
    let c = [s[0], s[1], s[2]];
    let r = (s[3] + dot(c, c)).max(0.0).sqrt();
    if !(r.is_finite() && r > 1e-6) {
        return None;
    }
    let sph = Surf::Sphere { c: add(c, mean), r };
    (max_dist(&sph, &p.pts) <= tol).then_some(sph)
}

fn fit_cone(p: &Patch, tol: f64) -> Option<Surf> {
    // the facet normals of a cone have a constant component along the axis: the axis is the direction in which they vary least
    let total: f64 = p.normals.iter().map(|x| x.1).sum();
    if total <= 0.0 {
        return None;
    }
    let mut mean = [0.0; 3];
    for (n, w) in &p.normals {
        mean = add(mean, mul(*n, *w));
    }
    let mean = mul(mean, 1.0 / total);
    let mut m = [0.0; 9];
    for (n, w) in &p.normals {
        let d = sub(*n, mean);
        for i in 0..3 {
            for j in 0..3 {
                m[i * 3 + j] += w * d[i] * d[j];
            }
        }
    }
    let (vals, vecs) = sym_eigen(3, &m);
    if vals[0] > 0.01 * total || vals[1] < 0.012 * total {
        return None;
    }
    let a = unit([vecs[0][0], vecs[0][1], vecs[0][2]]);
    fit_cone_axis(&p.pts, a, tol)
}

/// A cone with a known axis direction: the axis position and the slope from the points (a linear fit).
fn fit_cone_axis(pts: &[V3], a: V3, tol: f64) -> Option<Surf> {
    let (u, v) = basis(a);
    let n = pts.len() as f64;
    let zmean = pts.iter().map(|q| dot(*q, a)).sum::<f64>() / n;
    // x^2 + y^2 = 2 qx x + 2 qy y + A + B z + C z^2   (z about its mean)
    let mut ata = [0.0; 25];
    let mut atb = [0.0; 5];
    for q in pts {
        let (x, y, z) = (dot(*q, u), dot(*q, v), dot(*q, a) - zmean);
        let row = [2.0 * x, 2.0 * y, 1.0, z, z * z];
        let rhs = x * x + y * y;
        for i in 0..5 {
            for j in 0..5 {
                ata[i * 5 + j] += row[i] * row[j];
            }
            atb[i] += row[i] * rhs;
        }
    }
    let s = solve(5, &ata, &atb)?;
    let (qx, qy, big_a, big_b, big_c) = (s[0], s[1], s[2], s[3], s[4]);
    if big_c < 1e-9 {
        return None;
    }
    let mut k = big_c.sqrt();
    let mut r0 = big_b / (2.0 * k);
    // r0 + k z must be positive over the points (the sign of k is free)
    let positive = |k: f64, r0: f64| pts.iter().all(|q| r0 + k * (dot(*q, a) - zmean) > 0.0);
    if !positive(k, r0) {
        k = -k;
        r0 = -r0;
        if !positive(k, r0) {
            return None;
        }
    }
    let _ = big_a;
    // a slope below a degree is a cylinder
    if k.abs() < 0.0175 || k.abs() > 60.0 {
        return None;
    }
    let o = add(add(mul(u, qx), mul(v, qy)), mul(a, zmean));
    let c = Surf::Cone { o, a, k, r0 };
    (max_dist(&c, &pts) <= tol).then_some(c)
}

/// The distance of the points from a torus given as `[cx, cy, cz, ax, ay, az, big, small]` (the axis need not be a unit vector).
fn torus_residuals(q: &[f64; 8], pts: &[V3], out: &mut Vec<f64>) {
    let a = unit([q[3], q[4], q[5]]);
    let c = [q[0], q[1], q[2]];
    out.clear();
    for p in pts {
        let d = sub(*p, c);
        let z = dot(d, a);
        let rho = len(sub(d, mul(a, z)));
        out.push((((rho - q[6]) * (rho - q[6]) + z * z).sqrt()) - q[7]);
    }
}

/// Levenberg-Marquardt on the distances of the points from the torus.
fn refine_torus(init: [f64; 8], pts: &[V3]) -> [f64; 8] {
    let mut q = init;
    let mut res = Vec::new();
    torus_residuals(&q, pts, &mut res);
    let mut cost: f64 = res.iter().map(|r| r * r).sum();
    let mut lambda = 1e-3;
    let mut r2 = Vec::new();
    for _ in 0..40 {
        // numeric Jacobian
        let mut jac = vec![[0.0; 8]; pts.len()];
        for k in 0..8 {
            let mut qq = q;
            let h = 1e-6 * (1.0 + q[k].abs());
            qq[k] += h;
            torus_residuals(&qq, pts, &mut r2);
            for (i, row) in jac.iter_mut().enumerate() {
                row[k] = (r2[i] - res[i]) / h;
            }
        }
        let mut jtj = [0.0; 64];
        let mut jtr = [0.0; 8];
        for (row, r) in jac.iter().zip(&res) {
            for i in 0..8 {
                for j in 0..8 {
                    jtj[i * 8 + j] += row[i] * row[j];
                }
                jtr[i] += row[i] * r;
            }
        }
        let mut improved = false;
        for _ in 0..8 {
            let mut m = jtj;
            for i in 0..8 {
                m[i * 8 + i] += lambda * (1.0 + jtj[i * 8 + i]);
            }
            let rhs: Vec<f64> = jtr.iter().map(|x| -x).collect();
            let Some(step) = solve(8, &m, &rhs) else {
                lambda *= 10.0;
                continue;
            };
            let mut cand = q;
            for k in 0..8 {
                cand[k] += step[k];
            }
            torus_residuals(&cand, pts, &mut r2);
            let c2: f64 = r2.iter().map(|r| r * r).sum();
            if c2 < cost {
                let done = (cost - c2) < 1e-16 * cost.max(1e-30);
                q = cand;
                res.clone_from(&r2);
                cost = c2;
                lambda = (lambda * 0.3).max(1e-12);
                improved = true;
                if done {
                    return q;
                }
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
    q
}

/// A torus: the axis from the lines of the vertex normals (every normal line of a surface of revolution meets its axis), then the profile circle, then a fit
/// of all the parameters to the points.
fn fit_torus(p: &Patch, vnormals: &[V3], tol: f64) -> Option<Surf> {
    let n = p.pts.len();
    if n < 12 || vnormals.len() != n {
        return None;
    }
    let cen = mul(p.pts.iter().fold([0.0; 3], |s, q| add(s, *q)), 1.0 / n as f64);
    // Plücker coordinates of the axis (d, m): d . (x_i x n_i) + m . n_i = 0 for every normal line (x_i, n_i)
    let mut ata = [0.0; 36];
    for (q, nn) in p.pts.iter().zip(vnormals) {
        let row_d = cross(sub(*q, cen), *nn);
        let row = [row_d[0], row_d[1], row_d[2], nn[0], nn[1], nn[2]];
        for i in 0..6 {
            for j in 0..6 {
                ata[i * 6 + j] += row[i] * row[j];
            }
        }
    }
    let (vals, vecs) = sym_eigen(6, &ata);
    // exactly one (the axis) small value: a sphere has three, a cylinder two
    if vals[1] < 1e-3 * vals[5].max(1e-12) {
        return None;
    }
    let (d, mo) = ([vecs[0][0], vecs[0][1], vecs[0][2]], [vecs[0][3], vecs[0][4], vecs[0][5]]);
    let dl = len(d);
    if dl < 1e-6 {
        return None;
    }
    let a = mul(d, 1.0 / dl);
    let mo = mul(mo, 1.0 / dl);
    // a point of the axis: a x m for a unit direction and its moment
    let base = add(cross(a, mo), cen);
    let prof: Vec<(f64, f64)> = p
        .pts
        .iter()
        .map(|q| {
            let r = sub(*q, base);
            let z = dot(r, a);
            (len(sub(r, mul(a, z))), z)
        })
        .collect();
    let ((cr, cz), small) = fit_circle_2d(&prof)?;
    if !(small > 1e-6 && cr > small * 1.0001) {
        // a centre on the axis is a sphere, a centre outside the tube radius a spindle torus: neither is made here
        return None;
    }
    let c0 = add(base, mul(a, cz));
    let q = refine_torus([c0[0], c0[1], c0[2], a[0], a[1], a[2], cr, small], &p.pts);
    let ax = unit([q[3], q[4], q[5]]);
    let (big, small) = (q[6], q[7]);
    if !(small > 1e-6 && big > small * 1.0001 && big.is_finite() && small.is_finite()) {
        return None;
    }
    let s = Surf::Torus {
        c: [q[0], q[1], q[2]],
        a: ax,
        big,
        small,
    };
    (max_dist(&s, &p.pts) <= tol).then_some(s)
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------

struct Region {
    tris: Vec<u32>,
    surf: Surf,
    /// The triangles are one flat group (a plane region) or fitted round ones.
    round: bool,
}

const NONE: u32 = u32::MAX;

/// The largest angle (50 degrees) between the normals of two neighbouring facets of one round surface.
const STEP_COS: f64 = 0.64;

/// The conversion, see the module documentation. `stop` is asked now and then; `None` when it says yes.
pub fn fit(m: &TriMesh, o: &FitOptions, stop: &dyn Fn() -> bool) -> Option<Fitted> {
    let tolerance = o.tolerance;
    let mut stats = FitStats::default();
    // the vertices, rounded when asked; points that became one are one point
    let mut points: Vec<V3> = m.positions.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    if o.round {
        for p in &mut points {
            for c in p.iter_mut() {
                let r = (*c * 100.0).round() / 100.0;
                if (*c - r).abs() <= tolerance * 0.1 {
                    if r != *c {
                        stats.rounded += 1;
                    }
                    *c = r;
                }
            }
        }
    }
    let (points, tris) = {
        let mut seen: HashMap<[i64; 3], u32> = HashMap::new();
        let mut remap = vec![0u32; points.len()];
        let mut unique: Vec<V3> = Vec::new();
        for (i, p) in points.iter().enumerate() {
            let key = [(p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64, (p[2] * 1e5).round() as i64];
            remap[i] = *seen.entry(key).or_insert_with(|| {
                unique.push(*p);
                (unique.len() - 1) as u32
            });
        }
        let t: Vec<[u32; 3]> = m
            .triangles
            .iter()
            .map(|t| [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]])
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
        (unique, t)
    };
    let um = TriMesh {
        positions: points.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect(),
        triangles: tris,
    };
    let n = um.triangles.len();
    if n == 0 {
        return None;
    }
    let adj = neighbours(&um);
    let corners = |t: usize| {
        let tri = um.triangles[t];
        [points[tri[0] as usize], points[tri[1] as usize], points[tri[2] as usize]]
    };
    let normals: Vec<(V3, f64)> = (0..n)
        .map(|t| {
            let c = corners(t);
            let nn = cross(sub(c[1], c[0]), sub(c[2], c[0]));
            let l = len(nn);
            if l > 0.0 {
                (mul(nn, 1.0 / l), l / 2.0)
            } else {
                ([0.0; 3], 0.0)
            }
        })
        .collect();
    let total_area: f64 = normals.iter().map(|x| x.1).sum();
    if stop() {
        return None;
    }
    // ---- flat groups, the biggest triangle first
    let mut order: Vec<usize> = (0..n).filter(|t| normals[*t].1 > 0.0).collect();
    order.sort_by(|a, b| normals[*b].1.total_cmp(&normals[*a].1));
    let cos_t = 2f64.to_radians().cos();
    let mut group_of = vec![NONE; n];
    let mut groups: Vec<Vec<u32>> = Vec::new();
    let mut stamp = vec![0u32; n];
    for (k, &s) in order.iter().enumerate() {
        if k % 16384 == 0 && stop() {
            return None;
        }
        if group_of[s] != NONE {
            continue;
        }
        let (n0, _) = normals[s];
        let off0 = dot(n0, corners(s)[0]);
        let id = groups.len() as u32;
        let mark = s as u32 + 1;
        stamp[s] = mark;
        group_of[s] = id;
        let mut group = vec![s as u32];
        let mut stack = vec![s as u32];
        while let Some(t) = stack.pop() {
            for &o2 in &adj[t as usize] {
                let ou = o2 as usize;
                if group_of[ou] != NONE || stamp[ou] == mark {
                    continue;
                }
                stamp[ou] = mark;
                if normals[ou].1 == 0.0 || dot(normals[ou].0, n0) < cos_t {
                    continue;
                }
                if corners(ou).iter().any(|c| (dot(n0, *c) - off0).abs() > tolerance) {
                    continue;
                }
                group_of[ou] = id;
                group.push(o2);
                stack.push(o2);
            }
        }
        groups.push(group);
    }
    // ---- round patches: the flat groups are the facets of a round surface when several of them lie on one cylinder, cone, sphere or torus
    let ng = groups.len();
    let mut region_of = vec![NONE; n];
    let mut regions: Vec<Region> = Vec::new();
    let any_round = o.allow.cylinders || o.allow.cones || o.allow.spheres || o.allow.tori;
    if any_round {
        let mut g_adj: Vec<Vec<u32>> = vec![Vec::new(); ng];
        for t in 0..n {
            let g = group_of[t];
            if g == NONE {
                continue;
            }
            for &o2 in &adj[t] {
                let h = group_of[o2 as usize];
                if h != NONE && h != g && !g_adj[g as usize].contains(&h) {
                    g_adj[g as usize].push(h);
                }
            }
        }
        let g_area: Vec<f64> = groups.iter().map(|g| g.iter().map(|t| normals[*t as usize].1).sum()).collect();
        let g_normal: Vec<V3> = groups
            .iter()
            .map(|g| {
                let mut s = [0.0; 3];
                for t in g {
                    let (nn, a) = normals[*t as usize];
                    s = add(s, mul(nn, a));
                }
                unit(s)
            })
            .collect();
        let g_verts: Vec<Vec<u32>> = groups.iter().map(|g| patch_vertices(g, &um)).collect();
        let mut vn = vec![[0.0; 3]; points.len()];
        for t in 0..n {
            let (nn, a) = normals[t];
            for &v in &um.triangles[t] {
                vn[v as usize] = add(vn[v as usize], mul(nn, a));
            }
        }
        for v in &mut vn {
            *v = unit(*v);
        }
        // the triangles round every vertex (for the vertex normals that are only used where the whole neighbourhood is known)
        let mut v_tris: Vec<Vec<u32>> = vec![Vec::new(); points.len()];
        if o.allow.tori {
            for (t, tri) in um.triangles.iter().enumerate() {
                for &v in tri {
                    v_tris[v as usize].push(t as u32);
                }
            }
        }
        let ctx = GCtx {
            g_adj: &g_adj,
            g_normal: &g_normal,
            g_verts: &g_verts,
            points: &points,
        };
        // the points of a mesh from a CAD program lie on the surface, so a round surface is held to a tighter tolerance than a flat one (a narrow band of a
        // torus would else pass for a cone)
        let tol_s = tolerance * 0.5 + 1e-3;
        let mut g_region = vec![NONE; ng];
        let mut order_g: Vec<usize> = (0..ng).collect();
        order_g.sort_by(|a, b| g_area[*a].total_cmp(&g_area[*b]));
        let mut failures = 0usize;
        let budget = 3000usize.max(ng / 4);
        let mut tried = vec![false; ng];
        for (cursor, &seed) in order_g.iter().enumerate() {
            if g_region[seed] != NONE || tried[seed] {
                continue;
            }
            if cursor % 1024 == 0 && stop() {
                return None;
            }
            let mut hyp = hypotheses(seed, &ctx, &o.allow, |g| g_region[g] == NONE);
            if o.allow.tori {
                hyp.extend(torus_hypothesis(seed, &ctx, &vn, &v_tris, &group_of, &um, tol_s, |g| g_region[g] == NONE));
            }
            let mut best: Option<(Vec<u32>, Surf)> = None;
            for s in hyp {
                let grown = grow_groups(&s, seed, &ctx, tol_s, |g| g_region[g] == NONE);
                if best.as_ref().is_none_or(|b| grown.len() > b.0.len()) {
                    best = Some((grown, s));
                }
            }
            if let Some((gs, surf)) = best {
                let tris_of: Vec<u32> = gs.iter().flat_map(|g| groups[*g as usize].iter().copied()).collect();
                if gs.len() >= 3 && tris_of.len() >= 6 && !is_flat(&tris_of, &um, &points, &normals, 2.0 * tol_s) {
                    // the surface is fitted again to all the points of the patch (this also asks that the normals turn enough), and the patch grown again
                    if let Some(s2) = refit(&surf, &tris_of, &um, &points, &normals, &vn, tol_s) {
                        let g2 = grow_groups(&s2, seed, &ctx, tol_s, |g| g_region[g] == NONE);
                        let (gs, surf) = if g2.len() >= gs.len() { (g2, s2) } else { (gs, s2) };
                        let tris_of: Vec<u32> = gs.iter().flat_map(|g| groups[*g as usize].iter().copied()).collect();
                        if is_flat(&tris_of, &um, &points, &normals, 2.0 * tol_s) {
                            tried[seed] = true;
                            continue;
                        }
                        let id = regions.len() as u32;
                        for &g in &gs {
                            g_region[g as usize] = id;
                        }
                        for &t in &tris_of {
                            region_of[t as usize] = id;
                        }
                        regions.push(Region { tris: tris_of, surf, round: true });
                        continue;
                    }
                }
            }
            tried[seed] = true;
            failures += 1;
            if failures > budget {
                break;
            }
        }
        // what is left in pieces of small groups that are not small: a torus (a round along a round edge), or a surface the seeds missed
        {
            let mut seen = vec![false; ng];
            let small = |g: usize, gr: &[u32]| gr[g] == NONE && groups[g].len() <= 4;
            for st in 0..ng {
                if seen[st] || !small(st, &g_region) {
                    continue;
                }
                let mut blob = vec![st as u32];
                seen[st] = true;
                let mut head = 0;
                while head < blob.len() {
                    let g = blob[head] as usize;
                    head += 1;
                    for &q in &g_adj[g] {
                        if !seen[q as usize] && small(q as usize, &g_region) {
                            seen[q as usize] = true;
                            blob.push(q);
                        }
                    }
                }
                let tris_of: Vec<u32> = blob.iter().flat_map(|g| groups[*g as usize].iter().copied()).collect();
                if blob.len() < 6 || tris_of.len() > 40000 {
                    continue;
                }
                if is_flat(&tris_of, &um, &points, &normals, 2.0 * tol_s) {
                    continue;
                }
                let patch = make_patch(&tris_of, &um, &points, &normals);
                let mut fits: Vec<Surf> = Vec::new();
                if o.allow.tori {
                    let vns: Vec<V3> = patch_vertices(&tris_of, &um).iter().map(|v| vn[*v as usize]).collect();
                    fits.extend(fit_torus(&patch, &vns, tol_s));
                }
                if o.allow.cylinders {
                    fits.extend(fit_cylinder(&patch, tol_s));
                }
                if o.allow.cones {
                    fits.extend(fit_cone(&patch, tol_s));
                }
                if o.allow.spheres {
                    fits.extend(fit_sphere(&patch, tol_s));
                }
                if let Some(surf) = fits.into_iter().next() {
                    let agree = blob.iter().all(|g| ctx.agrees(&surf, *g as usize));
                    if agree {
                        let id = regions.len() as u32;
                        for &g in &blob {
                            g_region[g as usize] = id;
                        }
                        for &t in &tris_of {
                            region_of[t as usize] = id;
                        }
                        regions.push(Region { tris: tris_of, surf, round: true });
                    }
                }
            }
        }
        // two patches of the same surface that touch are one
        merge_regions(&mut regions, &mut region_of, &adj, tolerance);
        // a patch of fewer than six triangles is nothing
        for r in regions.iter_mut() {
            if r.tris.len() < 6 {
                for &t in &r.tris {
                    region_of[t as usize] = NONE;
                }
                r.tris.clear();
            }
        }
    }
    // ---- flat regions: the groups that are not facets of a round patch
    {
        for g in 0..ng {
            let tris: Vec<u32> = groups[g].iter().copied().filter(|t| region_of[*t as usize] == NONE).collect();
            if tris.is_empty() {
                continue;
            }
            // a group that a round patch took a part of (after the merge of patches) is taken apart into single triangles
            let whole = tris.len() == groups[g].len();
            let parts: Vec<Vec<u32>> = if whole { vec![tris] } else { tris.into_iter().map(|t| vec![t]).collect() };
            for tris in parts {
                let nrm = {
                    let mut s = [0.0; 3];
                    for t in &tris {
                        let (nn, a) = normals[*t as usize];
                        s = add(s, mul(nn, a));
                    }
                    unit(s)
                };
                let mut sum = 0.0;
                let mut cnt = 0.0;
                for t in &tris {
                    for c in corners(*t as usize) {
                        sum += dot(nrm, c);
                        cnt += 1.0;
                    }
                }
                let id = regions.len() as u32;
                for &t in &tris {
                    region_of[t as usize] = id;
                }
                regions.push(Region {
                    tris,
                    surf: Surf::Plane { n: nrm, off: sum / cnt },
                    round: false,
                });
            }
        }
    }
    regions.retain(|r| !r.tris.is_empty());
    if std::env::var_os("ROLIDIUM_DEBUG_FIT").is_some() {
        for r in regions.iter() {
            let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for t in &r.tris {
                for c in corners(*t as usize) {
                    for k in 0..3 {
                        lo[k] = lo[k].min(c[k]);
                        hi[k] = hi[k].max(c[k]);
                    }
                }
            }
            eprintln!("region {} tris {} box {:.1?} {:.1?}", r.tris.len(), if r.round { format!("{:?}", r.surf) } else { "plane".into() }, lo, hi);
        }
    }
    // the region ids are renumbered after the removals
    region_of.iter_mut().for_each(|r| *r = NONE);
    for (i, r) in regions.iter().enumerate() {
        for &t in &r.tris {
            region_of[t as usize] = i as u32;
        }
    }
    if stop() {
        return None;
    }
    // ---- the loops of every region; a flat region whose outline is not one loop is taken apart into single triangles
    let mut loops: Vec<Vec<Vec<u32>>> = Vec::with_capacity(regions.len());
    let mut exploded: Vec<Region> = Vec::new();
    for (ri, r) in regions.iter().enumerate() {
        if ri % 4096 == 0 && stop() {
            return None;
        }
        let nrm = match r.surf {
            Surf::Plane { n, .. } => n,
            _ => {
                let mut s = [0.0; 3];
                for t in &r.tris {
                    let (nn, a) = normals[*t as usize];
                    s = add(s, mul(nn, a));
                }
                let u = unit(s);
                if len(u) < 0.5 {
                    [0.0, 0.0, 1.0]
                } else {
                    u
                }
            }
        };
        let ok = crate::convert::loops_of_pub(&um, &r.tris, &points, nrm);
        let Some(l) = ok else {
            loops.push(Vec::new());
            exploded.push(Region { tris: r.tris.clone(), surf: r.surf, round: r.round });
            continue;
        };
        if !r.round {
            let outlines = crate::convert::split_outlines_pub(l, &points, nrm);
            if outlines.len() != 1 {
                loops.push(Vec::new());
                exploded.push(Region { tris: r.tris.clone(), surf: r.surf, round: false });
                continue;
            }
            loops.push(outlines.into_iter().next().unwrap_or_default());
        } else {
            loops.push(l);
        }
    }
    // a region that failed is made of single flat triangles
    let mut final_regions: Vec<Region> = Vec::new();
    let mut final_loops: Vec<Vec<Vec<u32>>> = Vec::new();
    for (r, l) in regions.into_iter().zip(loops) {
        if l.is_empty() {
            continue;
        }
        final_regions.push(r);
        final_loops.push(l);
    }
    for r in exploded {
        for &t in &r.tris {
            let tri = um.triangles[t as usize];
            let c = corners(t as usize);
            let nn = unit(cross(sub(c[1], c[0]), sub(c[2], c[0])));
            final_regions.push(Region {
                tris: vec![t],
                surf: Surf::Plane { n: nn, off: dot(nn, c[0]) },
                round: false,
            });
            final_loops.push(vec![tri.to_vec()]);
        }
    }
    let regions = final_regions;
    let loops = final_loops;
    for t in 0..n {
        region_of[t] = NONE;
    }
    for (i, r) in regions.iter().enumerate() {
        for &t in &r.tris {
            region_of[t as usize] = i as u32;
        }
    }
    build(&um, points, &regions, &loops, &region_of, &normals, total_area, o, stats, stop)
}

/// A torus from the neighbourhood of a seed group (up to 40 groups reached through small turns): its inner vertices are the points, their normals give the axis.
#[allow(clippy::too_many_arguments)]
fn torus_hypothesis(seed: usize, c: &GCtx, vn: &[V3], v_tris: &[Vec<u32>], group_of: &[u32], m: &TriMesh, tol: f64, ok: impl Fn(usize) -> bool) -> Option<Surf> {
    let _ = m;
    let mut set: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut order = vec![seed as u32];
    set.insert(seed as u32);
    let mut head = 0;
    while head < order.len() && order.len() < 40 {
        let g = order[head] as usize;
        head += 1;
        for &q in &c.g_adj[g] {
            if order.len() >= 40 {
                break;
            }
            if ok(q as usize) && dot(c.g_normal[g], c.g_normal[q as usize]) >= STEP_COS && set.insert(q) {
                order.push(q);
            }
        }
    }
    if order.len() < 8 {
        return None;
    }
    let mut pts = Vec::new();
    let mut vns = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for &g in &order {
        for &v in &c.g_verts[g as usize] {
            if seen.insert(v) && v_tris[v as usize].iter().all(|t| set.contains(&group_of[*t as usize])) {
                pts.push(c.points[v as usize]);
                vns.push(vn[v as usize]);
            }
        }
    }
    if pts.len() < 12 {
        return None;
    }
    let patch = Patch { pts, normals: vec![] };
    fit_torus(&patch, &vns, tol)
}

/// The groups of flat triangles and what is known of them, for the search of round surfaces.
struct GCtx<'a> {
    g_adj: &'a [Vec<u32>],
    g_normal: &'a [V3],
    g_verts: &'a [Vec<u32>],
    points: &'a [V3],
}

impl GCtx<'_> {
    fn centroid(&self, g: usize) -> V3 {
        let v = &self.g_verts[g];
        let mut c = [0.0; 3];
        for i in v {
            c = add(c, self.points[*i as usize]);
        }
        mul(c, 1.0 / v.len().max(1) as f64)
    }

    /// Every point of the group lies on the surface within `tol` and its normal agrees with the surface (within the angle of a facet).
    fn fits(&self, s: &Surf, g: usize, tol: f64) -> bool {
        self.g_verts[g].iter().all(|v| s.dist(self.points[*v as usize]).abs() <= tol) && self.agrees(s, g)
    }

    fn agrees(&self, s: &Surf, g: usize) -> bool {
        dot(self.g_normal[g], s.normal(self.centroid(g))).abs() >= 0.9
    }
}

/// Whether the points of the triangles lie within `tol` of one plane: a patch like that is flat, whatever surface it was fitted with.
fn is_flat(tris: &[u32], m: &TriMesh, points: &[V3], normals: &[(V3, f64)], tol: f64) -> bool {
    let mut s = [0.0; 3];
    for t in tris {
        let (nn, a) = normals[*t as usize];
        s = add(s, mul(nn, a));
    }
    let n = unit(s);
    let vs = patch_vertices(tris, m);
    let mean = vs.iter().fold(0.0, |a, v| a + dot(n, points[*v as usize])) / vs.len().max(1) as f64;
    vs.iter().all(|v| (dot(n, points[*v as usize]) - mean).abs() <= tol)
}

/// The groups that can be reached from `seed` through groups that lie on the surface.
fn grow_groups(s: &Surf, seed: usize, c: &GCtx, tol: f64, ok: impl Fn(usize) -> bool) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<u32> = Vec::new();
    let mut queue: Vec<u32> = Vec::new();
    // the seed may be a bad fit itself; the search starts from the seed or, then, from the neighbours that fit
    seen.insert(seed as u32);
    if ok(seed) && c.fits(s, seed, tol) {
        out.push(seed as u32);
        queue.push(seed as u32);
    } else {
        for &q in &c.g_adj[seed] {
            if seen.insert(q) && ok(q as usize) && c.fits(s, q as usize, tol) {
                out.push(q);
                queue.push(q);
            }
        }
    }
    let mut head = 0;
    while head < queue.len() {
        let g = queue[head] as usize;
        head += 1;
        for &q in &c.g_adj[g] {
            // neighbouring facets of a round surface turn by a few degrees, not by the angle of a corner
            if dot(c.g_normal[g], c.g_normal[q as usize]) < STEP_COS {
                continue;
            }
            if seen.insert(q) && ok(q as usize) && c.fits(s, q as usize, tol) {
                out.push(q);
                queue.push(q);
            }
        }
    }
    out
}

/// Surfaces that the seed group and its neighbours can lie on, from the least samples that determine them: the axis of a cylinder is square to the normals of
/// two facets, a cone has the same slope in the normals of three, and a sphere goes through four points. Facets of one round surface are not the same plane,
/// so a neighbour in the same plane is skipped.
fn hypotheses(seed: usize, c: &GCtx, allow: &Allow, ok: impl Fn(usize) -> bool) -> Vec<Surf> {
    let n0 = c.g_normal[seed];
    let mut out = Vec::new();
    if c.g_verts[seed].len() > 200 {
        return out;
    }
    let other = |g: usize, n: V3| ok(g) && c.g_verts[g].len() <= 200 && len(cross(n, c.g_normal[g])) > 0.04 && dot(n, c.g_normal[g]) >= STEP_COS;
    let others: Vec<usize> = c.g_adj[seed].iter().map(|g| *g as usize).filter(|g| other(*g, n0)).collect();
    let pts_of = |gs: &[usize]| -> Vec<V3> {
        let mut vs: Vec<u32> = gs.iter().flat_map(|g| c.g_verts[*g].iter().copied()).collect();
        vs.sort_unstable();
        vs.dedup();
        vs.iter().map(|v| c.points[*v as usize]).collect()
    };
    for &o in &others {
        let n1 = c.g_normal[o];
        if allow.cylinders {
            let a = unit(cross(n0, n1));
            let pts = pts_of(&[seed, o]);
            let (u, v) = basis(a);
            let mut flat: Vec<(f64, f64)> = Vec::new();
            for p in &pts {
                let q = (dot(*p, u), dot(*p, v));
                if !flat.iter().any(|f| (f.0 - q.0).abs() < 1e-7 && (f.1 - q.1).abs() < 1e-7) {
                    flat.push(q);
                }
            }
            if flat.len() >= 3 {
                if let Some(((cx, cy), r)) = fit_circle_2d(&flat) {
                    if r > 1e-6 && r < 1e5 {
                        let zs = pts.iter().map(|q| dot(*q, a)).sum::<f64>() / pts.len() as f64;
                        out.push(Surf::Cyl { o: add(add(mul(u, cx), mul(v, cy)), mul(a, zs)), a, r });
                    }
                }
            }
        }
        if !(allow.cones || allow.spheres) {
            continue;
        }
        // a third plane
        let mut third: Vec<usize> = Vec::new();
        for &q in c.g_adj[o].iter().chain(c.g_adj[seed].iter()) {
            let q = q as usize;
            if q != seed && q != o && other(q, n0) && len(cross(n1, c.g_normal[q])) > 0.04 && !third.contains(&q) {
                third.push(q);
            }
        }
        for &q in third.iter().take(2) {
            let n2 = c.g_normal[q];
            let pts = pts_of(&[seed, o, q]);
            if allow.cones {
                let ax = cross(sub(n0, n1), sub(n1, n2));
                if len(ax) > 1e-6 {
                    out.extend(fit_cone_axis(&pts, unit(ax), 1e9));
                }
            }
            if allow.spheres {
                let patch = Patch { pts: pts.clone(), normals: vec![] };
                out.extend(fit_sphere(&patch, 1e9));
            }
        }
    }
    out
}

fn patch_vertices(tris: &[u32], m: &TriMesh) -> Vec<u32> {
    let mut v: Vec<u32> = tris.iter().flat_map(|t| m.triangles[*t as usize]).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn make_patch(tris: &[u32], m: &TriMesh, points: &[V3], normals: &[(V3, f64)]) -> Patch {
    Patch {
        pts: patch_vertices(tris, m).iter().map(|v| points[*v as usize]).collect(),
        normals: tris.iter().map(|t| normals[*t as usize]).collect(),
    }
}

/// The same kind of surface fitted again to all the triangles of a patch.
fn refit(s: &Surf, tris: &[u32], m: &TriMesh, points: &[V3], normals: &[(V3, f64)], vn: &[V3], tol: f64) -> Option<Surf> {
    let patch = make_patch(tris, m, points, normals);
    let better = |new: Option<Surf>| new.filter(|n| max_dist(n, &patch.pts) <= tol);
    match s {
        Surf::Cyl { .. } => better(fit_cylinder(&patch, tol)),
        Surf::Cone { .. } => better(fit_cone(&patch, tol)),
        Surf::Sphere { .. } => better(fit_sphere(&patch, tol)),
        Surf::Torus { .. } => {
            let vns: Vec<V3> = patch_vertices(tris, m).iter().map(|v| vn[*v as usize]).collect();
            better(fit_torus(&patch, &vns, tol))
        }
        Surf::Plane { .. } => None,
    }
}

fn same_surface(a: &Surf, b: &Surf, tol: f64) -> bool {
    let par = |x: V3, y: V3| dot(x, y).abs() > 0.99995;
    match (a, b) {
        (Surf::Cyl { o: o1, a: a1, r: r1 }, Surf::Cyl { o: o2, a: a2, r: r2 }) => {
            par(*a1, *a2) && (r1 - r2).abs() <= tol * 2.0 && {
                let d = sub(*o2, *o1);
                len(sub(d, mul(*a1, dot(d, *a1)))) <= tol * 2.0
            }
        }
        (Surf::Sphere { c: c1, r: r1 }, Surf::Sphere { c: c2, r: r2 }) => len(sub(*c1, *c2)) <= tol * 2.0 && (r1 - r2).abs() <= tol * 2.0,
        (Surf::Cone { o: o1, a: a1, k: k1, r0: r1 }, Surf::Cone { o: o2, a: a2, k: k2, r0: r2 }) => {
            // the same cone: the same radius at the same height along the same axis
            dot(*a1, *a2) > 0.99995 && (k1 - k2).abs() < 0.01 && {
                let d = sub(*o2, *o1);
                let z = dot(d, *a1);
                len(sub(d, mul(*a1, z))) <= tol * 2.0 && (r1 + k1 * z - r2).abs() <= tol * 2.0
            }
        }
        (Surf::Torus { c: c1, a: a1, big: b1, small: s1 }, Surf::Torus { c: c2, a: a2, big: b2, small: s2 }) => {
            par(*a1, *a2) && len(sub(*c1, *c2)) <= tol * 2.0 && (b1 - b2).abs() <= tol * 2.0 && (s1 - s2).abs() <= tol * 2.0
        }
        _ => false,
    }
}

fn merge_regions(regions: &mut [Region], region_of: &mut [u32], adj: &[Vec<u32>], tol: f64) {
    let nr = regions.len();
    if nr < 2 {
        return;
    }
    let mut parent: Vec<usize> = (0..nr).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut pairs: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for (t, nb) in adj.iter().enumerate() {
        let a = region_of[t];
        if a == NONE {
            continue;
        }
        for &o in nb {
            let b = region_of[o as usize];
            if b != NONE && b != a {
                pairs.insert((a.min(b), a.max(b)));
            }
        }
    }
    for (a, b) in pairs {
        if same_surface(&regions[a as usize].surf, &regions[b as usize].surf, tol) {
            let (ra, rb) = (find(&mut parent, a as usize), find(&mut parent, b as usize));
            if ra != rb {
                parent[rb] = ra;
            }
        }
    }
    for i in 0..nr {
        let r = find(&mut parent, i);
        if r != i {
            let moved = std::mem::take(&mut regions[i].tris);
            for &t in &moved {
                region_of[t as usize] = r as u32;
            }
            regions[r].tris.extend(moved);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// chains: the boundary between two regions, from one meeting point to the next

struct Chain {
    /// Vertices in the canonical direction; a closed chain does not repeat the first one.
    vs: Vec<u32>,
    closed: bool,
    regions: (u32, u32),
    /// The edges (indices into the edge list) in the order of `vs`, and whether the chain is a single closed curve.
    edges: Vec<(u32, bool)>,
}

#[allow(clippy::too_many_arguments)]
fn build(
    m: &TriMesh,
    points: Vec<V3>,
    regions: &[Region],
    loops: &[Vec<Vec<u32>>],
    region_of: &[u32],
    normals: &[(V3, f64)],
    total_area: f64,
    o: &FitOptions,
    mut stats: FitStats,
    stop: &dyn Fn() -> bool,
) -> Option<Fitted> {
    let tol = o.tolerance;
    // the region across every directed edge
    let mut across: HashMap<(u32, u32), u32> = HashMap::with_capacity(m.triangles.len() * 3);
    for (t, tri) in m.triangles.iter().enumerate() {
        for k in 0..3 {
            across.insert((tri[k], tri[(k + 1) % 3]), region_of[t]);
        }
    }
    // the regions round every vertex
    let mut vregions: Vec<Vec<u32>> = vec![Vec::new(); points.len()];
    for (t, tri) in m.triangles.iter().enumerate() {
        for &v in tri {
            let r = region_of[t];
            if r != NONE && !vregions[v as usize].contains(&r) {
                vregions[v as usize].push(r);
            }
        }
    }
    let mut chains: Vec<Chain> = Vec::new();
    let mut chain_of: HashMap<Vec<u32>, u32> = HashMap::new();
    // for every region loop: the chains it is made of, with their direction
    let mut loop_chains: Vec<Vec<Vec<(u32, bool)>>> = Vec::with_capacity(regions.len());
    for (ri, ls) in loops.iter().enumerate() {
        if ri % 4096 == 0 && stop() {
            return None;
        }
        let mut out_loops = Vec::new();
        for lp in ls {
            let nl = lp.len();
            if nl < 3 {
                continue;
            }
            // the region across the edge from lp[i] to lp[i+1]
            let q: Vec<u32> = (0..nl).map(|i| across.get(&(lp[(i + 1) % nl], lp[i])).copied().unwrap_or(NONE)).collect();
            // breaks: where the region across changes, or where three or more regions meet
            let is_break: Vec<bool> = (0..nl)
                .map(|i| {
                    let prev = q[(i + nl - 1) % nl];
                    prev != q[i] || vregions[lp[i] as usize].len() >= 3
                })
                .collect();
            let first_break = is_break.iter().position(|b| *b);
            let mut parts: Vec<(Vec<u32>, bool, u32)> = Vec::new(); // vertices (open: with the last one), closed, region across
            match first_break {
                None => parts.push((lp.clone(), true, q[0])),
                Some(f) => {
                    let mut i = 0;
                    while i < nl {
                        let start = (f + i) % nl;
                        let mut j = i + 1;
                        while j < nl && !is_break[(f + j) % nl] {
                            j += 1;
                        }
                        let mut vs: Vec<u32> = (i..=j.min(nl)).map(|k| lp[(f + k) % nl]).collect();
                        if vs.len() > 2 && vs.first() == vs.last() {
                            // the one break is the start: a closed curve
                            vs.pop();
                            parts.push((vs, true, q[start]));
                        } else {
                            parts.push((vs, false, q[start]));
                        }
                        i = j;
                    }
                }
            }
            let mut seq = Vec::new();
            for (vs, closed, across_r) in parts {
                // canonical form: the smaller of the list and its reverse (a closed one starts at its smallest vertex)
                let canon = canonical(&vs, closed);
                let forward = canon.0;
                let cvs = canon.1;
                let id = match chain_of.get(&cvs) {
                    Some(id) => *id,
                    None => {
                        let id = chains.len() as u32;
                        chain_of.insert(cvs.clone(), id);
                        chains.push(Chain {
                            vs: cvs,
                            closed,
                            regions: (ri as u32, across_r),
                            edges: Vec::new(),
                        });
                        id
                    }
                };
                seq.push((id, !forward));
            }
            out_loops.push(seq);
        }
        loop_chains.push(out_loops);
    }
    // ---- the edges of every chain
    let mut edges: Vec<FEdge> = Vec::new();
    let mut vtol = vec![1e-6f64; points.len()];
    let mut max_dev: f64 = 0.0;
    for (ci, ch) in chains.iter_mut().enumerate() {
        if ci % 4096 == 0 && stop() {
            return None;
        }
        let curved = |r: u32| r != NONE && regions[r as usize].round;
        let allow_arcs = curved(ch.regions.0) || curved(ch.regions.1);
        let pts: Vec<V3> = ch.vs.iter().map(|v| points[*v as usize]).collect();
        let list = fit_chain(&pts, ch.closed, allow_arcs, o.allow.ellipses, tol, &ch.vs, ch.regions, regions);
        for e in list {
            // the vertices get the tolerance of the edge
            for v in e.v {
                vtol[v as usize] = vtol[v as usize].max(e.tol);
            }
            max_dev = max_dev.max(e.tol.min(tol * 4.0));
            match e.kind {
                0 => stats.lines += 1,
                1 => stats.circles += 1,
                _ => stats.ellipses += 1,
            }
            ch.edges.push((edges.len() as u32, false));
            edges.push(e);
        }
    }
    // ---- the surfaces and faces
    let mut surfaces: Vec<(u32, [f64; 12])> = Vec::new();
    let mut faces: Vec<FFace> = Vec::new();
    let mut single_area = 0.0;
    let mut round_area = 0.0;
    for (ri, r) in regions.iter().enumerate() {
        // the direction the seam of a periodic surface is put to: away from the middle of the patch, or at a vertex of a closed chain
        let mut mid = [0.0; 3];
        let mut cnt = 0.0;
        let mut dev: f64 = 0.0;
        for t in &r.tris {
            for &v in &m.triangles[*t as usize] {
                let p = points[v as usize];
                mid = add(mid, p);
                cnt += 1.0;
                dev = dev.max(r.surf.dist(p).abs());
            }
        }
        let mid = mul(mid, 1.0 / cnt);
        for t in &r.tris {
            for &v in &m.triangles[*t as usize] {
                let d = r.surf.dist(points[v as usize]).abs();
                vtol[v as usize] = vtol[v as usize].max(d * 1.5 + 1e-6);
            }
        }
        max_dev = max_dev.max(dev);
        let area: f64 = r.tris.iter().map(|t| normals[*t as usize].1).sum();
        let (kind, params) = if r.round {
            let closed_vertex = loop_chains[ri]
                .iter()
                .flatten()
                .find(|(c, _)| chains[*c as usize].closed)
                .map(|(c, _)| chains[*c as usize].vs[0]);
            let seam = match closed_vertex {
                // at the first vertex of a closed chain: the seam passes through it
                Some(v) => sub(points[v as usize], axis_point(&r.surf, points[v as usize])),
                None => {
                    let c = axis_point(&r.surf, mid);
                    mul(sub(mid, c), -1.0)
                }
            };
            r.surf.params(seam)
        } else {
            r.surf.params([0.0; 3])
        };
        let sid = surfaces.len() as u32;
        surfaces.push((kind, params));
        match r.surf {
            Surf::Plane { .. } => {
                stats.planes += 1;
                if r.tris.len() == 1 {
                    stats.singles += 1;
                    single_area += area;
                }
            }
            Surf::Cyl { .. } => stats.cylinders += 1,
            Surf::Cone { .. } => stats.cones += 1,
            Surf::Sphere { .. } => stats.spheres += 1,
            Surf::Torus { .. } => stats.tori += 1,
        }
        if r.round {
            round_area += area;
        }
        let mut f = FFace { surface: sid, area, loops: Vec::new() };
        for seq in &loop_chains[ri] {
            let mut l = Vec::new();
            for &(c, rev) in seq {
                let ch = &chains[c as usize];
                // the edges of the chain in the direction of the loop
                let list: Vec<(u32, bool)> = if rev {
                    ch.edges.iter().rev().map(|(e, r)| (*e, !*r)).collect()
                } else {
                    ch.edges.clone()
                };
                for (e, r) in list {
                    l.push((e << 1) | r as u32);
                }
            }
            f.loops.push(l);
        }
        faces.push(f);
    }
    // a point cannot be more tolerant than a good part of the shortest edge it ends
    let mut shortest = vec![f64::MAX; points.len()];
    for e in &edges {
        if e.v[0] != e.v[1] {
            let d = len(sub(points[e.v[0] as usize], points[e.v[1] as usize]));
            for v in e.v {
                shortest[v as usize] = shortest[v as usize].min(d);
            }
        }
    }
    for (t, sh) in vtol.iter_mut().zip(&shortest) {
        *t = t.min((sh * 0.4).max(1e-7));
    }
    stats.max_deviation = max_dev;
    stats.single_area = if total_area > 0.0 { single_area / total_area } else { 0.0 };
    stats.round_area = if total_area > 0.0 { round_area / total_area } else { 0.0 };
    Some(Fitted {
        points,
        point_tol: vtol,
        surfaces,
        edges,
        faces,
        stats,
    })
}

/// The foot of `p` on the axis of a surface of revolution (the point itself for a plane or a sphere's centre).
fn axis_point(s: &Surf, p: V3) -> V3 {
    match *s {
        Surf::Cyl { o, a, .. } | Surf::Cone { o, a, .. } => add(o, mul(a, dot(sub(p, o), a))),
        Surf::Torus { c, a, .. } => add(c, mul(a, dot(sub(p, c), a))),
        Surf::Sphere { c, .. } => c,
        Surf::Plane { .. } => p,
    }
}

/// The chain in its canonical direction: `(true, vs)` when `vs` already is it, `(false, reversed)` otherwise. A closed chain starts at its smallest vertex
/// and goes the way of the smaller second vertex.
fn canonical(vs: &[u32], closed: bool) -> (bool, Vec<u32>) {
    if closed {
        let n = vs.len();
        let (k, _) = vs.iter().enumerate().min_by_key(|x| *x.1).unwrap_or((0, &0));
        let fwd: Vec<u32> = (0..n).map(|i| vs[(k + i) % n]).collect();
        let bwd: Vec<u32> = (0..n).map(|i| vs[(k + n - i) % n]).collect();
        if fwd[1.min(n - 1)] <= bwd[1.min(n - 1)] {
            (true, fwd)
        } else {
            (false, bwd)
        }
    } else {
        let rev: Vec<u32> = vs.iter().rev().copied().collect();
        if vs <= rev.as_slice() {
            (true, vs.to_vec())
        } else {
            (false, rev)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------
// the curves of the chains

fn line_dist(p: V3, a: V3, b: V3) -> f64 {
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2 < 1e-30 {
        return len(sub(p, a));
    }
    let t = dot(sub(p, a), d) / l2;
    len(sub(p, add(a, mul(d, t))))
}

fn line_edge(a: u32, b: u32, tol: f64) -> FEdge {
    FEdge { kind: 0, v: [a, b], params: [0.0; 12], tol }
}

/// Edges for the chain with the vertices `vs` (points `pts`): one curve when a line, a circle or an ellipse fits all of it, else split at the point that
/// is the farthest from the chord, and at last the straight pieces between the vertices.
#[allow(clippy::too_many_arguments)]
fn fit_chain(
    pts: &[V3],
    closed: bool,
    allow_arcs: bool,
    allow_ellipses: bool,
    tol: f64,
    vs: &[u32],
    _regions: (u32, u32),
    _all: &[Region],
) -> Vec<FEdge> {
    let n = pts.len();
    if closed {
        if allow_arcs && n >= 8 {
            if let Some(c) = fit_arc(pts, true, tol, allow_ellipses, vs[0], vs[0]) {
                return vec![c];
            }
        }
        // a closed chain that is not one curve: two open halves, split at the first vertex and the one opposite
        if n >= 4 {
            let half = n / 2;
            let a: Vec<V3> = (0..=half).map(|i| pts[i]).collect();
            let av: Vec<u32> = (0..=half).map(|i| vs[i]).collect();
            let b: Vec<V3> = (half..=n).map(|i| pts[i % n]).collect();
            let bv: Vec<u32> = (half..=n).map(|i| vs[i % n]).collect();
            let mut out = fit_open(&a, allow_arcs, allow_ellipses, tol, &av);
            out.extend(fit_open(&b, allow_arcs, allow_ellipses, tol, &bv));
            return out;
        }
        return (0..n).map(|i| line_edge(vs[i], vs[(i + 1) % n], 1e-6)).collect();
    }
    fit_open(pts, allow_arcs, allow_ellipses, tol, vs)
}

fn fit_open(pts: &[V3], allow_arcs: bool, allow_ellipses: bool, tol: f64, vs: &[u32]) -> Vec<FEdge> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    if n == 2 {
        return vec![line_edge(vs[0], vs[1], 1e-6)];
    }
    let (a, b) = (pts[0], pts[n - 1]);
    let (mut far, mut far_d) = (0, 0.0);
    for (i, p) in pts.iter().enumerate().take(n - 1).skip(1) {
        let d = line_dist(*p, a, b);
        if d > far_d {
            far_d = d;
            far = i;
        }
    }
    if far_d <= tol * 0.25 {
        return vec![line_edge(vs[0], vs[n - 1], far_d * 1.2 + 1e-6)];
    }
    if allow_arcs && n >= 3 {
        if let Some(c) = fit_arc(pts, false, tol, allow_ellipses, vs[0], vs[n - 1]) {
            return vec![c];
        }
    }
    if n == 3 {
        return vec![line_edge(vs[0], vs[1], 1e-6), line_edge(vs[1], vs[2], 1e-6)];
    }
    // split where the chain leaves its chord the most
    let mut out = fit_open(&pts[..=far], allow_arcs, allow_ellipses, tol, &vs[..=far]);
    out.extend(fit_open(&pts[far..], allow_arcs, allow_ellipses, tol, &vs[far..]));
    out
}

/// A circle (or an ellipse) through the points of a chain: in one plane, within the tolerance; an open one must not turn back.
fn fit_arc(pts: &[V3], closed: bool, tol: f64, allow_ellipse: bool, va: u32, vb: u32) -> Option<FEdge> {
    let n = pts.len();
    let mean = mul(pts.iter().fold([0.0; 3], |s, p| add(s, *p)), 1.0 / n as f64);
    let mut cov = [0.0; 9];
    for p in pts {
        let d = sub(*p, mean);
        for i in 0..3 {
            for j in 0..3 {
                cov[i * 3 + j] += d[i] * d[j];
            }
        }
    }
    let (vals, vecs) = sym_eigen(3, &cov);
    let nrm = unit([vecs[0][0], vecs[0][1], vecs[0][2]]);
    // flat within the tolerance
    let flat_dev = pts.iter().map(|p| dot(nrm, sub(*p, mean)).abs()).fold(0.0, f64::max);
    if flat_dev > tol * 0.5 || vals[1] < 1e-12 {
        return None;
    }
    let (u, v) = basis(nrm);
    let flat: Vec<(f64, f64)> = pts.iter().map(|p| (dot(sub(*p, mean), u), dot(sub(*p, mean), v))).collect();
    let fit = fit_circle_2d(&flat);
    let circle_ok = fit.filter(|((cx, cy), r)| {
        *r > 1e-6
            && *r < 1e6
            && flat.iter().map(|q| (((q.0 - cx).powi(2) + (q.1 - cy).powi(2)).sqrt() - r).abs()).fold(0.0, f64::max) <= tol * 0.5
    });
    if let Some(((cx, cy), r)) = circle_ok {
        // the direction of the walk about the normal; the normal is turned so that the walk is counter-clockwise
        let ang: Vec<f64> = flat.iter().map(|q| (q.1 - cy).atan2(q.0 - cx)).collect();
        let mut turn = 0.0;
        let mut mono = true;
        let steps = if closed { n } else { n - 1 };
        for i in 0..steps {
            let mut d = ang[(i + 1) % n] - ang[i];
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            turn += d;
        }
        if !closed && turn.abs() > std::f64::consts::TAU - 0.05 {
            mono = false;
        }
        // every step goes the same way
        let sign = if turn >= 0.0 { 1.0 } else { -1.0 };
        for i in 0..steps {
            let mut d = ang[(i + 1) % n] - ang[i];
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            if d * sign < -0.02 {
                mono = false;
            }
        }
        if closed && (turn.abs() - std::f64::consts::TAU).abs() > 0.2 {
            mono = false;
        }
        if !mono {
            return None;
        }
        let normal = mul(nrm, sign);
        let xdir = if sign > 0.0 { u } else { u };
        let centre = add(add(mean, mul(u, cx)), mul(v, cy));
        let mut p = [0.0; 12];
        p[0..3].copy_from_slice(&centre);
        p[3..6].copy_from_slice(&normal);
        p[6..9].copy_from_slice(&xdir);
        p[9] = r;
        let dev = flat.iter().map(|q| (((q.0 - cx).powi(2) + (q.1 - cy).powi(2)).sqrt() - r).abs()).fold(0.0, f64::max).max(flat_dev);
        return Some(FEdge { kind: 1, v: [va, vb], params: p, tol: dev * 1.2 + 1e-6 });
    }
    if !allow_ellipse {
        return None;
    }
    fit_ellipse(&flat, mean, u, v, nrm, closed, tol, va, vb)
}

/// An ellipse through points of a plane (a conic fit); only closed chains and arcs that do not turn back.
#[allow(clippy::too_many_arguments)]
fn fit_ellipse(flat: &[(f64, f64)], mean: V3, u: V3, v: V3, nrm: V3, closed: bool, tol: f64, va: u32, vb: u32) -> Option<FEdge> {
    let n = flat.len();
    if n < 8 {
        return None;
    }
    // x^2 + b xy + c y^2 + d x + e y = 1 (the centre of the ellipse is not at the origin, which the points are centred on)
    let mut ata = [0.0; 25];
    let mut atb = [0.0; 5];
    for q in flat {
        let (x, y) = *q;
        let row = [x * x, x * y, y * y, x, y];
        for i in 0..5 {
            for j in 0..5 {
                ata[i * 5 + j] += row[i] * row[j];
            }
            atb[i] += row[i];
        }
    }
    let s = solve(5, &ata, &atb)?;
    let (a, b, c, d, e) = (s[0], s[1], s[2], s[3], s[4]);
    let det = 4.0 * a * c - b * b;
    if det <= 1e-18 {
        return None;
    }
    // centre
    let cx = (b * e - 2.0 * c * d) / det;
    let cy = (b * d - 2.0 * a * e) / det;
    let k = 1.0 + a * cx * cx + b * cx * cy + c * cy * cy;
    if k <= 0.0 {
        return None;
    }
    // the matrix of the quadratic form [[a, b/2], [b/2, c]] / k: eigenvalues 1/major^2 and 1/minor^2
    let tr = a + c;
    let disc = ((a - c) * (a - c) + b * b).sqrt();
    let (l1, l2) = ((tr - disc) / 2.0, (tr + disc) / 2.0);
    if l1 <= 1e-18 {
        return None;
    }
    let (major, minor) = ((k / l1).sqrt(), (k / l2).sqrt());
    let theta = 0.5 * b.atan2(a - c);
    // the direction of the major axis (the eigenvector of l1)
    let mdir = (theta.cos(), theta.sin());
    // choose the one that belongs to the smaller eigenvalue
    let q = |x: f64, y: f64| a * x * x + b * x * y + c * y * y;
    let (mx, my) = if q(mdir.0, mdir.1) <= q(-mdir.1, mdir.0) { mdir } else { (-mdir.1, mdir.0) };
    if !(major.is_finite() && minor.is_finite()) || minor < 1e-6 || major > 1e6 {
        return None;
    }
    // the distance of the points from the ellipse (about, by the algebraic value)
    let dev = flat
        .iter()
        .map(|p| {
            let (dx, dy) = (p.0 - cx, p.1 - cy);
            let (xr, yr) = (dx * mx + dy * my, -dx * my + dy * mx);
            let g = ((xr / major).powi(2) + (yr / minor).powi(2)).sqrt();
            // a first order distance
            (g - 1.0).abs() * minor.min(major) * 1.0f64.max(1.0)
        })
        .fold(0.0, f64::max);
    if dev > tol * 0.5 {
        return None;
    }
    // the walk about the normal must be counter-clockwise from the first to the last point and not turn back
    let ang: Vec<f64> = flat
        .iter()
        .map(|p| {
            let (dx, dy) = (p.0 - cx, p.1 - cy);
            let (xr, yr) = (dx * mx + dy * my, -dx * my + dy * mx);
            (yr / minor).atan2(xr / major)
        })
        .collect();
    let mut turn = 0.0;
    let mut steps = Vec::new();
    let nsteps = if closed { n } else { n - 1 };
    for i in 0..nsteps {
        let mut dd = ang[(i + 1) % n] - ang[i];
        while dd > std::f64::consts::PI {
            dd -= std::f64::consts::TAU;
        }
        while dd < -std::f64::consts::PI {
            dd += std::f64::consts::TAU;
        }
        turn += dd;
        steps.push(dd);
    }
    let sign = if turn >= 0.0 { 1.0 } else { -1.0 };
    if steps.iter().any(|d| d * sign < -0.02) {
        return None;
    }
    if closed && (turn.abs() - std::f64::consts::TAU).abs() > 0.2 {
        return None;
    }
    if !closed && turn.abs() > std::f64::consts::TAU - 0.05 {
        return None;
    }
    let normal = mul(nrm, sign);
    // the plane axes (u, v) are right-handed about nrm; with the normal turned the x direction of the ellipse stays in the plane
    let xdir = add(mul(u, mx), mul(v, my));
    let centre = add(add(mean, mul(u, cx)), mul(v, cy));
    let mut p = [0.0; 12];
    p[0..3].copy_from_slice(&centre);
    p[3..6].copy_from_slice(&normal);
    p[6..9].copy_from_slice(&xdir);
    p[9] = major;
    p[10] = minor;
    Some(FEdge { kind: 2, v: [va, vb], params: p, tol: dev * 1.2 + 1e-6 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen;

    fn opts() -> FitOptions {
        FitOptions {
            tolerance: 0.02,
            round: false,
            allow: Allow::ALL,
        }
    }

    #[test]
    fn a_cylinder_of_facets_is_one_cylinder_and_two_discs() {
        let m = gen::cylinder(5.0, 20.0, 24);
        let f = fit(&m, &opts(), &|| false).unwrap();
        eprintln!("{:?}", f.stats);
        assert_eq!(f.stats.cylinders, 1, "{:?}", f.stats);
        assert_eq!(f.stats.planes, 2, "{:?}", f.stats);
        assert_eq!(f.stats.circles, 2, "{:?}", f.stats);
        assert!((f.surfaces.iter().find(|s| s.0 == 1).unwrap().1[6] - 5.0).abs() < 1e-3);
    }
}
