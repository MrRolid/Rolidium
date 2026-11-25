// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The first half of the conversion of a mesh to a solid: the triangles are grouped into flat faces (neighbouring triangles whose points lie on one plane within
//! the tolerance are one face) and every face gets its outline and its holes as loops of the vertices of the mesh. The faces share the vertices of the mesh, so
//! they close without a gap; the solid is made from them by `rolidium_kernel::Solid::from_polygons`.

use crate::{cross, dot, len, mul, sub, unit, TriMesh, V3};

/// The faces of a mesh: the points and, for every face, its loops of indices (the first is the outline, the rest are holes).
#[derive(Debug, Clone, Default)]
pub struct Polyfaces {
    pub points: Vec<V3>,
    pub faces: Vec<Vec<Vec<u32>>>,
    /// For every face: the triangles it is made of (vertex indices), used instead of the face when its outline is not flat enough to make it. Empty when the group made
    /// more than one face.
    pub fallback: Vec<Vec<[u32; 3]>>,
    /// The triangles of every face (for the statistics).
    pub triangles: Vec<u32>,
    /// The largest distance of a vertex from the plane of its face (millimetres).
    pub max_deviation: f64,
    /// For every point: how far it is from the plane of the faces it belongs to (at least 1e-6): the tolerance the point needs to lie on them.
    pub point_tolerance: Vec<f64>,
    /// How many faces are single triangles (nothing flat was found round them: a round surface, a scan).
    pub single: usize,
    /// How many faces are bigger groups of triangles.
    pub flat: usize,
    /// The share of the area (0 to 1) that is in single triangles.
    pub single_area: f64,
    /// How many vertices were moved to a round value, and how many loops were dropped as no polygon.
    pub rounded: usize,
}

