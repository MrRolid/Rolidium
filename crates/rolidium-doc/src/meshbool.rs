// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Cutting and joining with meshes: a mesh and a solid (or another mesh) are combined with the Manifold library (`rolidium-manifold`). A solid that takes part is
//! meshed first (finely: a few thousandths of its size), so the result is a mesh. Before the operation small flat holes of a mesh are patched and the number is
//! said; a hole that is big or curved is left and the operation is refused with the reason.

use crate::{BodyId, CombineOp, Computed, DocError, Job, State};
use rolidium_kernel::{KernelError, Solid};
use rolidium_manifold::{boolean, Mesh as MMesh, Op};
use rolidium_mesh::check;
use rolidium_mesh::holefill;
use rolidium_mesh::TriMesh;
use std::sync::Arc;

fn to_mm(m: &TriMesh) -> MMesh {
    MMesh {
        positions: m.positions.clone(),
        triangles: m.triangles.clone(),
    }
}

fn from_mm(m: MMesh) -> TriMesh {
    TriMesh {
        positions: m.positions,
        triangles: m.triangles,
    }
}

/// The mesh of a solid for a boolean: welded, closed, turned outwards.
pub fn solid_to_mesh(s: &Solid) -> Result<TriMesh, KernelError> {
    let (lo, hi) = s.bounding_box()?;
    let diag = (hi - lo).length().max(1e-6);
    let linear = (diag * 2e-4).clamp(0.004, 0.05);
    let m = s.mesh(linear, 0.15)?;
    let mut t = TriMesh {
        positions: m.positions.iter().map(|p| [p.x, p.y, p.z]).collect(),
        triangles: m.indices.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
    };
    let tol = check::merge_tolerance(&t);
    check::merge_vertices(&mut t, tol);
    if t.signed_volume() < 0.0 {
        for tri in &mut t.triangles {
            tri.swap(1, 2);
        }
    }
    Ok(t)
}

/// A mesh ready for the library: loose joints and turned triangles are mended on a copy, small flat holes are patched (their number is returned), and a mesh that
/// is still not a closed surface is refused with the reason.
pub fn prepare(mesh: &TriMesh, name: &str) -> Result<(TriMesh, usize), KernelError> {
    let mut m = mesh.clone();
    let mut rep = check::check(&m);
    let mut filled = 0;
    if rep.open_edges > 0 || rep.inconsistent_edges > 0 {
        let tol = check::merge_tolerance(&m);
        check::merge_vertices(&mut m, tol);
        check::orient_normals(&mut m);
        rep = check::check(&m);
    }
    if rep.open_edges > 0 {
        let r = holefill::fill_holes(&mut m, &holefill::FillOptions::default());
        filled = r.filled;
        rep = check::check(&m);
        if rep.open_edges > 0 {
            let why = r.left.first().map(|(_, w)| format!(": {w}")).unwrap_or_default();
            return Err(KernelError::Invalid(format!(
                "The mesh {name} has {} holes that were not patched{why}. Repair it first",
                r.left.len().max(1)
            )));
        }
    }
    if rep.non_manifold_edges > 0 {
        return Err(KernelError::Invalid(format!(
            "The mesh {name} has {} edges that more than two triangles share, so it cannot be cut or joined",
            rep.non_manifold_edges
        )));
    }
    if rep.signed_volume < 0.0 {
        for tri in &mut m.triangles {
            tri.swap(1, 2);
        }
    }
    Ok((m, filled))
}

/// The operation in a thread of its own, so that Cancel (`stop`) is answered at once; a cancelled calculation is left to finish and its result is dropped.
pub fn combine_meshes(
    a: &TriMesh,
    b: &TriMesh,
    op: Op,
    stop: &dyn Fn() -> bool,
) -> Result<TriMesh, KernelError> {
    let (am, bm) = (to_mm(a), to_mm(b));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let _ = tx.send(boolean(&am, &bm, op));
        })
        .map_err(|e| KernelError::Invalid(format!("the operation could not be started: {e}")))?;
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(40)) {
            Ok(r) => return r.map(from_mm).map_err(KernelError::Invalid),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if stop() {
                    return Err(KernelError::Cancelled);
                }
            }
            Err(_) => {
                return Err(KernelError::Invalid(
                    "the operation failed inside the library".into(),
                ))
            }
        }
    }
}

fn op_of(op: CombineOp) -> Op {
    match op {
        CombineOp::Merge => Op::Union,
        CombineOp::Cut => Op::Difference,
        CombineOp::Intersect => Op::Intersection,
    }
}

/// What a mesh or a solid of the document is, as a mesh ready for the library.
pub(crate) fn body_mesh(st: &State, id: BodyId) -> Result<(TriMesh, String, usize), DocError> {
    if let Some(m) = st.meshes.iter().find(|m| m.id == id) {
        let (t, n) = prepare(&m.mesh, &m.name)?;
        return Ok((t, m.name.clone(), n));
    }
    let b = st
        .bodies
        .iter()
        .find(|b| b.id == id)
        .ok_or(DocError::NoSuchBody)?;
    Ok((solid_to_mesh(&b.solid)?, b.name.clone(), 0))
}

