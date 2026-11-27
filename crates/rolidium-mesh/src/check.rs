// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The check of a mesh (holes, normals that do not agree, edges of more than two triangles, degenerate triangles, loose pieces) and the simple repairs.

use crate::{TriMesh, V3};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub vertices: usize,
    pub triangles: usize,
    /// Edges that belong to one triangle only: the holes of the mesh.
    pub open_edges: usize,
    /// Edges of more than two triangles.
    pub non_manifold_edges: usize,
    /// Edges whose two triangles run the same way (the neighbours disagree about which side is outside).
    pub inconsistent_edges: usize,
    /// Triangles with no area, or with a repeated vertex.
    pub degenerate: usize,
    /// Vertices no triangle uses.
    pub unused_vertices: usize,
    /// Pieces of the mesh that are not connected.
    pub components: usize,
    /// A closed mesh whose triangles run counter-clockwise seen from outside has a positive volume; a negative one is turned inside out.
    pub signed_volume: f64,
}

impl Report {
    pub fn is_closed(&self) -> bool {
        self.open_edges == 0 && self.non_manifold_edges == 0
    }

    /// No holes in the surface and no triangle that disagrees with its neighbours about the outside: the edits of walls and holes may be made (edges shared
    /// by more than two triangles, where two parts touch, do not matter for them).
    pub fn is_watertight(&self) -> bool {
        self.open_edges == 0 && self.inconsistent_edges == 0
    }

    pub fn is_clean(&self) -> bool {
        self.is_closed() && self.inconsistent_edges == 0 && self.degenerate == 0 && self.signed_volume > 0.0
    }

    /// The findings as lines of text for a panel or a script.
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![format!(
            "{} triangles, {} vertices, {} piece(s)",
            self.triangles, self.vertices, self.components
        )];
        let mut flag = |n: usize, what: &str| {
            if n > 0 {
                v.push(format!("{n} {what}"));
            }
        };
        flag(self.open_edges, "open edge(s): the mesh has holes");
        flag(self.non_manifold_edges, "edge(s) of more than two triangles");
        flag(self.inconsistent_edges, "edge(s) where the normals do not agree");
        flag(self.degenerate, "degenerate triangle(s)");
        flag(self.unused_vertices, "unused vertex(es)");
        if self.is_closed() && self.signed_volume < 0.0 {
            v.push("the mesh is turned inside out".into());
        }
        if self.is_clean() {
            v.push("the mesh is closed and consistent".into());
        }
        v
    }
}

/// Every edge of every triangle as (edge key, triangle * 2 + 1 if the triangle runs the edge from the lower vertex to the higher), sorted by the key: the
/// triangles of one edge follow each other. (A sorted list is several times faster than a hash map for a mesh of millions of triangles.)
pub(crate) fn sorted_edges(m: &TriMesh) -> Vec<(u64, u32)> {
    let mut v: Vec<(u64, u32)> = Vec::with_capacity(m.triangles.len() * 3);
    for (t, tri) in m.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            v.push((
                ((lo as u64) << 32) | hi as u64,
                (t as u32) << 1 | u32::from(a < b),
            ));
        }
    }
    v.sort_unstable();
    v
}

/// The runs of equal keys in a sorted list of edges.
pub(crate) fn runs(e: &[(u64, u32)]) -> impl Iterator<Item = &[(u64, u32)]> {
    e.chunk_by(|a, b| a.0 == b.0)
}

pub fn check(m: &TriMesh) -> Report {
    let mut r = Report {
        vertices: m.positions.len(),
        triangles: m.triangles.len(),
        signed_volume: m.signed_volume(),
        ..Default::default()
    };
    let mut used = vec![false; m.positions.len()];
    for (t, tri) in m.triangles.iter().enumerate() {
        for &v in tri {
            used[v as usize] = true;
        }
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] || m.normal_area(t).1 < 1e-12 {
            r.degenerate += 1;
        }
    }
    r.unused_vertices = used.iter().filter(|u| !**u).count();
    let edges = sorted_edges(m);
    for run in runs(&edges) {
        match run.len() {
            1 => r.open_edges += 1,
            2 => {
                if (run[0].1 & 1) == (run[1].1 & 1) {
                    r.inconsistent_edges += 1;
                }
            }
            _ => r.non_manifold_edges += 1,
        }
    }
    r.components = components(m, &edges).1;
    r
}

