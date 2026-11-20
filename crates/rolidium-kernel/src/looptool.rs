// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Finding the closed planar loop of edges that an edge of a body belongs to (the edge of a hole, the inner circle of a pipe).
//! The Fill tool turns such a loop into a region on its plane, from the exact geometry of the edges.

use crate::{DVec3, EdgeId, EdgeKind, FaceKind, KernelError, Result, Solid};

/// One edge of a loop with the exact geometry that is needed to redraw it in a sketch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopEdge {
    pub id: EdgeId,
    pub kind: EdgeKind,
    pub start: DVec3,
    pub end: DVec3,
    /// A point in the middle of the edge (tells which way an arc goes round).
    pub mid: DVec3,
    /// Circle edges: the centre, the axis and the radius.
    pub center: Option<DVec3>,
    pub axis: Option<DVec3>,
    pub radius: f64,
}

/// A closed loop of edges in one plane, in the order of walking round it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanarLoop {
    /// A point of the plane and its normal, pointing away from the material of the face the loop bounds when there is one.
    pub origin: DVec3,
    pub normal: DVec3,
    pub edges: Vec<LoopEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopError {
    /// There is no such edge.
    NoEdge,
    /// The edges do not close up.
    NotClosed,
    /// The edges do not lie in one plane.
    NotPlanar,
    /// Several edges could continue the loop at some point.
    Ambiguous,
}

impl LoopError {
    /// The sentence for the user (English; it is translated like every message).
    pub fn message(self) -> &'static str {
        match self {
            LoopError::NoEdge => "There is no such edge",
            LoopError::NotClosed => "These edges do not form a closed loop, so there is nothing to fill",
            LoopError::NotPlanar => "These edges do not lie in one plane, so they cannot be filled with a flat region",
            LoopError::Ambiguous => "Several edges meet here, so it is not clear which loop is meant. Select the edges of the loop one by one",
        }
    }
}

fn close(a: DVec3, b: DVec3, tol: f64) -> bool {
    (a - b).length() <= tol
}

/// A plane through the given points, if they all lie in one (and are not all on a line).
fn plane_through(points: &[DVec3], tol: f64) -> Option<Option<(DVec3, DVec3)>> {
    let p0 = *points.first()?;
    // the farthest point from p0, then the one farthest from that line
    let p1 = points
        .iter()
        .copied()
        .max_by(|a, b| (*a - p0).length().total_cmp(&(*b - p0).length()))?;
    if (p1 - p0).length() <= tol {
        return Some(None);
    }
    let dir = (p1 - p0).normalize();
    let (p2, d) = points
        .iter()
        .map(|p| (*p, (*p - p0).cross(dir).length()))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if d <= tol {
        return Some(None); // all on a line: the plane is not decided yet
    }
    let n = (p1 - p0).cross(p2 - p0).normalize();
    if points.iter().all(|p| (*p - p0).dot(n).abs() <= tol) {
        Some(Some((p0, n)))
    } else {
        None // not planar
    }
}

impl Solid {
    fn loop_edge(&self, id: EdgeId) -> Result<LoopEdge> {
        let i = self.edge_info(id)?;
        let g = self.edge_geometry(id)?;
        Ok(LoopEdge {
            id,
            kind: i.kind,
            start: i.start,
            end: i.end,
            mid: g.0,
            center: (i.kind == EdgeKind::Circle).then_some(g.1),
            axis: (i.kind == EdgeKind::Circle).then_some(g.2),
            radius: i.radius,
        })
    }

