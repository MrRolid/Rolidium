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
    /// How much of the circle the surface goes round (radians): a hole or a pin goes all the way, a rounded edge only a part.
    pub arc: f64,
    /// The number of flat facets the surface is made of when the mesh is coarse (a cylinder of 8 to 24 sides); 0 for a smooth surface.
    pub facets: u32,
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
    // every flat group of triangles, big or small, is a unit: the facets of a coarse cylinder are small planes (often of two triangles) that only together mean something
    let mut unit_of = vec![u32::MAX; n];
    let mut unit_start: Vec<u32> = vec![0];
    let mut unit_tris: Vec<u32> = Vec::with_capacity(n);
    let mut unit_normal: Vec<V3> = Vec::new();
    let mut plane_unit: Vec<u32> = Vec::new();
    // planes, the biggest triangles first so that a wall grows from a clear start
    let mut tried = vec![0u32; n];
    let mut order: Vec<usize> = (0..n).filter(|t| normals[*t].1 > 0.0).collect();
    order.sort_by(|a, b| normals[*b].1.total_cmp(&normals[*a].1));
    let order_len = order.len().max(1);
    for (k, &s) in order.iter().enumerate() {
        if k % 8192 == 0 && !progress(0.25 + 0.45 * k as f32 / order_len as f32) {
            return None;
        }
        if unit_of[s] != u32::MAX {
            continue;
        }
        let (n0, _) = normals[s];
        let p0 = m.corners(s)[0];
        let off0 = dot(n0, p0);
        let mut region = vec![s as u32];
        let mut stack = vec![s as u32];
        let stamp = s as u32 + 1;
        tried[s] = stamp;
        while let Some(t) = stack.pop() {
            for &o in &adj[t as usize] {
                if unit_of[o as usize] != u32::MAX || tried[o as usize] == stamp {
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
                stack.push(o);
            }
        }
        let unit = unit_normal.len() as u32;
        for &t in &region {
            unit_of[t as usize] = unit;
            unit_tris.push(t);
        }
        unit_start.push(unit_tris.len() as u32);
        unit_normal.push(n0);
        if region.len() >= min_triangles {
            let area: f64 = region.iter().map(|t| normals[*t as usize].1).sum();
            out.planes.push(Plane {
                triangles: region,
                normal: n0,
                offset: off0,
                area,
            });
            plane_unit.push(unit);
        }
    }
    let total: f64 = normals.iter().map(|x| x.1).sum();
    // the facets of a coarse cylinder (8 to 24 sides, or finer ones that are still flat strips) are joined into one round surface
    let (facet_cyls, claimed) = facet_cylinders(
        m,
        &adj,
        &normals,
        &Units {
            of: &unit_of,
            start: &unit_start,
            tris: &unit_tris,
            normal: &unit_normal,
        },
        size,
        total,
        progress,
    )?;
    let all: Vec<Plane> = std::mem::take(&mut out.planes)
        .into_iter()
        .zip(plane_unit.iter())
        .filter(|(_, u)| !claimed[**u as usize])
        .map(|(p, _)| p)
        .collect();
    // a finely divided hole makes strips of a few degrees that look like little planes: the planes that are small against the whole surface are
    // dissolved first, the round holes are looked for among the triangles that are left, and a small plane that is no part of a hole comes back
    let small_limit = total * 0.005;
    let (big, small): (Vec<Plane>, Vec<Plane>) = all.into_iter().partition(|p| p.area >= small_limit);
    of.iter_mut().for_each(|x| *x = 0);
    for c in facet_cyls {
        out.cylinders.push(c);
        let id = CYL_FLAG + out.cylinders.len() as u32;
        for t in &out.cylinders.last().unwrap().triangles {
            of[*t as usize] = id;
        }
    }
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
        arc: sector.iter().filter(|s| **s).count() as f64 / 12.0 * std::f64::consts::TAU,
        facets: 0,
    })
}

/// The flat groups of triangles ("units"): which unit a triangle belongs to (`u32::MAX` none), where each unit's triangles are in `tris`, and its normal.
struct Units<'a> {
    of: &'a [u32],
    start: &'a [u32],
    tris: &'a [u32],
    normal: &'a [V3],
}

impl Units<'_> {
    fn triangles(&self, u: usize) -> &[u32] {
        &self.tris[self.start[u] as usize..self.start[u + 1] as usize]
    }
}

/// A circle through points of a plane (algebraic fit): the centre and the radius.
fn circle_fit(pts: &[(f64, f64)]) -> Option<((f64, f64), f64)> {
    let k = pts.len() as f64;
    if pts.len() < 3 {
        return None;
    }
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / k,
        pts.iter().map(|p| p.1).sum::<f64>() / k,
    );
    let (mut suu, mut svv, mut suv, mut suuu, mut svvv, mut suvv, mut svuu) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in pts {
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
    let radius = pts
        .iter()
        .map(|p| ((p.0 - centre.0).powi(2) + (p.1 - centre.1).powi(2)).sqrt())
        .sum::<f64>()
        / k;
    if radius.is_finite() && radius > 0.0 {
        Some((centre, radius))
    } else {
        None
    }
}