/// The piece of every triangle and the number of pieces.
fn components(m: &TriMesh, edges: &[(u64, u32)]) -> (Vec<u32>, usize) {
    let n = m.triangles.len();
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for run in runs(edges) {
        for w in run.windows(2) {
            let (a, b) = (find(&mut parent, w[0].1 >> 1), find(&mut parent, w[1].1 >> 1));
            if a != b {
                parent[a as usize] = b;
            }
        }
    }
    let mut label = vec![u32::MAX; n];
    let mut count = 0usize;
    let mut out = vec![0u32; n];
    for t in 0..n {
        let r = find(&mut parent, t as u32) as usize;
        if label[r] == u32::MAX {
            label[r] = count as u32;
            count += 1;
        }
        out[t] = label[r];
    }
    (out, count)
}

/// Joins vertices that lie within `tol` of each other (a mesh whose triangles were written with a little noise), drops the triangles that collapse and the
/// vertices nobody uses. Returns how many vertices were merged.
pub fn merge_vertices(m: &mut TriMesh, tol: f64) -> usize {
    let before = m.positions.len();
    // cells of the size `tol`: a vertex within `tol` of another is in the same cell or in one of the 26 next to it
    let cell = |c: f32| (c as f64 / tol).floor() as i64;
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(before);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let tol2 = tol * tol;
    for p in &m.positions {
        let k = [cell(p[0]), cell(p[1]), cell(p[2])];
        let mut found = None;
        'look: for dx in -1..=1i64 {
            for dy in -1..=1i64 {
                for dz in -1..=1i64 {
                    if let Some(&i) = map.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        let q = positions[i as usize];
                        let d2 = (0..3).map(|a| (q[a] as f64 - p[a] as f64).powi(2)).sum::<f64>();
                        if d2 <= tol2 {
                            found = Some(i);
                            break 'look;
                        }
                    }
                }
            }
        }
        let id = found.unwrap_or_else(|| {
            positions.push(*p);
            let id = (positions.len() - 1) as u32;
            map.insert(k, id);
            id
        });
        remap.push(id);
    }
    let triangles: Vec<[u32; 3]> = m
        .triangles
        .iter()
        .map(|t| [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]])
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .collect();
    m.positions = positions;
    m.triangles = triangles;
    compact(m);
    before - m.positions.len()
}

/// Drops the vertices that no triangle uses.
pub fn compact(m: &mut TriMesh) {
    let mut used = vec![false; m.positions.len()];
    for t in &m.triangles {
        for &v in t {
            used[v as usize] = true;
        }
    }
    let mut remap = vec![u32::MAX; m.positions.len()];
    let mut positions = Vec::new();
    for (i, u) in used.iter().enumerate() {
        if *u {
            remap[i] = positions.len() as u32;
            positions.push(m.positions[i]);
        }
    }
    for t in &mut m.triangles {
        for v in t.iter_mut() {
            *v = remap[*v as usize];
        }
    }
    m.positions = positions;
}

