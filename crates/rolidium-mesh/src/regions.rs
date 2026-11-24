// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Regions of a mesh: planar walls (found by growing from a triangle over neighbours whose normals agree and whose points lie in one plane) and round
//! holes and pins (smooth patches whose normals are square to one axis, with a circle fitted to the points).

use crate::check::{runs, sorted_edges};
use crate::{cross, dot, len, mul, sub, unit, TriMesh, V3};

#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    pub triangles: Vec<u32>,
    /// The outward unit normal and the offset: the points of the wall satisfy `normal . x = offset`.
    pub normal: V3,
    pub offset: f64,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cylinder {
    pub triangles: Vec<u32>,
    pub axis: V3,
    /// A point of the axis.
    pub point: V3,
    pub radius: f64,
    /// The wall faces the axis (a hole), else it is a pin.
    pub is_hole: bool,
    /// The extent along the axis, from the point.
    pub z_range: (f64, f64),
}

#[derive(Debug, Clone, Default)]
pub struct Regions {
    pub planes: Vec<Plane>,
    pub cylinders: Vec<Cylinder>,
    /// The region of every triangle: 0 none, 1 + index in `planes`, or `1 << 30` + 1 + index in `cylinders`.
    pub of_triangle: Vec<u32>,
}

pub const CYL_FLAG: u32 = 1 << 30;

/// Two neighbouring triangles whose normals differ by more than about 6 degrees have a crease between them.
const CREASE_COS: f64 = 0.9945;

/// The smallest wall that is kept when it is not a big one: this part of the surface of the whole mesh.
const MIN_WALL_AREA: f64 = 1e-4;

/// Neighbours of every triangle (through edges that two triangles share).
pub fn neighbours(m: &TriMesh) -> Vec<Vec<u32>> {
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); m.triangles.len()];
    let edges = sorted_edges(m);
    for run in runs(&edges) {
        if run.len() == 2 {
            let (a, b) = (run[0].1 >> 1, run[1].1 >> 1);
            adj[a as usize].push(b);
            adj[b as usize].push(a);
        }
    }
    adj
}