/// The text for the status line after a patch.
pub fn filled_text(n: usize) -> Option<String> {
    (n > 0).then(|| {
        format!(
            "{n} small hole{} in the mesh patched before the operation",
            if n == 1 { "" } else { "s" }
        )
    })
}

impl Job {
    /// Combine where the target or a tool is a mesh: the result is a mesh in the place of the target (the tools are kept or removed as asked).
    pub(crate) fn combine_with_meshes(
        &self,
        mut st: State,
        target: BodyId,
        tools: &[BodyId],
        op: CombineOp,
        keep_tools: bool,
    ) -> Result<Computed, DocError> {
        if tools.iter().all(|t| *t == target) {
            return Err(KernelError::Invalid("select at least one more body".into()).into());
        }
        let stop = || rolidium_kernel::cancel_requested();
        let (mut cur, name, mut filled) = body_mesh(&st, target)?;
        let tool_ids: Vec<BodyId> = tools.iter().copied().filter(|t| *t != target).collect();
        let before = cur.signed_volume();
        for t in &tool_ids {
            let (tm, _, n) = body_mesh(&st, *t)?;
            filled += n;
            if stop() {
                return Err(KernelError::Cancelled.into());
            }
            let r = combine_meshes(&cur, &tm, op_of(op), &stop)?;
            if r.triangles.is_empty() {
                return Err(KernelError::Invalid(match op {
                    CombineOp::Intersect => {
                        "The bodies do not overlap, so there is no common part".to_string()
                    }
                    _ => "The result is empty".to_string(),
                })
                .into());
            }
            cur = r;
        }
        if op == CombineOp::Cut && (cur.signed_volume() - before).abs() < 1e-9 * before.abs().max(1.0) {
            return Err(
                KernelError::Invalid("There is nothing to cut: the bodies do not overlap".into()).into(),
            );
        }
        // the result is a mesh in the place of the target
        let color;
        if let Some(i) = st.meshes.iter().position(|m| m.id == target) {
            color = st.meshes[i].color;
            st.meshes[i].mesh = Arc::new(cur);
            st.meshes[i].verified = true;
            st.meshes[i].visible = true;
        } else {
            let bi = st
                .bodies
                .iter()
                .position(|b| b.id == target)
                .ok_or(DocError::NoSuchBody)?;
            let b = st.bodies.remove(bi);
            color = b.color;
            st.meshes.push(crate::meshes::MeshBody {
                id: target,
                name: name.clone(),
                mesh: Arc::new(cur),
                color,
                visible: true,
                verified: true,
            });
        }
        let _ = color;
        if !keep_tools {
            for t in &tool_ids {
                st.meshes.retain(|m| m.id != *t);
                st.bodies.retain(|b| b.id != *t);
                crate::links::remove_from_groups(&mut st, *t);
            }
        }
        let mut c = self.done(st, "Combine");
        c.touched_body = Some(target);
        c.notice = filled_text(filled);
        Ok(c)
    }
}

impl Job {
    /// A volume pulled from a sketch meets meshes: cut out of them (`cut`) or joined to them. `only` limits it to one mesh (the sketch was drawn on its wall).
    /// Returns the mesh that was changed (the first one) and how many holes were patched; `None` when no mesh was touched.
    pub(crate) fn pull_on_meshes(
        &self,
        st: &mut State,
        tool: &Solid,
        cut: bool,
        only: Option<BodyId>,
    ) -> Result<Option<(BodyId, usize)>, DocError> {
        if st.meshes.is_empty() {
            return Ok(None);
        }
        let stop = || rolidium_kernel::cancel_requested();
        let tm = solid_to_mesh(tool)?;
        let (tlo, thi) = tm
            .bounding_box()
            .ok_or(KernelError::Invalid("the new volume is empty".into()))?;
        let mut first = None;
        let mut filled = 0;
        for i in 0..st.meshes.len() {
            if only.is_some_and(|o| o != st.meshes[i].id) {
                continue;
            }
            let Some((lo, hi)) = st.meshes[i].mesh.bounding_box() else {
                continue;
            };
            let apart = (0..3).any(|k| lo[k] > thi[k] + 1e-6 || hi[k] < tlo[k] - 1e-6);
            if apart {
                continue;
            }
            let (pm, _, n) = body_mesh(st, st.meshes[i].id)?;
            let before = pm.signed_volume();
            let r = combine_meshes(&pm, &tm, if cut { Op::Difference } else { Op::Union }, &stop)?;
            if r.triangles.is_empty() {
                continue;
            }
            let after = r.signed_volume();
            if cut {
                if (after - before).abs() < 1e-9 * before.abs().max(1.0) {
                    continue; // only touches: nothing removed
                }
            } else {
                // joined only when they share volume or a face: a union that is as big as the two together and falls into two pieces touched nothing
                let sum = before + tm.signed_volume();
                if (after - sum).abs() < 1e-9 * sum.abs().max(1.0) && check::check(&r).components > 1 {
                    continue;
                }
            }
            filled += n;
            let id = st.meshes[i].id;
            st.meshes[i].mesh = Arc::new(r);
            st.meshes[i].verified = true;
            first.get_or_insert(id);
            if !cut {
                break;
            }
        }
        Ok(first.map(|id| (id, filled)))
    }
}