/// Turns the triangles so that neighbours agree about the outside (each piece is walked from one triangle), then turns a piece that is inside out
/// (negative volume). Returns the number of triangles that were turned.
pub fn orient_normals(m: &mut TriMesh) -> usize {
    let edges = sorted_edges(m);
    let (comp, count) = components(m, &edges);
    let n = m.triangles.len();
    let mut flipped = vec![false; n];
    let mut seen = vec![false; n];
    // triangle neighbours through edges used by exactly two triangles
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for run in runs(&edges) {
        if run.len() == 2 {
            let (a, b) = (run[0].1 >> 1, run[1].1 >> 1);
            adj[a as usize].push(b);
            adj[b as usize].push(a);
        }
    }
    let directed = |tri: [u32; 3], a: u32, b: u32| -> Option<bool> {
        for k in 0..3 {
            if tri[k] == a && tri[(k + 1) % 3] == b {
                return Some(true);
            }
            if tri[k] == b && tri[(k + 1) % 3] == a {
                return Some(false);
            }
        }
        None
    };
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start as u32];
        while let Some(t) = stack.pop() {
            let tt = m.triangles[t as usize];
            let cur = if flipped[t as usize] {
                [tt[0], tt[2], tt[1]]
            } else {
                tt
            };
            for &o in &adj[t as usize] {
                if seen[o as usize] {
                    continue;
                }
                seen[o as usize] = true;
                let ot = m.triangles[o as usize];
                // the shared edge: the two vertices both triangles have
                let shared: Vec<u32> = cur.iter().copied().filter(|v| ot.contains(v)).collect();
                if shared.len() == 2 {
                    let a = directed(cur, shared[0], shared[1]);
                    let b = directed(ot, shared[0], shared[1]);
                    if let (Some(a), Some(b)) = (a, b) {
                        // agreeing neighbours run the shared edge in opposite directions
                        flipped[o as usize] = a == b;
                    }
                }
                stack.push(o);
            }
        }
    }
    // a piece with a negative volume is inside out: turn all of it
    let mut volume = vec![0.0f64; count];
    for t in 0..n {
        let tt = m.triangles[t];
        let tri = if flipped[t] { [tt[0], tt[2], tt[1]] } else { tt };
        let (a, b, c) = (m.vertex(tri[0]), m.vertex(tri[1]), m.vertex(tri[2]));
        volume[comp[t] as usize] += crate::dot(a, crate::cross(b, c)) / 6.0;
    }
    let mut turned = 0;
    for t in 0..n {
        if flipped[t] != (volume[comp[t] as usize] < 0.0) {
            m.triangles[t].swap(1, 2);
            turned += 1;
        }
    }
    turned
}

/// What the check found and what the two repairs would do about it (found by trying them on a copy), so that a program can offer exactly the repairs
/// that help and say in plain words what they do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Analysis {
    pub report: Report,
    /// Vertices that lie on each other without being one (loose joints): what "merge" would join.
    pub loose_joints: usize,
    /// Triangles that face inwards (after the joints are joined): what "orient" would turn.
    pub inward: usize,
    /// The check after the joints are joined (the open edges that are left are real holes).
    pub after_merge: Report,
    /// Edges where two parts of the mesh touch (four or more triangles share them) that `separate_pinches` mends (after the joints are joined).
    pub pinches: usize,
}

impl Analysis {
    /// Nothing is wrong that a slicer or a printer would mind.
    pub fn is_clean(&self) -> bool {
        self.report.is_clean()
    }
}

/// The tolerance within which two vertices count as one (the same as the repair uses).
pub fn merge_tolerance(m: &TriMesh) -> f64 {
    let size = m
        .bounding_box()
        .map(|(lo, hi)| crate::len(crate::sub(hi, lo)))
        .unwrap_or(0.0);
    (size * 1e-6).max(1e-6)
}

/// The check and a trial of the repairs.
pub fn analyse(m: &TriMesh) -> Analysis {
    let report = check(m);
    if report.is_clean() {
        return Analysis {
            after_merge: report.clone(),
            report,
            ..Default::default()
        };
    }
    let mut c = m.clone();
    let loose_joints = merge_vertices(&mut c, merge_tolerance(m));
    let after_merge = if loose_joints > 0 {
        check(&c)
    } else {
        report.clone()
    };
    let inward = orient_normals(&mut c);
    let pinches = if after_merge.non_manifold_edges > 0 {
        let mut d = c.clone();
        let n = separate_pinches(&mut d);
        if n > 0 && check(&d).non_manifold_edges == 0 {
            n
        } else {
            0
        }
    } else {
        0
    };
    Analysis {
        report,
        loose_joints,
        inward,
        after_merge,
        pinches,
    }
}

