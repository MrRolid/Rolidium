// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! What the pointer needs to find the edge under it without looking at everything: a box round every edge, and a bounding volume hierarchy of the
//! triangles of a body (for the question "does a surface hide this point of an edge"). Both belong to a mesh and are made once per mesh (the BVH on
//! another thread, so that a big mesh never stops the window); while a BVH is not ready nothing is taken for hidden.

use super::*;
use rolidium_kernel::Mesh;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// A hierarchy of boxes round the triangles of one mesh.
pub struct Bvh {
    nodes: Vec<Node>,
    /// Triangle numbers in the order of the leaves.
    order: Vec<u32>,
    mesh: Arc<Mesh>,
}

struct Node {
    lo: Vec3,
    hi: Vec3,
    /// A leaf: the first triangle in `order` and how many (`count > 0`); an inner node: the index of the left child (the right one follows
    /// the whole left subtree: it is `right`), `count == 0`.
    first: u32,
    count: u32,
    right: u32,
}

const LEAF: usize = 4;

impl Bvh {
    pub fn build(mesh: Arc<Mesh>) -> Bvh {
        let n = mesh.indices.len() / 3;
        let tri = |i: usize| -> [Vec3; 3] {
            let t = &mesh.indices[i * 3..i * 3 + 3];
            [
                mesh.positions[t[0] as usize],
                mesh.positions[t[1] as usize],
                mesh.positions[t[2] as usize],
            ]
        };
        let centre: Vec<Vec3> = (0..n)
            .map(|i| {
                let [a, b, c] = tri(i);
                (a + b + c) / 3.0
            })
            .collect();
        let bounds = |ids: &[u32]| -> (Vec3, Vec3) {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for &i in ids {
                for p in tri(i as usize) {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            (lo, hi)
        };
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes: Vec<Node> = Vec::new();
        // (range in `order`, index of the node to fill)
        fn go(
            order: &mut [u32],
            base: usize,
            nodes: &mut Vec<Node>,
            centre: &[Vec3],
            bounds: &dyn Fn(&[u32]) -> (Vec3, Vec3),
        ) -> usize {
            let (lo, hi) = bounds(order);
            let at = nodes.len();
            nodes.push(Node {
                lo,
                hi,
                first: base as u32,
                count: order.len() as u32,
                right: 0,
            });
            if order.len() <= LEAF {
                return at;
            }
            let ext = hi - lo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            let mid = order.len() / 2;
            order.select_nth_unstable_by(mid, |a, b| {
                centre[*a as usize][axis].total_cmp(&centre[*b as usize][axis])
            });
            let (l, r) = order.split_at_mut(mid);
            go(l, base, nodes, centre, bounds);
            let right = go(r, base + mid, nodes, centre, bounds);
            nodes[at].count = 0;
            nodes[at].right = right as u32;
            at
        }
        if n > 0 {
            go(&mut order, 0, &mut nodes, &centre, &bounds);
        }
        Bvh { nodes, order, mesh }
    }

    /// The nearest triangle hit by the ray `o + t d` (d a unit vector): the distance and the number of the triangle.
    pub fn nearest(&self, o: Vec3, d: Vec3) -> Option<(f32, usize)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best: Option<(f32, usize)> = None;
        let mut stack = [0usize; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let idx = stack[sp];
            let node = &self.nodes[idx];
            let t1 = (node.lo - o) * inv;
            let t2 = (node.hi - o) * inv;
            let (tmin, tmax) = (t1.min(t2), t1.max(t2));
            let near = tmin.x.max(tmin.y).max(tmin.z).max(0.0);
            let far = tmax.x.min(tmax.y).min(tmax.z).min(best.map_or(f32::MAX, |b| b.0));
            if near > far {
                continue;
            }
            if node.count > 0 {
                for k in 0..node.count {
                    let tri = self.order[(node.first + k) as usize] as usize;
                    let t = &self.mesh.indices[tri * 3..tri * 3 + 3];
                    let p = |i: u32| self.mesh.positions[i as usize];
                    if let Some(h) = super::pick::ray_triangle(o, d, p(t[0]), p(t[1]), p(t[2])) {
                        if best.is_none_or(|b| h < b.0) {
                            best = Some((h, tri));
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = idx + 1;
                stack[sp + 1] = node.right as usize;
                sp += 2;
            }
        }
        best
    }

    /// True if a triangle is hit by the ray `o + t d` (d a unit vector) at a distance below `max`.
    pub fn hit_before(&self, o: Vec3, d: Vec3, max: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut stack = [0usize; 64];
        let mut sp = 1;
        stack[0] = 0;
        while sp > 0 {
            sp -= 1;
            let idx = stack[sp];
            let node = &self.nodes[idx];
            // slab test
            let t1 = (node.lo - o) * inv;
            let t2 = (node.hi - o) * inv;
            let (tmin, tmax) = (t1.min(t2), t1.max(t2));
            let near = tmin.x.max(tmin.y).max(tmin.z).max(0.0);
            let far = tmax.x.min(tmax.y).min(tmax.z).min(max);
            if near > far {
                continue;
            }
            if node.count > 0 {
                for k in 0..node.count {
                    let tri = self.order[(node.first + k) as usize] as usize;
                    let t = &self.mesh.indices[tri * 3..tri * 3 + 3];
                    let (a, b, c) = (
                        self.mesh.positions[t[0] as usize],
                        self.mesh.positions[t[1] as usize],
                        self.mesh.positions[t[2] as usize],
                    );
                    if super::pick::ray_triangle(o, d, a, b, c).is_some_and(|h| h < max) {
                        return true;
                    }
                }
            } else if sp + 2 <= stack.len() {
                // the left child is the next node, the right one is stored
                stack[sp] = idx + 1;
                stack[sp + 1] = node.right as usize;
                sp += 2;
            }
        }
        false
    }
}

/// The caches of the picking, kept for the meshes that are shown.
#[derive(Default)]
pub struct PickCache {
    bvh: HashMap<u64, Arc<OnceLock<Bvh>>>,
    boxes: HashMap<u64, Arc<Vec<(Vec3, Vec3)>>>,
}

impl PickCache {
    /// The BVH of the mesh if it is ready; otherwise its making is started (on another thread) and `None` is returned.
    pub fn bvh(&mut self, stamp: u64, mesh: &Arc<Mesh>) -> Option<Arc<OnceLock<Bvh>>> {
        if self.bvh.len() > 64 {
            self.bvh.clear();
        }
        let slot = self
            .bvh
            .entry(stamp)
            .or_insert_with(|| {
                let slot: Arc<OnceLock<Bvh>> = Arc::new(OnceLock::new());
                let (s2, m2) = (slot.clone(), mesh.clone());
                let _ = std::thread::Builder::new().name("bvh".into()).spawn(move || {
                    let t = std::time::Instant::now();
                    let n = m2.triangle_count();
                    let _ = s2.set(Bvh::build(m2));
                    crate::logging::log(&format!(
                        "hierarchy of boxes for {n} triangles made in {:.2} s",
                        t.elapsed().as_secs_f32()
                    ));
                });
                slot
            })
            .clone();
        slot.get().is_some().then_some(slot)
    }

    /// The box round every edge of the mesh.
    pub fn edge_boxes(&mut self, stamp: u64, mesh: &Mesh) -> Arc<Vec<(Vec3, Vec3)>> {
        if self.boxes.len() > 64 {
            self.boxes.clear();
        }
        self.boxes
            .entry(stamp)
            .or_insert_with(|| {
                Arc::new(
                    mesh.edges
                        .iter()
                        .map(|e| {
                            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                            for p in &e.points {
                                lo = lo.min(*p);
                                hi = hi.max(*p);
                            }
                            (lo, hi)
                        })
                        .collect(),
                )
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_mesh(z: f32) -> Mesh {
        Mesh {
            positions: vec![
                Vec3::new(-5.0, -5.0, z),
                Vec3::new(5.0, -5.0, z),
                Vec3::new(5.0, 5.0, z),
                Vec3::new(-5.0, 5.0, z),
            ],
            normals: vec![Vec3::Z; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            tri_face: vec![0, 0],
            ..Default::default()
        }
    }

    #[test]
    fn the_bvh_agrees_with_testing_every_triangle() {
        // a stack of plates; rays from above, some through the plates, some beside them
        let mut m = Mesh::default();
        for k in 0..60 {
            let q = quad_mesh(k as f32);
            let base = m.positions.len() as u32;
            m.positions.extend(q.positions.iter().copied());
            m.normals.extend(q.normals.iter().copied());
            m.indices.extend(q.indices.iter().map(|i| i + base));
            m.tri_face.extend([k, k]);
        }
        let bvh = Bvh::build(Arc::new(m.clone()));
        for (x, y, z0, max, want) in [
            (0.0, 0.0, 100.0, 30.0, false),
            (0.0, 0.0, 100.0, 45.0, true),
            (9.0, 0.0, 100.0, 200.0, false),
            (1.0, 2.0, 30.5, 1.0, true),
            (1.0, 2.0, 30.5, 0.4, false),
        ] {
            let o = Vec3::new(x, y, z0);
            let d = -Vec3::Z;
            let brute = m.indices.as_chunks::<3>().0.iter().any(|t| {
                super::super::pick::ray_triangle(
                    o,
                    d,
                    m.positions[t[0] as usize],
                    m.positions[t[1] as usize],
                    m.positions[t[2] as usize],
                )
                .is_some_and(|h| h < max)
            });
            assert_eq!(brute, want, "brute force at {x} {y} {z0} {max}");
            assert_eq!(bvh.hit_before(o, d, max), want, "bvh at {x} {y} {z0} {max}");
        }
    }
}