/// Eigenvector of the smallest eigenvalue of a symmetric 3x3 matrix (Jacobi).
fn smallest_eigenvector(mut a: [[f64; 3]; 3]) -> V3 {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..30 {
        let (mut p, mut q, mut big) = (0, 1, 0.0);
        for i in 0..3 {
            for j in (i + 1)..3 {
                if a[i][j].abs() > big {
                    (p, q, big) = (i, j, a[i][j].abs());
                }
            }
        }
        if big < 1e-14 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let (c, s) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
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
    let mut best = 0;
    for i in 1..3 {
        if a[i][i] < a[best][best] {
            best = i;
        }
    }
    [v[0][best], v[1][best], v[2][best]]
}

/// Finds the planar walls (at least `min_triangles` triangles) and the round holes and pins. `angle_deg` is how much the normals of a planar wall may
/// differ from the first one (a few degrees); the points of a wall must lie within `0.2 %` of the size of the mesh of its plane.
pub fn find(m: &TriMesh, angle_deg: f64, min_triangles: usize) -> Regions {
    find_with(m, angle_deg, min_triangles, &mut |_| true).expect("nothing cancels it")
}

/// The same, reporting how far it is (0 to 1) now and then; when `progress` answers `false` the search stops and the answer is `None`.
pub fn find_with(
    m: &TriMesh,
    angle_deg: f64,
    min_triangles: usize,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Option<Regions> {
    let n = m.triangles.len();
    if !progress(0.0) {
        return None;
    }
    let adj = neighbours(m);
    if !progress(0.25) {
        return None;
    }
    let normals: Vec<(V3, f64)> = (0..n).map(|t| m.normal_area(t)).collect();
    let size = m
        .bounding_box()
        .map(|(lo, hi)| len(sub(hi, lo)))
        .unwrap_or(1.0)
        .max(1e-9);
    let dtol = size * 0.002;
    let cos_t = angle_deg.to_radians().cos();
    let mut of = vec![0u32; n];
    let mut out = Regions::default();
    // planes, the biggest triangles first so that a wall grows from a clear start
    let mut tried = vec![0u32; n];
    let mut order: Vec<usize> = (0..n).filter(|t| normals[*t].1 > 0.0).collect();
    order.sort_by(|a, b| normals[*b].1.total_cmp(&normals[*a].1));
    let order_len = order.len().max(1);
    for (k, &s) in order.iter().enumerate() {
        if k % 8192 == 0 && !progress(0.25 + 0.45 * k as f32 / order_len as f32) {
            return None;
        }
        if of[s] != 0 {
            continue;
        }
        let (n0, _) = normals[s];
        let p0 = m.corners(s)[0];
        let off0 = dot(n0, p0);
        let mut region = vec![s as u32];
        let mut claimed = vec![s as u32];
        let mut stack = vec![s as u32];
        let stamp = s as u32 + 1;
        tried[s] = stamp;
        while let Some(t) = stack.pop() {
            for &o in &adj[t as usize] {
                if of[o as usize] != 0 || tried[o as usize] == stamp {
                    continue;
                }
                tried[o as usize] = stamp;
                let (no, ao) = normals[o as usize];
                if ao == 0.0 || dot(no, n0) < cos_t {
                    continue;
                }
                if m.corners(o as usize)
                    .iter()
                    .any(|c| (dot(n0, *c) - off0).abs() > dtol)
                {
                    continue;
                }
                region.push(o);
                claimed.push(o);
                stack.push(o);
            }
        }
        if region.len() >= min_triangles {
            let area: f64 = region.iter().map(|t| normals[*t as usize].1).sum();
            out.planes.push(Plane {
                triangles: region,
                normal: n0,
                offset: off0,
                area,
            });
            let id = out.planes.len() as u32;
            for t in claimed {
                of[t as usize] = id;
            }
        }
    }
    // a finely divided hole makes strips of a few degrees that look like little planes: the planes that are small against the whole surface are
    // dissolved first, the round holes are looked for among the triangles that are left, and a small plane that is no part of a hole comes back
    let total: f64 = normals.iter().map(|x| x.1).sum();
    let small_limit = total * 0.005;
    let all = std::mem::take(&mut out.planes);
    let (big, small): (Vec<Plane>, Vec<Plane>) = all.into_iter().partition(|p| p.area >= small_limit);
    of.iter_mut().for_each(|x| *x = 0);
    for p in big {
        out.planes.push(p);
        let id = out.planes.len() as u32;
        for t in &out.planes.last().unwrap().triangles {
            of[*t as usize] = id;
        }
    }
    // round holes and pins: smooth patches of the triangles that are left
    let smooth = 35f64.to_radians().cos();
    let mut seen = vec![false; n];
    for s in 0..n {
        if s % 65536 == 0 && !progress(0.7 + 0.28 * s as f32 / n.max(1) as f32) {
            return None;
        }
        if seen[s] || of[s] != 0 || normals[s].1 == 0.0 {
            continue;
        }
        let mut patch = vec![s as u32];
        seen[s] = true;
        let mut stack = vec![s as u32];
        while let Some(t) = stack.pop() {
            for &o in &adj[t as usize] {
                if seen[o as usize] || of[o as usize] != 0 || normals[o as usize].1 == 0.0 {
                    continue;
                }
                if dot(normals[t as usize].0, normals[o as usize].0) >= smooth {
                    seen[o as usize] = true;
                    patch.push(o);
                    stack.push(o);
                }
            }
        }
        if patch.len() < 8 {
            continue;
        }
        if let Some(c) = fit_cylinder(m, &patch, &normals, size) {
            out.cylinders.push(c);
            let id = CYL_FLAG + out.cylinders.len() as u32;
            for t in &out.cylinders.last().unwrap().triangles {
                of[*t as usize] = id;
            }
        }
    }
    // a small patch of a smooth surface (a finely divided ball) is flat to within the tolerance, but it is no wall: a wall has a crease round it
    let mut member = vec![u32::MAX; n];
    let sharp = |p: &Plane, member: &mut Vec<u32>, tag: u32| -> bool {
        for t in &p.triangles {
            member[*t as usize] = tag;
        }
        let (mut border, mut creased) = (0usize, 0usize);
        for &t in &p.triangles {
            let mut shared = 0;
            for &o in &adj[t as usize] {
                shared += 1;
                if member[o as usize] != tag {
                    border += 1;
                    if dot(normals[t as usize].0, normals[o as usize].0) < CREASE_COS {
                        creased += 1;
                    }
                }
            }
            // an edge that no other triangle shares is an open border: it counts as a crease
            let open = 3usize.saturating_sub(shared);
            border += open;
            creased += open;
        }
        border > 0 && creased * 2 >= border
    };
    for (k, p) in small.into_iter().enumerate() {
        // a patch that is a hundredth of a per cent of the surface is no wall (a scanned or sculpted model has thousands of such flat spots)
        if p.area >= total * MIN_WALL_AREA
            && p.triangles.iter().all(|t| of[*t as usize] == 0)
            && sharp(&p, &mut member, k as u32)
        {
            out.planes.push(p);
            let id = out.planes.len() as u32;
            for t in &out.planes.last().unwrap().triangles {
                of[*t as usize] = id;
            }
        }
    }
    out.of_triangle = of;
    progress(1.0);
    Some(out)
}

fn fit_cylinder(m: &TriMesh, patch: &[u32], normals: &[(V3, f64)], size: f64) -> Option<Cylinder> {
    // the normals of a cylinder are all square to its axis: the axis is the direction the normals have the least of
    let mut cov = [[0.0; 3]; 3];
    for &t in patch {
        let (nrm, a) = normals[t as usize];
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += a * nrm[i] * nrm[j];
            }
        }
    }
    let axis = unit(smallest_eigenvector(cov));
    if len(axis) < 0.5 {
        return None;
    }
    let off: f64 = patch
        .iter()
        .map(|t| dot(normals[*t as usize].0, axis).abs() * normals[*t as usize].1)
        .sum::<f64>()
        / patch.iter().map(|t| normals[*t as usize].1).sum::<f64>();
    if off > 0.06 {
        return None;
    }
    // a circle through the points projected on the plane square to the axis (algebraic fit)
    let a1 = if axis[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = unit(cross(axis, a1));
    let w = cross(axis, u);
    let mut pts: Vec<(f64, f64, f64)> = Vec::new();
    for &t in patch {
        for c in m.corners(t as usize) {
            pts.push((dot(c, u), dot(c, w), dot(c, axis)));
        }
    }
    let k = pts.len() as f64;
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / k,
        pts.iter().map(|p| p.1).sum::<f64>() / k,
    );
    let (mut suu, mut svv, mut suv, mut suuu, mut svvv, mut suvv, mut svuu) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in &pts {
        let (x, y) = (p.0 - mx, p.1 - my);
        suu += x * x;
        svv += y * y;
        suv += x * y;
        suuu += x * x * x;
        svvv += y * y * y;
        suvv += x * y * y;
        svuu += y * x * x;
    }
    let det = suu * svv - suv * suv;
    if det.abs() < 1e-12 {
        return None;
    }
    let (b1, b2) = (0.5 * (suuu + suvv), 0.5 * (svvv + svuu));
    let (cx, cy) = ((b1 * svv - b2 * suv) / det, (suu * b2 - suv * b1) / det);
    let centre = (cx + mx, cy + my);
    let radius = (pts
        .iter()
        .map(|p| ((p.0 - centre.0).powi(2) + (p.1 - centre.1).powi(2)).sqrt())
        .sum::<f64>())
        / k;
    let resid = pts
        .iter()
        .map(|p| (((p.0 - centre.0).powi(2) + (p.1 - centre.1).powi(2)).sqrt() - radius).abs())
        .fold(0.0, f64::max);
    if radius < size * 1e-4 || resid > radius * 0.03 + size * 1e-4 {
        return None;
    }
    // the patch has to go round a good part of the circle
    let mut sector = [false; 12];
    for p in &pts {
        let a = (p.1 - centre.1).atan2(p.0 - centre.0);
        sector[(((a + std::f64::consts::PI) / std::f64::consts::TAU * 12.0) as usize).min(11)] = true;
    }
    if sector.iter().filter(|s| **s).count() < 8 {
        return None;
    }
    let (zlo, zhi) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.2), a.1.max(p.2)));
    let point = [
        u[0] * centre.0 + w[0] * centre.1,
        u[1] * centre.0 + w[1] * centre.1,
        u[2] * centre.0 + w[2] * centre.1,
    ];
    // a hole faces the axis: the normal points from the surface towards the axis
    let mut inward = 0.0;
    for &t in patch {
        let c = m.corners(t as usize);
        let mid = mul(
            [
                c[0][0] + c[1][0] + c[2][0],
                c[0][1] + c[1][1] + c[2][1],
                c[0][2] + c[1][2] + c[2][2],
            ],
            1.0 / 3.0,
        );
        let radial = sub(mid, point);
        let radial = sub(radial, mul(axis, dot(radial, axis)));
        inward += dot(normals[t as usize].0, radial) * normals[t as usize].1;
    }
    Some(Cylinder {
        triangles: patch.to_vec(),
        axis,
        point: sub(point, mul(axis, 0.0)),
        radius,
        is_hole: inward < 0.0,
        z_range: (zlo - dot(point, axis), zhi - dot(point, axis)),
    })
}
