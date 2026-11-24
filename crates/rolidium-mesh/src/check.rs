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
    let q = |c: f32| (c as f64 / tol).round() as i64;
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(before);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    for p in &m.positions {
        let k = [q(p[0]), q(p[1]), q(p[2])];
        let id = *map.entry(k).or_insert_with(|| {
            positions.push(*p);
            (positions.len() - 1) as u32
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
