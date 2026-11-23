// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The flat end faces of a loft (and the start face of a sweep) lie where the sketch of the profile lies. A change of such a face is a change of that sketch:
//! another size of its circle, a move of the sketch. The link then calculates the body again, and the link stays.

use crate::links::LinkKind;
use crate::{BodyId, LinkId, LoftRef, SketchId, State};
use rolidium_kernel::{DVec3, FaceId, FaceKind};
use rolidium_sketch::{DVec2, Entity, EntityId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndKind {
    /// An end of a loft: it can be pulled and moved (the sketch moves) and its profile can be changed.
    Loft,
    /// The start of a sweep: its profile can be changed; the face cannot be moved (the path starts there).
    SweepStart,
}

/// The sketch an end face comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndFace {
    pub link: LinkId,
    pub sketch: SketchId,
    pub region: usize,
    pub kind: EndKind,
}

/// The sketch that makes the flat face `face` of the body `body` of a loft or a sweep, when the face lies on the plane of the sketch of a profile.
pub fn end_face_of(st: &State, body: BodyId, face: FaceId) -> Option<EndFace> {
    let link = crate::links::link_of_result(st, body)?;
    let (profiles, kind): (Vec<LoftRef>, EndKind) = match &link.kind {
        LinkKind::Loft { profiles, .. } => (profiles.clone(), EndKind::Loft),
        LinkKind::Sweep { profile, .. } => (vec![*profile], EndKind::SweepStart),
        _ => return None,
    };
    let solid = &st.bodies.iter().find(|b| b.id == body)?.solid;
    let fi = solid.face_info(face).ok()?;
    if fi.kind != FaceKind::Plane {
        return None;
    }
    let n = fi.normal.normalize();
    let scale = solid.bounding_box().map(|(a, b)| (b - a).length()).unwrap_or(1.0);
    for p in profiles {
        let LoftRef::Region { sketch, region } = p else {
            continue;
        };
        let Some(sd) = st.sketches.iter().find(|s| s.id == sketch) else {
            continue;
        };
        let sn = sd.plane.normal.normalize();
        if sn.cross(n).length() < 1e-6 && (fi.origin - sd.plane.origin).dot(sn).abs() < 1e-6 * (1.0 + scale) {
            return Some(EndFace {
                link: link.id,
                sketch,
                region,
                kind,
            });
        }
    }
    None
}

/// The circle that is the whole profile of an end face (its entity, centre in the sketch and radius), when the profile is one circle.
pub fn end_circle(st: &State, ef: &EndFace) -> Option<(EntityId, DVec2, f64)> {
    let sd = st.sketches.iter().find(|s| s.id == ef.sketch)?;
    let regions = sd.sketch.regions();
    let r = regions.get(ef.region)?;
    if r.border_entities.len() != 1 || !r.holes.is_empty() {
        return None;
    }
    let id = r.border_entities[0];
    match sd.sketch.entity(id)? {
        Entity::Circle { center, radius } => Some((id, *center, *radius)),
        _ => None,
    }
}

/// The outward normal of a face of a body (for the pull of an end face).
pub(crate) fn outward_normal(st: &State, body: BodyId, face: FaceId) -> Option<DVec3> {
    let solid = &st.bodies.iter().find(|b| b.id == body)?.solid;
    Some(solid.face_info(face).ok()?.normal.normalize())
}