/// The centre of the box and its size: a one-line summary for the import dialog.
pub fn size_text(m: &TriMesh) -> String {
    match m.bounding_box() {
        Some((lo, hi)) => {
            let d: V3 = crate::sub(hi, lo);
            format!("{:.3} x {:.3} x {:.3} mm", d[0], d[1], d[2])
        }
        None => "empty".into(),
    }
}

/// Separates the places where the surface touches itself along an edge (an edge that four or more triangles share: two parts of the model meet there): the triangles
/// round such an edge are paired by the way they lie round it (a pair is the triangles that enclose a wedge of material), and the vertices are duplicated so that
/// every pair has the edge to itself. The duplicates are moved apart by a hair (a few hundredths of a micron into their own material), so that a later weld of equal
/// points does not join them again. Returns how many edges were separated; the mesh is left as it was when some edge could not be paired (a closed, consistently turned
/// mesh is needed) and the answer is 0.
pub fn separate_pinches(m: &mut TriMesh) -> usize {
    let mut by_edge: HashMap<(u32, u32), Vec<(u32, bool)>> = HashMap::new();
    for (t, tri) in m.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            by_edge
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((t as u32, a < b));
        }
    }
    let bad: Vec<(u32, u32)> = by_edge
        .iter()
        .filter(|(_, v)| v.len() > 2)
        .map(|(k, _)| *k)
        .collect();
    if bad.is_empty() {
        return 0;
    }
    // links[t] = the triangles that t is joined to across an edge (through the manifold edges, and through the pairs of the pinched ones)
    let mut link: Vec<(u32, u32, u32, u32)> = Vec::new(); // (triangle, triangle, edge a, edge b)
    for (&(a, b), v) in &by_edge {
        if v.len() == 2 {
            link.push((v[0].0, v[1].0, a, b));
        }
    }
    let mut separated = 0usize;
    for &(a, b) in &bad {
        let v = &by_edge[&(a, b)];
        if v.len() % 2 != 0 {
            return 0;
        }
        let (pa, pb) = (m.vertex(a), m.vertex(b));
        let axis = crate::unit(crate::sub(pb, pa));
        let (u, w) = {
            let helper = if axis[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let u = crate::unit(crate::cross(axis, helper));
            (u, crate::cross(axis, u))
        };
        // every triangle: its angle round the edge, and the way its material lies (+1 towards bigger angles)
        let mut items: Vec<(f64, f64, u32)> = Vec::new();
        for &(t, forward) in v {
            let tri = m.triangles[t as usize];
            let c = tri.iter().copied().find(|x| *x != a && *x != b).unwrap_or(a);
            let d = crate::sub(m.vertex(c), pa);
            let perp = crate::sub(d, crate::mul(axis, crate::dot(d, axis)));
            let ang = crate::dot(perp, w).atan2(crate::dot(perp, u));
            let (nn, _) = m.normal_area(t as usize);
            // the direction of growing angle at the triangle, and the outward normal: the material lies behind the normal
            let grow = crate::cross(axis, perp);
            let sigma = if crate::dot(grow, nn) < 0.0 { 1.0 } else { -1.0 };
            let _ = forward;
            items.push((ang, sigma, t));
        }
        items.sort_by(|x, y| x.0.total_cmp(&y.0));
        let k = items.len();
        // pairs: a triangle whose material lies towards bigger angles with the next one (cyclically) whose material lies towards smaller ones
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut used = vec![false; k];
        for i in 0..k {
            let j = (i + 1) % k;
            if !used[i] && !used[j] && items[i].1 > 0.0 && items[j].1 < 0.0 {
                pairs.push((items[i].2, items[j].2));
                used[i] = true;
                used[j] = true;
            }
        }
        if used.iter().any(|u| !*u) {
            return 0;
        }
        for (x, y) in pairs {
            link.push((x, y, a, b));
        }
        separated += 1;
    }
    // for every vertex the triangles round it fall into fans (joined through the links of the edges that end at the vertex); the first fan keeps the vertex, the other
    // fans get a copy each
    let n = m.triangles.len();
    let mut at: HashMap<u32, Vec<u32>> = HashMap::new();
    for (t, tri) in m.triangles.iter().enumerate() {
        for &x in tri {
            at.entry(x).or_default().push(t as u32);
        }
    }
    let mut parent: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    fn find(p: &mut HashMap<(u32, u32), (u32, u32)>, x: (u32, u32)) -> (u32, u32) {
        let mut r = x;
        while let Some(&q) = p.get(&r) {
            if q == r {
                break;
            }
            r = q;
        }
        let mut c = x;
        while let Some(&q) = p.get(&c) {
            if q == r {
                break;
            }
            p.insert(c, r);
            c = q;
        }
        r
    }
    for (v, ts) in &at {
        for &t in ts {
            parent.insert((*v, t), (*v, t));
        }
    }
    for &(x, y, a, b) in &link {
        for v in [a, b] {
            let (rx, ry) = (find(&mut parent, (v, x)), find(&mut parent, (v, y)));
            if rx != ry {
                parent.insert(rx, ry);
            }
        }
    }
    let _ = n;
    let mut copies: HashMap<(u32, u32), u32> = HashMap::new(); // (vertex, fan root) -> the vertex used by that fan
    let mut first_root: HashMap<u32, (u32, u32)> = HashMap::new();
    let mut moves: Vec<(u32, V3)> = Vec::new(); // (new vertex, direction of the little move)
    let mut tris = m.triangles.clone();
    for (t, tri) in m.triangles.iter().enumerate() {
        for (c, &v) in tri.iter().enumerate() {
            let root = find(&mut parent, (v, t as u32));
            let keep = *first_root.entry(v).or_insert(root);
            if root == keep {
                continue;
            }
            let id = *copies.entry(root).or_insert_with(|| {
                m.positions.push(m.positions[v as usize]);
                (m.positions.len() - 1) as u32
            });
            tris[t][c] = id;
            moves.push((id, m.normal_area(t).0));
        }
    }
    m.triangles = tris;
    // the copy moves a hair against the outward normals of its triangles (into its own material)
    let mut shift: HashMap<u32, V3> = HashMap::new();
    for (id, nn) in moves {
        let e = shift.entry(id).or_insert([0.0; 3]);
        *e = crate::add(*e, nn);
    }
    for (id, d) in shift {
        let p = m.vertex(id);
        m.set_vertex(id, crate::sub(p, crate::mul(crate::unit(d), 3e-5)));
    }
    separated
}

/// The connected pieces of a mesh as meshes of their own (through shared edges), the biggest first.
pub fn split_components(m: &TriMesh) -> Vec<TriMesh> {
    let edges = sorted_edges(m);
    let (comp, count) = components(m, &edges);
    if count <= 1 {
        return vec![m.clone()];
    }
    let mut parts: Vec<TriMesh> = vec![TriMesh::default(); count];
    let mut remap: Vec<HashMap<u32, u32>> = vec![HashMap::new(); count];
    for (t, tri) in m.triangles.iter().enumerate() {
        let c = comp[t] as usize;
        let mut out = [0u32; 3];
        for k in 0..3 {
            let v = tri[k];
            out[k] = *remap[c].entry(v).or_insert_with(|| {
                parts[c].positions.push(m.positions[v as usize]);
                (parts[c].positions.len() - 1) as u32
            });
        }
        parts[c].triangles.push(out);
    }
    parts.sort_by_key(|p| std::cmp::Reverse(p.triangles.len()));
    parts
}