/// Groups the triangles into flat faces and finds their loops. `tolerance` is how far a point may be from the plane of its face; with `round` the coordinates
/// that are within a tenth of the tolerance of a multiple of 0.01 mm are moved to it. A flat group of fewer than `merge_from` triangles is not made one face: every
/// triangle is (a triangle is always flat, so the solid can always be made). `stop` is asked now and then; when it says yes the answer is `None`.
pub fn polyfaces(
    m: &TriMesh,
    tolerance: f64,
    round: bool,
    merge_from: usize,
    stop: &dyn Fn() -> bool,
) -> Option<Polyfaces> {
    let mut out = Polyfaces::default();
    // the vertices, rounded when asked
    let mut points: Vec<V3> = m
        .positions
        .iter()
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect();
    if round {
        let snap = |v: f64| {
            let r = (v * 100.0).round() / 100.0;
            if (v - r).abs() <= tolerance * 0.1 {
                (r, true)
            } else {
                (v, false)
            }
        };
        for p in &mut points {
            for c in p.iter_mut() {
                let (r, moved) = snap(*c);
                if moved && r != *c {
                    out.rounded += 1;
                }
                *c = r;
            }
        }
    }
    let unified;
    {
        // two points that became one are one point, and the triangles that collapse are gone
        let mut seen: std::collections::HashMap<[u64; 3], u32> = std::collections::HashMap::new();
        let mut remap = vec![0u32; points.len()];
        let mut unique: Vec<V3> = Vec::new();
        for (i, p) in points.iter().enumerate() {
            let key = [
                (p[0] * 1e5).round() as i64 as u64,
                (p[1] * 1e5).round() as i64 as u64,
                (p[2] * 1e5).round() as i64 as u64,
            ];
            let id = *seen.entry(key).or_insert_with(|| {
                unique.push(*p);
                (unique.len() - 1) as u32
            });
            remap[i] = id;
        }
        let tris: Vec<[u32; 3]> = m
            .triangles
            .iter()
            .map(|t| [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]])
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
        points = unique;
        unified = TriMesh {
            positions: points
                .iter()
                .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
                .collect(),
            triangles: tris,
        };
    }
    let m = &unified;
    let n = m.triangles.len();
    let corners = |t: usize| {
        let tri = m.triangles[t];
        [
            points[tri[0] as usize],
            points[tri[1] as usize],
            points[tri[2] as usize],
        ]
    };
    let normal_area = |t: usize| {
        let c = corners(t);
        let n = cross(sub(c[1], c[0]), sub(c[2], c[0]));
        let l = len(n);
        if l > 0.0 {
            (mul(n, 1.0 / l), l / 2.0)
        } else {
            ([0.0; 3], 0.0)
        }
    };
    let adj = crate::regions::neighbours(m);
    if stop() {
        return None;
    }
    let normals: Vec<(V3, f64)> = (0..n).map(normal_area).collect();
    let total: f64 = normals.iter().map(|x| x.1).sum();
    // the triangles, the biggest first: a face grows from a clear start
    let mut order: Vec<usize> = (0..n).filter(|t| normals[*t].1 > 0.0).collect();
    order.sort_by(|a, b| normals[*b].1.total_cmp(&normals[*a].1));
    let cos_t = 2f64.to_radians().cos();
    let mut face_of = vec![u32::MAX; n];
    let mut groups: Vec<Vec<u32>> = Vec::new();
    let mut stamp = vec![0u32; n];
    for (k, &s) in order.iter().enumerate() {
        if k % 16384 == 0 && stop() {
            return None;
        }
        if face_of[s] != u32::MAX {
            continue;
        }
        let (n0, _) = normals[s];
        let c0 = corners(s)[0];
        let off0 = dot(n0, c0);
        let id = groups.len() as u32;
        let mark = s as u32 + 1;
        stamp[s] = mark;
        face_of[s] = id;
        let mut group = vec![s as u32];
        let mut stack = vec![s as u32];
        while let Some(t) = stack.pop() {
            for &o in &adj[t as usize] {
                let ou = o as usize;
                if face_of[ou] != u32::MAX || stamp[ou] == mark {
                    continue;
                }
                stamp[ou] = mark;
                if normals[ou].1 == 0.0 || dot(normals[ou].0, n0) < cos_t {
                    continue;
                }
                if corners(ou).iter().any(|c| (dot(n0, *c) - off0).abs() > tolerance) {
                    continue;
                }
                face_of[ou] = id;
                group.push(o);
                stack.push(o);
            }
        }
        groups.push(group);
    }
    // the small groups are taken apart: every triangle is a face of its own
    let mut exploded: Vec<Vec<u32>> = Vec::new();
    groups.retain(|g| {
        if g.len() < merge_from && g.len() > 1 {
            exploded.extend(g.iter().map(|t| vec![*t]));
            false
        } else {
            true
        }
    });
    groups.extend(exploded);
    // triangles of no area are left out (they close nothing)
    out.point_tolerance = vec![1e-6; points.len()];
    let mut single_area = 0.0;
    for g in &groups {
        if g.len() == 1 {
            out.single += 1;
            single_area += normals[g[0] as usize].1;
        } else {
            out.flat += 1;
        }
    }
    out.single_area = if total > 0.0 { single_area / total } else { 0.0 };
    // the loops of every group
    for (gi, g) in groups.iter().enumerate() {
        if gi % 4096 == 0 && stop() {
            return None;
        }
        let nrm = {
            let mut s = [0.0; 3];
            for t in g {
                let (nn, a) = normals[*t as usize];
                for k in 0..3 {
                    s[k] += nn[k] * a;
                }
            }
            unit(s)
        };
        let loops = match loops_of(m, g, &points, nrm) {
            Some(l) => l,
            None => continue,
        };
        // the distance of the vertices from the plane of the face (through the mean of the vertices)
        let vs: Vec<u32> = loops.iter().flatten().copied().collect();
        if !vs.is_empty() {
            let mut mean = [0.0; 3];
            for v in &vs {
                for k in 0..3 {
                    mean[k] += points[*v as usize][k];
                }
            }
            let mean = mul(mean, 1.0 / vs.len() as f64);
            let mut face_dev: f64 = 0.0;
            for v in &vs {
                face_dev = face_dev.max(dot(nrm, sub(points[*v as usize], mean)).abs());
            }
            out.max_deviation = out.max_deviation.max(face_dev);
            let need = face_dev * 1.5 + 1e-6;
            for u in &vs {
                let slot = &mut out.point_tolerance[*u as usize];
                *slot = slot.max(need);
            }
        }
        let outlines = split_outlines(loops, &points, nrm);
        if outlines.len() != 1 {
            // a group with several outlines (or none that can be told) is taken apart: every triangle is a face
            for t in g {
                let tri = m.triangles[*t as usize];
                out.faces.push(vec![tri.to_vec()]);
                out.triangles.push(1);
                out.fallback.push(Vec::new());
            }
            continue;
        }
        for outline in outlines {
            out.faces.push(outline);
            out.triangles.push(g.len() as u32);
            out.fallback.push(if g.len() > 1 {
                g.iter().map(|t| m.triangles[*t as usize]).collect()
            } else {
                Vec::new()
            });
        }
    }
    // a point cannot be more tolerant than a good part of the shortest edge it ends (the kernel refuses an edge shorter than the tolerance of its ends); a face that
    // needs more than that is made of its triangles
    let mut shortest = vec![f64::MAX; points.len()];
    for tri in &m.triangles {
        for k in 0..3 {
            let (a, b) = (tri[k] as usize, tri[(k + 1) % 3] as usize);
            let d = len(sub(points[a], points[b]));
            shortest[a] = shortest[a].min(d);
            shortest[b] = shortest[b].min(d);
        }
    }
    for (t, sh) in out.point_tolerance.iter_mut().zip(&shortest) {
        *t = t.min((sh * 0.4).max(1e-7));
    }
    out.points = points;
    Some(out)
}