/// Joins the flat units that meet along edges parallel to one axis, have normals square to it, turn by 1.5 to 50 degrees from one to the next and
/// together lie on one circle. Returns the cylinders and which units they took.
fn facet_cylinders(
    m: &TriMesh,
    adj: &[Vec<u32>],
    normals: &[(V3, f64)],
    units: &Units,
    size: f64,
    total: f64,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Option<(Vec<Cylinder>, Vec<bool>)> {
    let nu = units.normal.len();
    let mut claimed = vec![false; nu];
    let mut found: Vec<Cylinder> = Vec::new();
    let (cos_hi, cos_lo) = (1.5f64.to_radians().cos(), 50f64.to_radians().cos());
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for t in 0..adj.len() {
        let ua = units.of[t];
        if ua == u32::MAX {
            continue;
        }
        for &o in &adj[t] {
            let ub = units.of[o as usize];
            if (o as usize) < t || ub == u32::MAX || ub == ua {
                continue;
            }
            let c = dot(units.normal[ua as usize], units.normal[ub as usize]);
            if c < cos_hi && c > cos_lo {
                pairs.push((ua.min(ub), ua.max(ub)));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    if pairs.is_empty() {
        return Some((found, claimed));
    }
    let mut deg = vec![0u32; nu + 1];
    for (a, b) in &pairs {
        deg[*a as usize + 1] += 1;
        deg[*b as usize + 1] += 1;
    }
    for i in 0..nu {
        deg[i + 1] += deg[i];
    }
    let mut fill = deg.clone();
    let mut nbr = vec![0u32; pairs.len() * 2];
    for (a, b) in &pairs {
        nbr[fill[*a as usize] as usize] = *b;
        fill[*a as usize] += 1;
        nbr[fill[*b as usize] as usize] = *a;
        fill[*b as usize] += 1;
    }
    let near = |u: usize| &nbr[deg[u] as usize..deg[u + 1] as usize];
    let mut stamp = vec![0u32; nu];
    let mut cur = 0u32;
    for s in 0..nu {
        if s % 65536 == 0 && !progress(0.7) {
            return None;
        }
        if claimed[s] || near(s).is_empty() {
            continue;
        }
        let cands: Vec<u32> = near(s)
            .iter()
            .copied()
            .filter(|u| !claimed[*u as usize])
            .take(3)
            .collect();
        for u in cands {
            let axis = unit(cross(units.normal[s], units.normal[u as usize]));
            if len(axis) < 0.5 {
                continue;
            }
            cur += 1;
            let mut group = vec![s as u32];
            stamp[s] = cur;
            let mut stack = vec![s];
            while let Some(x) = stack.pop() {
                for &nb in near(x) {
                    let nb = nb as usize;
                    if claimed[nb] || stamp[nb] == cur || dot(units.normal[nb], axis).abs() > 0.044 {
                        continue;
                    }
                    stamp[nb] = cur;
                    group.push(nb as u32);
                    stack.push(nb);
                }
            }
            if group.len() < 3 {
                continue;
            }
            if let Some((cyl, kept)) = fit_facets(m, normals, units, &group, axis, size, total) {
                for k in kept {
                    claimed[k as usize] = true;
                }
                found.push(cyl);
                break;
            }
        }
    }
    Some((found, claimed))
}

/// Fits a circle to the units of a group (they are square to `axis`); the units that do not lie on it (a flat wall that runs on tangentially) are left
/// out one by one. Gives the cylinder and the units it took.
fn fit_facets(
    m: &TriMesh,
    normals: &[(V3, f64)],
    units: &Units,
    group: &[u32],
    axis: V3,
    size: f64,
    total: f64,
) -> Option<(Cylinder, Vec<u32>)> {
    let a1 = if axis[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = unit(cross(axis, a1));
    let w = cross(axis, u);
    let flat = |p: V3| (dot(p, u), dot(p, w));
    // the vertices of every unit
    let mut verts: Vec<Vec<u32>> = group
        .iter()
        .map(|g| {
            let mut v: Vec<u32> = units
                .triangles(*g as usize)
                .iter()
                .flat_map(|t| m.triangles[*t as usize])
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        })
        .collect();
    let mut kept: Vec<u32> = group.to_vec();
    let mut centre = (0.0, 0.0);
    let mut radius = 0.0;
    let mut sign = 1.0;
    for _ in 0..8 {
        if kept.len() < 3 {
            return None;
        }
        let mut all: Vec<u32> = verts.iter().flatten().copied().collect();
        all.sort_unstable();
        all.dedup();
        let pts: Vec<(f64, f64)> = all.iter().map(|v| flat(m.vertex(*v))).collect();
        (centre, radius) = circle_fit(&pts)?;
        let tol = radius * 0.02 + size * 1e-4;
        // how far every unit is from the circle (its vertices), and whether its normal looks along the radius
        let mut score = Vec::with_capacity(kept.len());
        let (mut out_n, mut in_n) = (0, 0);
        for (k, g) in kept.iter().enumerate() {
            let err = verts[k]
                .iter()
                .map(|v| {
                    let p = flat(m.vertex(*v));
                    (((p.0 - centre.0).powi(2) + (p.1 - centre.1).powi(2)).sqrt() - radius).abs()
                })
                .fold(0.0, f64::max);
            let mid = verts[k]
                .iter()
                .map(|v| flat(m.vertex(*v)))
                .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            let c = verts[k].len() as f64;
            let r = (mid.0 / c - centre.0, mid.1 / c - centre.1);
            let rl = (r.0 * r.0 + r.1 * r.1).sqrt().max(1e-300);
            let nn = units.normal[*g as usize];
            let along = (dot(nn, u) * r.0 + dot(nn, w) * r.1) / rl;
            if along > 0.0 {
                out_n += 1;
            } else {
                in_n += 1;
            }
            score.push((err / tol).max(if along.abs() < 15f64.to_radians().cos() {
                10.0
            } else {
                0.0
            }));
        }
        sign = if out_n >= in_n { 1.0 } else { -1.0 };
        // units that look the wrong way round are no part of it either
        for (k, g) in kept.iter().enumerate() {
            let nn = units.normal[*g as usize];
            let c = verts[k].len() as f64;
            let mid = verts[k]
                .iter()
                .map(|v| flat(m.vertex(*v)))
                .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            let r = (mid.0 / c - centre.0, mid.1 / c - centre.1);
            if (dot(nn, u) * r.0 + dot(nn, w) * r.1) * sign < 0.0 {
                score[k] = score[k].max(10.0);
            }
        }
        let (worst, ws) = score
            .iter()
            .enumerate()
            .fold((0, 0.0), |a, (i, s)| if *s > a.1 { (i, *s) } else { a });
        if ws <= 1.0 {
            break;
        }
        kept.remove(worst);
        verts.remove(worst);
        if kept.len() < 3 {
            return None;
        }
        centre = (0.0, 0.0);
        radius = 0.0;
    }
    if kept.len() < 3 || radius < size * 1e-4 {
        return None;
    }
    // the units must lie on the circle now
    let tol = radius * 0.02 + size * 1e-4;
    let bad = verts.iter().flatten().any(|v| {
        let p = flat(m.vertex(*v));
        (((p.0 - centre.0).powi(2) + (p.1 - centre.1).powi(2)).sqrt() - radius).abs() > tol
    });
    if bad {
        return None;
    }
    let mut all: Vec<u32> = verts.iter().flatten().copied().collect();
    all.sort_unstable();
    all.dedup();
    // how far round the circle it goes: the whole circle less the biggest gap between the vertices
    let mut angles: Vec<f64> = all
        .iter()
        .map(|v| {
            let p = flat(m.vertex(*v));
            (p.1 - centre.1).atan2(p.0 - centre.0)
        })
        .collect();
    angles.sort_by(|a, b| a.total_cmp(b));
    let mut gaps: Vec<f64> = angles.windows(2).map(|p| p[1] - p[0]).collect();
    gaps.push(angles[0] + std::f64::consts::TAU - angles[angles.len() - 1]);
    let gap = gaps.iter().copied().fold(0.0, f64::max);
    gaps.sort_by(|a, b| a.total_cmp(b));
    let median = gaps[gaps.len() / 2];
    // no gap that is bigger than the usual step: it goes all the way round
    let arc = if gap <= median * 1.5 + 1e-6 {
        std::f64::consts::TAU
    } else {
        std::f64::consts::TAU - gap
    };
    if arc < 60f64.to_radians() {
        return None;
    }
    let triangles: Vec<u32> = kept
        .iter()
        .flat_map(|g| units.triangles(*g as usize).iter().copied())
        .collect();
    let area: f64 = triangles.iter().map(|t| normals[*t as usize].1).sum();
    if area < total * MIN_WALL_AREA {
        return None;
    }
    let (zlo, zhi) = all
        .iter()
        .map(|v| dot(m.vertex(*v), axis))
        .fold((f64::MAX, f64::MIN), |a, z| (a.0.min(z), a.1.max(z)));
    let point = [
        u[0] * centre.0 + w[0] * centre.1,
        u[1] * centre.0 + w[1] * centre.1,
        u[2] * centre.0 + w[2] * centre.1,
    ];
    Some((
        Cylinder {
            triangles,
            axis,
            point,
            radius,
            is_hole: sign < 0.0,
            z_range: (zlo, zhi),
            arc,
            facets: kept.len() as u32,
        },
        kept,
    ))
}