    /// The closed planar loop made of the `selected` edges (all of them must belong to it), or, for one selected edge, the
    /// loop that the edge is part of. A full circle is a loop of its own.
    pub fn planar_loop(&self, selected: &[EdgeId]) -> std::result::Result<PlanarLoop, LoopError> {
        let first = *selected.first().ok_or(LoopError::NoEdge)?;
        if selected.iter().any(|e| e.0 as usize >= self.edge_count()) {
            return Err(LoopError::NoEdge);
        }
        let get = |e: EdgeId| self.loop_edge(e).map_err(|_| LoopError::NoEdge);
        let diag = {
            let p = self.props().map_err(|_| LoopError::NoEdge)?;
            (p.max - p.min).length().max(1.0)
        };
        let tol = 1e-6 * diag;
        let pts_of = |e: &LoopEdge| {
            let mut v = vec![e.start, e.end, e.mid];
            if let Some(c) = e.center {
                v.push(c);
            }
            v
        };
        let mut chain: Vec<LoopEdge> = Vec::new();
        if selected.len() == 1 {
            let e = get(first)?;
            if e.kind == EdgeKind::Circle && close(e.start, e.end, tol) {
                chain.push(e);
            } else {
                // walk from this edge: at every end take the one other edge that continues there
                let all: Vec<LoopEdge> = (0..self.edge_count() as u32)
                    .filter_map(|i| self.loop_edge(EdgeId(i)).ok())
                    .collect();
                let mut at = e.end;
                let mut pts = pts_of(&e);
                chain.push(e);
                for _ in 0..all.len() {
                    if close(at, e.start, tol) && chain.len() > 1 {
                        break;
                    }
                    let mut next: Vec<(LoopEdge, DVec3)> = all
                        .iter()
                        .filter(|c| !chain.iter().any(|x| x.id == c.id))
                        .filter_map(|c| {
                            if close(c.start, at, tol) {
                                Some((*c, c.end))
                            } else if close(c.end, at, tol) {
                                Some((*c, c.start))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if next.len() > 1 {
                        // only the ones that lie in the plane so far
                        next.retain(|(c, _)| {
                            let mut p = pts.clone();
                            p.extend(pts_of(c));
                            plane_through(&p, tol).is_some()
                        });
                    }
                    match next.len() {
                        0 => return Err(LoopError::NotClosed),
                        1 => {}
                        _ => return Err(LoopError::Ambiguous),
                    }
                    let (c, far) = next.remove(0);
                    pts.extend(pts_of(&c));
                    if plane_through(&pts, tol).is_none() {
                        return Err(LoopError::NotPlanar);
                    }
                    at = far;
                    chain.push(c);
                }
                if !close(at, e.start, tol) {
                    return Err(LoopError::NotClosed);
                }
            }
        } else {
            // the selected edges together: every end must meet exactly one other end
            let all: Vec<LoopEdge> = selected
                .iter()
                .map(|e| get(*e))
                .collect::<std::result::Result<_, _>>()?;
            let mut rest = all.clone();
            let e = rest.remove(0);
            let mut at = e.end;
            chain.push(e);
            if e.kind == EdgeKind::Circle && close(e.start, e.end, tol) {
                return Err(LoopError::NotClosed); // a full circle next to other edges is not one loop
            }
            while !rest.is_empty() {
                let k = rest
                    .iter()
                    .position(|c| close(c.start, at, tol) || close(c.end, at, tol))
                    .ok_or(LoopError::NotClosed)?;
                let c = rest.remove(k);
                at = if close(c.start, at, tol) { c.end } else { c.start };
                chain.push(c);
            }
            if !close(at, e.start, tol) {
                return Err(LoopError::NotClosed);
            }
        }
        // the plane
        let mut pts: Vec<DVec3> = chain.iter().flat_map(pts_of).collect();
        let (mut origin, mut normal) = match plane_through(&pts, tol) {
            None => return Err(LoopError::NotPlanar),
            Some(Some(pl)) => pl,
            Some(None) => {
                // a circle alone (or an arc): the axis tells
                let c = chain
                    .iter()
                    .find_map(|e| Some((e.center?, e.axis?)))
                    .ok_or(LoopError::NotPlanar)?;
                (c.0, c.1.normalize())
            }
        };
        if let Some((c, a)) = chain.iter().find_map(|e| Some((e.center?, e.axis?))) {
            if pts.len() >= 3 && chain.len() == 1 {
                origin = c;
                normal = a.normalize();
            }
        }
        pts.clear();
        // the side: away from the material of a flat face that the loop bounds (the ring around a hole)
        let mut outward = None;
        for f in self.faces() {
            let Ok(info) = self.face_info(f) else { continue };
            if info.kind != FaceKind::Plane || info.normal.normalize().dot(normal).abs() < 1.0 - 1e-6 {
                continue;
            }
            if (info.origin - origin).dot(normal).abs() > tol {
                continue;
            }
            let Ok(fe) = self.face_edges(f) else { continue };
            if chain.iter().any(|c| fe.contains(&c.id)) {
                outward = Some(info.normal.normalize());
                break;
            }
        }
        if let Some(n) = outward {
            normal = n;
        } else if normal.z < -1e-9
            || (normal.z.abs() <= 1e-9 && (normal.y < -1e-9 || (normal.y.abs() <= 1e-9 && normal.x < 0.0)))
        {
            normal = -normal;
        }
        Ok(PlanarLoop {
            origin,
            normal,
            edges: chain,
        })
    }

    /// Middle point of an edge, and for a circle its centre and axis (zeros otherwise).
    pub(crate) fn edge_geometry(&self, edge: EdgeId) -> Result<(DVec3, DVec3, DVec3)> {
        let v = crate::nat::edge_geom(&self.shape, edge.0)?;
        if v.len() < 9 {
            return Err(KernelError::Invalid("edge geometry is not available".into()));
        }
        Ok((
            DVec3::new(v[0], v[1], v[2]),
            DVec3::new(v[3], v[4], v[5]),
            DVec3::new(v[6], v[7], v[8]),
        ))
    }
}