/// The boundary of a group of triangles as closed loops of vertices, each running with the inside on its left (the way the triangles run).
fn loops_of(m: &TriMesh, group: &[u32], points: &[V3], normal: V3) -> Option<Vec<Vec<u32>>> {
    use std::collections::HashMap;
    // directed edges; an edge that the group uses both ways is inside it
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for &t in group {
        let tri = m.triangles[t as usize];
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *edges.entry((a, b)).or_insert(0) += 1;
        }
    }
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&(a, b), &c) in &edges {
        if c == 1 && !edges.contains_key(&(b, a)) {
            next.entry(a).or_default().push(b);
        }
    }
    let basis_u = unit(cross(
        normal,
        if normal[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        },
    ));
    let basis_v = cross(normal, basis_u);
    let dir = |a: u32, b: u32| {
        let d = sub(points[b as usize], points[a as usize]);
        dot(d, basis_v).atan2(dot(d, basis_u))
    };
    let mut loops = Vec::new();
    let starts: Vec<u32> = {
        let mut v: Vec<u32> = next.keys().copied().collect();
        v.sort_unstable();
        v
    };
    for s in starts {
        while next.get(&s).is_some_and(|v| !v.is_empty()) {
            let mut lp = vec![s];
            let mut prev = s;
            let mut cur = next.get_mut(&s)?.pop()?;
            let mut guard = 0usize;
            while cur != s {
                lp.push(cur);
                let outs = next.get_mut(&cur)?;
                if outs.is_empty() {
                    return None;
                }
                let k = if outs.len() == 1 {
                    0
                } else {
                    // at a point where the boundary touches itself: the edge that turns most to the left keeps the loops apart
                    let back = dir(cur, prev);
                    let turn = |b: u32| {
                        let mut a = dir(cur, b) - back;
                        while a <= 0.0 {
                            a += std::f64::consts::TAU;
                        }
                        a
                    };
                    (0..outs.len())
                        .min_by(|x, y| turn(outs[*x]).total_cmp(&turn(outs[*y])))
                        .unwrap_or(0)
                };
                let nx = outs.swap_remove(k);
                prev = cur;
                cur = nx;
                guard += 1;
                if guard > edges.len() + 4 {
                    return None;
                }
            }
            if lp.len() >= 3 {
                loops.push(lp);
            }
        }
    }
    Some(loops)
}

/// Loops with the inside on the left run counter-clockwise round the outline and clockwise round a hole: a face for every outline, with the holes inside it.
fn split_outlines(loops: Vec<Vec<u32>>, points: &[V3], normal: V3) -> Vec<Vec<Vec<u32>>> {
    let u = unit(cross(
        normal,
        if normal[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        },
    ));
    let v = cross(normal, u);
    let flat = |l: &Vec<u32>| -> Vec<(f64, f64)> {
        l.iter()
            .map(|i| (dot(points[*i as usize], u), dot(points[*i as usize], v)))
            .collect()
    };
    let area = |p: &[(f64, f64)]| -> f64 {
        let mut a = 0.0;
        for i in 0..p.len() {
            let (x0, y0) = p[i];
            let (x1, y1) = p[(i + 1) % p.len()];
            a += x0 * y1 - x1 * y0;
        }
        a / 2.0
    };
    let inside = |p: &[(f64, f64)], q: (f64, f64)| -> bool {
        let mut c = false;
        let mut j = p.len() - 1;
        for i in 0..p.len() {
            if (p[i].1 > q.1) != (p[j].1 > q.1)
                && q.0 < (p[j].0 - p[i].0) * (q.1 - p[i].1) / (p[j].1 - p[i].1) + p[i].0
            {
                c = !c;
            }
            j = i;
        }
        c
    };
    let flats: Vec<Vec<(f64, f64)>> = loops.iter().map(&flat).collect();
    let areas: Vec<f64> = flats.iter().map(|p| area(p)).collect();
    let outer: Vec<usize> = (0..loops.len()).filter(|i| areas[*i] > 0.0).collect();
    let mut faces: Vec<Vec<Vec<u32>>> = outer.iter().map(|i| vec![loops[*i].clone()]).collect();
    for (h, a) in areas.iter().enumerate() {
        if *a > 0.0 {
            continue;
        }
        // the smallest outline that contains the first point of the hole
        let q = flats[h][0];
        let mut best: Option<(f64, usize)> = None;
        for (k, o) in outer.iter().enumerate() {
            if inside(&flats[*o], q) && best.is_none_or(|(ba, _)| areas[*o] < ba) {
                best = Some((areas[*o], k));
            }
        }
        if let Some((_, k)) = best {
            faces[k].push(loops[h].clone());
        }
    }
    faces
}

/// How well a mesh would make a solid: the share of its surface (0 to 1) that lies in flat groups of at least four triangles, and how many triangles there are.
pub fn flat_share(m: &TriMesh, tolerance: f64) -> f64 {
    let n = m.triangles.len();
    if n == 0 {
        return 0.0;
    }
    let adj = crate::regions::neighbours(m);
    let na: Vec<(V3, f64)> = (0..n).map(|t| m.normal_area(t)).collect();
    let total: f64 = na.iter().map(|x| x.1).sum();
    let cos_t = 2f64.to_radians().cos();
    let mut seen = vec![false; n];
    let mut flat = 0.0;
    let mut order: Vec<usize> = (0..n).filter(|t| na[*t].1 > 0.0).collect();
    order.sort_by(|a, b| na[*b].1.total_cmp(&na[*a].1));
    for s in order {
        if seen[s] {
            continue;
        }
        let (n0, _) = na[s];
        let off0 = dot(n0, m.corners(s)[0]);
        seen[s] = true;
        let mut stack = vec![s as u32];
        let (mut count, mut area) = (1usize, na[s].1);
        while let Some(t) = stack.pop() {
            for &o in &adj[t as usize] {
                let ou = o as usize;
                if seen[ou] || na[ou].1 == 0.0 || dot(na[ou].0, n0) < cos_t {
                    continue;
                }
                if m.corners(ou)
                    .iter()
                    .any(|c| (dot(n0, *c) - off0).abs() > tolerance)
                {
                    continue;
                }
                seen[ou] = true;
                count += 1;
                area += na[ou].1;
                stack.push(o);
            }
        }
        if count >= 4 {
            flat += area;
        }
    }
    if total > 0.0 {
        flat / total
    } else {
        0.0
    }
}
